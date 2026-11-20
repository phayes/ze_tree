//! Checks shader compilation, validation, and compatibility with the Rust settings.
//! Compares wind movement against a CPU reference based on ez-tree's leaf animation,
//! with this crate's trunk and branch motion added.

use std::collections::HashMap;

use glam::{Vec3, Vec3Swizzles, Vec4, Vec4Swizzles};
use naga::TypeInner;
use wesl::eval::Inputs;
use wesl::resolver::VirtualResolver;
use wesl::{CompileResult, Compiler};
use ze_tree::{Skeleton, TreeOptions, WIND_WESL, WindParams};

/// Compiles a test shader with the wind module importable as `package::wind`.
fn compile(main: &str) -> CompileResult {
    let mut resolver = VirtualResolver::new();
    resolver.add_module("package::wind".parse().unwrap(), WIND_WESL.into());
    resolver.add_module("package::main".parse().unwrap(), main.to_string().into());
    Compiler::default()
        .with_resolver(resolver)
        .compile_module(&"package::main".parse().unwrap())
        .unwrap_or_else(|e| panic!("WESL compile failed: {e}"))
}

/// Minimal renderer example: supply wind settings and per-vertex data, then
/// add the wind displacement to the position.
const VERTEX_MAIN: &str = r#"
import package::wind::{wind_offset, WindParams};

@group(0) @binding(0) var<uniform> wind_params: WindParams;

@vertex
fn main(@location(0) position: vec3<f32>, @location(1) wind: vec4<f32>) -> @builtin(position) vec4<f32> {
    let offset = wind_offset(position, wind, vec3<f32>(0.0), 1.0, wind_params);
    return vec4<f32>(position + offset, 1.0);
}
"#;

fn naga_module() -> naga::Module {
    let wgsl = compile(VERTEX_MAIN).to_string();
    naga::front::wgsl::parse_str(&wgsl).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&wgsl)))
}

#[test]
fn shader_passes_naga_validation() {
    let module = naga_module();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("naga validation failed: {e:?}"));
}

#[test]
fn uniform_layout_matches_to_uniform() {
    let module = naga_module();
    let (members, span) = module
        .types
        .iter()
        .find_map(|(_, ty)| match &ty.inner {
            TypeInner::Struct { members, span }
                if members
                    .iter()
                    .any(|m| m.name.as_deref() == Some("trunk_bend")) =>
            {
                Some((members.clone(), *span))
            }
            _ => None,
        })
        .expect("WindParams struct in the compiled shader");
    assert_eq!(span as usize, size_of::<[f32; 8]>());

    // Distinct field values reveal whether the Rust buffer and shader agree
    // on where each setting is stored.
    let params = WindParams {
        strength: Vec3::new(1.0, 2.0, 3.0),
        frequency: 4.0,
        scale: 5.0,
        trunk_bend: 6.0,
        branch_sway: 7.0,
    };
    let uniform = params.to_uniform();
    let first_value = |name: &str| {
        let member = members
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .unwrap();
        uniform[member.offset as usize / 4]
    };
    assert_eq!(first_value("strength"), 1.0);
    assert_eq!(first_value("frequency"), 4.0);
    assert_eq!(first_value("scale"), 5.0);
    assert_eq!(first_value("trunk_bend"), 6.0);
    assert_eq!(first_value("branch_sway"), 7.0);
}

/// Inputs for one vertex's wind displacement at a chosen time.
struct Case {
    position: Vec3,
    wind: Vec4,
    anchor: Vec3,
    time: f32,
    params: WindParams,
}

/// Evaluates the shader for one case using WESL's interpreter, without a GPU.
fn shader_offset(case: &Case) -> Vec3 {
    let v3 = |v: Vec3| format!("vec3<f32>({:?}, {:?}, {:?})", v.x, v.y, v.z);
    let p = case.params;
    let main = format!(
        r#"
import package::wind::{{wind_offset, WindParams}};

@fragment
fn main() -> @location(0) vec4<f32> {{
    let params = WindParams({}, {:?}, {:?}, {:?}, {:?});
    let wind = vec4<f32>({:?}, {:?}, {:?}, {:?});
    return vec4<f32>(wind_offset({}, wind, {}, {:?}, params), 0.0);
}}
"#,
        v3(p.strength),
        p.frequency,
        p.scale,
        p.trunk_bend,
        p.branch_sway,
        case.wind.x,
        case.wind.y,
        case.wind.z,
        case.wind.w,
        v3(case.position),
        v3(case.anchor),
        case.time,
    );
    let compiled = compile(&main);
    let result = compiled
        .exec("main", Inputs::default(), HashMap::new(), HashMap::new())
        .unwrap_or_else(|e| panic!("WESL exec failed: {e}"));
    let bytes = result.inst.expect("a return value").to_buffer().unwrap();
    let f = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    Vec3::new(f(0), f(1), f(2))
}

// CPU reference for comparison with the shader. The noise and leaf motion follow
// ez-tree's GLSL; trunk and branch motion follow this crate's added wind controls.

fn step3(edge: Vec3, x: Vec3) -> Vec3 {
    Vec3::select(x.cmplt(edge), Vec3::ZERO, Vec3::ONE)
}

fn step4(edge: Vec4, x: Vec4) -> Vec4 {
    Vec4::select(x.cmplt(edge), Vec4::ZERO, Vec4::ONE)
}

fn mod289_3(x: Vec3) -> Vec3 {
    x - (x * (1.0 / 289.0)).floor() * 289.0
}

fn mod289_4(x: Vec4) -> Vec4 {
    x - (x * (1.0 / 289.0)).floor() * 289.0
}

fn permute(x: Vec4) -> Vec4 {
    mod289_4((x * 34.0 + 1.0) * x)
}

fn simplex3(v: Vec3) -> f32 {
    let c = glam::Vec2::new(1.0 / 6.0, 1.0 / 3.0);
    let d = Vec4::new(0.0, 0.5, 1.0, 2.0);

    let mut i = (v + v.dot(Vec3::splat(c.y))).floor();
    let x0 = v - i + i.dot(Vec3::splat(c.x));

    let g = step3(x0.yzx(), x0);
    let l = 1.0 - g;
    let i1 = g.min(l.zxy());
    let i2 = g.max(l.zxy());
    let x1 = x0 - i1 + c.x;
    let x2 = x0 - i2 + c.y;
    let x3 = x0 - d.y;

    i = mod289_3(i);
    let p = permute(
        permute(
            permute(i.z + Vec4::new(0.0, i1.z, i2.z, 1.0)) + i.y + Vec4::new(0.0, i1.y, i2.y, 1.0),
        ) + i.x
            + Vec4::new(0.0, i1.x, i2.x, 1.0),
    );

    // At f32 precision, 1/7 equals the GLSL constant 0.142857142857.
    let n_ = 1.0_f32 / 7.0;
    let ns = n_ * d.wyz() - d.xzx();
    let j = p - 49.0 * (p * ns.z * ns.z).floor();
    let x_ = (j * ns.z).floor();
    let y_ = (j - 7.0 * x_).floor();
    let x = x_ * ns.x + ns.y;
    let y = y_ * ns.x + ns.y;
    let h = 1.0 - x.abs() - y.abs();

    let b0 = Vec4::new(x.x, x.y, y.x, y.y);
    let b1 = Vec4::new(x.z, x.w, y.z, y.w);
    let s0 = b0.floor() * 2.0 + 1.0;
    let s1 = b1.floor() * 2.0 + 1.0;
    let sh = -step4(h, Vec4::ZERO);
    let a0 = b0.xzyw() + s0.xzyw() * sh.xxyy();
    let a1 = b1.xzyw() + s1.xzyw() * sh.zzww();

    let g0 = Vec3::new(a0.x, a0.y, h.x);
    let g1 = Vec3::new(a0.z, a0.w, h.y);
    let g2 = Vec3::new(a1.x, a1.y, h.z);
    let g3 = Vec3::new(a1.z, a1.w, h.w);
    let norm =
        1.792_842_9 - 0.853_734_7 * Vec4::new(g0.dot(g0), g1.dot(g1), g2.dot(g2), g3.dot(g3));
    let (g0, g1, g2, g3) = (g0 * norm.x, g1 * norm.y, g2 * norm.z, g3 * norm.w);

    let m = (0.6 - Vec4::new(x0.dot(x0), x1.dot(x1), x2.dot(x2), x3.dot(x3))).max(Vec4::ZERO);
    let m = m * m;
    42.0 * (m * m).dot(Vec4::new(g0.dot(x0), g1.dot(x1), g2.dot(x2), g3.dot(x3)))
}

/// Reproduces ez-tree's leaf motion, using `weight` for the base-to-tip amount
/// stored in ez-tree's texture coordinate `uv.y`. Keep 2 × 3.14 rather than 2π.
#[allow(clippy::approx_constant)]
fn leaf_flutter(position: Vec3, weight: f32, time: f32, p: &WindParams) -> Vec3 {
    let offset = 2.0 * 3.14 * simplex3(position / p.scale);
    let t = time * p.frequency;
    let sway = 0.5 * (t + offset).sin()
        + 0.3 * (2.0 * t + 1.3 * offset).sin()
        + 0.2 * (5.0 * t + 1.5 * offset).sin();
    weight * p.strength * sway
}

fn cpu_offset(case: &Case) -> Vec3 {
    let p = &case.params;
    let [bend, weight, phase, flutter] = case.wind.to_array();
    let tree_phase = std::f32::consts::TAU * simplex3(case.anchor / p.scale);

    let lean = 0.75 + 0.25 * (0.5 * case.time * p.frequency + tree_phase).sin();
    let trunk = p.strength * (p.trunk_bend * bend * lean);

    let angle = case.time * p.frequency + std::f32::consts::TAU * phase + tree_phase;
    let swing = 0.7 * angle.sin() + 0.3 * (2.3 * angle).sin();
    let branch = p.strength * (p.branch_sway * weight * swing);

    trunk + branch + leaf_flutter(case.position, flutter, case.time, p)
}

fn cases() -> Vec<Case> {
    let layered = WindParams {
        strength: Vec3::new(0.3, 0.05, -0.4),
        frequency: 1.7,
        scale: 9.0,
        trunk_bend: 0.8,
        branch_sway: 0.15,
    };
    vec![
        // Default settings animate this leaf tip as in ez-tree.
        Case {
            position: Vec3::new(3.2, 11.5, -4.75),
            wind: Vec4::new(0.6, 2.5, 0.3, 1.0),
            anchor: Vec3::ZERO,
            time: 12.25,
            params: WindParams::default(),
        },
        // Default settings leave the leaf base still: its flutter weight is zero.
        Case {
            position: Vec3::new(-1.0, 6.0, 2.0),
            wind: Vec4::new(0.3, 1.0, 0.7, 0.0),
            anchor: Vec3::ZERO,
            time: 3.0,
            params: WindParams::default(),
        },
        // Enable trunk, branch, and leaf motion with a nonzero tree anchor.
        Case {
            position: Vec3::new(0.4, 7.9, 1.1),
            wind: Vec4::new(0.45, 3.2, 0.81, 1.0),
            anchor: Vec3::new(25.0, 0.0, -13.0),
            time: 47.5,
            params: layered,
        },
        // Bark follows trunk and branch motion, with no leaf flutter.
        Case {
            position: Vec3::new(1.5, 4.0, -0.5),
            wind: Vec4::new(0.2, 1.4, 0.12, 0.0),
            anchor: Vec3::new(-8.0, 0.0, 30.0),
            time: 0.75,
            params: layered,
        },
    ]
}

#[test]
fn shader_matches_cpu_port() {
    for (n, case) in cases().iter().enumerate() {
        let shader = shader_offset(case);
        let cpu = cpu_offset(case);
        assert!(
            shader.abs_diff_eq(cpu, 1e-4),
            "case {n}: shader {shader}, cpu {cpu}"
        );
    }
}

#[test]
fn defaults_only_move_leaf_tips() {
    let cases = cases();
    assert_ne!(cpu_offset(&cases[0]), Vec3::ZERO);
    assert_eq!(shader_offset(&cases[1]), Vec3::ZERO);
}

#[test]
fn simplex_noise_stays_in_range() {
    // Check that noise varies across a region while staying within its expected range.
    let mut max = 0.0_f32;
    for x in -10..10 {
        for y in -10..10 {
            for z in -3..3 {
                let n = simplex3(Vec3::new(x as f32, y as f32, z as f32) * 0.37);
                max = max.max(n.abs());
            }
        }
    }
    assert!(max > 0.3 && max <= 1.0, "{max}");
}

#[test]
fn leaves_stay_attached_to_their_branches() {
    // A leaf base inherits trunk and branch movement from its attachment point.
    // Check the generated weights and that flutter is disabled at the base.
    let options = TreeOptions::default();
    let skeleton = Skeleton::generate(&options).unwrap();
    let mesh = skeleton.mesh(&options, &Default::default());
    let leaf = &skeleton.leaves[0];
    let span = &mesh.leaf_spans[0];
    let base = mesh.leaves.wind[span.vertices.start as usize + 1];
    assert_eq!(base[3], 0.0);

    let bark = &mesh.branch_spans[leaf.parent.branch];
    let ring = bark
        .sections
        .iter()
        .position(|&s| s == leaf.parent.section)
        .unwrap();
    let ring_start = bark.vertices.start as usize + ring * (bark.segments as usize + 1);
    let bark_wind = mesh.branches.wind[ring_start];
    if leaf.parent.t == 0.0 {
        assert_eq!(base[..3], bark_wind[..3]);
    } else {
        // Attachments between sections interpolate the movement weights and
        // retain the branch's timing offset.
        let next = mesh.branches.wind[ring_start + bark.segments as usize + 1];
        for k in 0..2 {
            let (lo, hi) = (bark_wind[k].min(next[k]), bark_wind[k].max(next[k]));
            assert!((lo..=hi).contains(&base[k]), "component {k}: {base:?}");
        }
        assert_eq!(base[2], bark_wind[2]);
    }
}
