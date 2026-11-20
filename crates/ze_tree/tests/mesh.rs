//! Check Rust meshes against saved meshes from the JavaScript ez-tree generator.
//!
//! Each case uses both the saved skeleton, to isolate meshing differences, and a
//! Rust-grown skeleton, to include the growth differences covered by `tests/skeleton.rs`.
//!
//! Buffer counts and triangle indices must match exactly. Floating-point attributes
//! allow small differences: positions are scaled by the tree's extent, lighting normals
//! use vector distance, and texture coordinates use absolute differences. Here, extent
//! means the farthest reference vertex's distance from the origin, with a minimum of 1.

mod common;

use glam::{Quat, Vec3};
use serde_json::Value;
use ze_tree::{
    BarkUvMode, Billboard, LodDetail, LodLevel, MeshBuffers, Section, Skeleton, SkeletonLeaf,
    TreeOptions, TreePreset, default_lod_levels,
};

/// Allowed differences when Rust meshes the saved ez-tree skeleton.
///
/// Worst seen: position 1.0e-7, branch normal 5.4e-7, leaf normal 5.1e-4, uv 0.
/// Leaf normals need a looser tolerance because rounding errors are amplified on
/// the second square of a double billboard (two crossed squares representing a leaf).
/// With ez-tree's rounded normals, its bottom +X corner has the normal
/// `normalize((1 - size / 2) * face)`. Near leaf size 2, cancellation leaves a
/// nearly zero vector whose direction is sensitive to small errors; at exactly 2,
/// the ideal vector is zero and has no direction.
const MESHING_ONLY: Tolerances = Tolerances {
    position: 1e-6,
    branch_normal: 1e-5,
    leaf_normal: 2e-3,
    uv: 1e-6,
};

/// Allowed differences when Rust both grows and meshes the tree.
/// The position limit matches `tests/skeleton.rs` to allow for growth differences.
///
/// Worst seen: position 5.3e-6, branch normal 8.5e-5, leaf normal 2.5e-4, uv 0.
const GROWN: Tolerances = Tolerances {
    position: 1e-4,
    branch_normal: 5e-4,
    leaf_normal: 2e-3,
    uv: 1e-6,
};

/// Maximum accepted differences for each kind of vertex attribute.
struct Tolerances {
    /// Maximum distance between matching positions, divided by the reference tree's extent.
    position: f32,
    /// Maximum vector distance between matching bark lighting normals.
    branch_normal: f32,
    /// Maximum vector distance between matching leaf lighting normals.
    leaf_normal: f32,
    /// Maximum absolute difference in either texture coordinate, u or v.
    uv: f32,
}

const CASES: [&str; 3] = ["aspen_small", "bush_2", "evergreen_levels_2"];

#[test]
fn default_lod_levels_match_ez_tree() {
    let expected: Vec<LodLevel> =
        serde_json::from_str(&common::fixture_text("lod_levels.json")).expect("lod levels parse");
    // The ez-tree fixture has no fruit or flower settings; exclude those from this comparison.
    let ours = default_lod_levels().map(|mut level| {
        let full = LodDetail::default();
        level.detail.cluster_stride = full.cluster_stride;
        level.detail.item_stride = full.item_stride;
        level.detail.item_scale = full.item_scale;
        level.detail.item_detail = full.item_detail;
        level.detail.flat_item_size = full.flat_item_size;
        level
    });
    assert_eq!(expected, ours);
}

/// Returns the case's options, a Rust-grown skeleton, and a skeleton using saved ez-tree shapes.
/// The saved shapes replace the grown branch sections, base radii, and leaf placements.
/// Other data, including hierarchy and branch side counts, stays from the Rust-grown skeleton.
fn load_skeletons(case: &str) -> (TreeOptions, Skeleton, Skeleton) {
    let fixture = common::fixture_json(&format!("skeletons/{case}.json"));
    let options = TreeOptions::from_json(&fixture["options"].to_string()).expect("options");
    let grown = Skeleton::generate(&options).expect("valid options");

    let mut reference = grown.clone();
    let branches = fixture["branches"].as_array().expect("branches");
    let leaves = fixture["leaves"].as_array().expect("leaves");
    assert_eq!(
        branches.len(),
        reference.branches.len(),
        "{case}: branch count"
    );
    assert_eq!(leaves.len(), reference.leaves.len(), "{case}: leaf count");
    for (ours, theirs) in reference.branches.iter_mut().zip(branches) {
        ours.base_radius = theirs["baseRadius"].as_f64().expect("baseRadius") as f32;
        ours.sections = theirs["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .map(|row| {
                let (origin, orientation, radius) = row_values(row);
                Section {
                    origin,
                    orientation,
                    radius,
                }
            })
            .collect();
    }
    for (ours, theirs) in reference.leaves.iter_mut().zip(leaves) {
        let (origin, orientation, size) = row_values(theirs);
        *ours = SkeletonLeaf {
            origin,
            orientation,
            size,
            ..*ours
        };
    }
    (options, grown, reference)
}

/// Reads a saved row: three position coordinates, four quaternion components, and radius or size.
/// Returns the position, normalized orientation, and final value; requires eight numeric entries.
fn row_values(row: &Value) -> (Vec3, Quat, f32) {
    let v = floats(row);
    assert_eq!(v.len(), 8, "row has 8 values");
    let origin = Vec3::new(v[0], v[1], v[2]);
    let orientation = Quat::from_xyzw(v[3], v[4], v[5], v[6]).normalize();
    (origin, orientation, v[7])
}

fn floats(value: &Value) -> Vec<f32> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|x| x.as_f64().expect("number") as f32)
        .collect()
}

/// Largest attribute differences for one mesh, each paired with the affected vertex index.
#[derive(Default, Debug)]
struct Worst {
    position: (f32, usize),
    normal: (f32, usize),
    uv: (f32, usize),
}

/// Records a larger difference and its vertex index; treats NaN as a failing difference.
fn update(worst: &mut (f32, usize), value: f32, vertex: usize) {
    // Ordinary comparisons ignore NaN. Convert it to infinity so the tolerance check fails.
    let value = if value.is_nan() { f32::INFINITY } else { value };
    if value > worst.0 {
        *worst = (value, vertex);
    }
}

/// Checks buffer counts and triangle indices against a saved mesh, then returns attribute errors.
/// `what` labels assertion failures; `extent` scales position errors and must be positive.
fn compare(what: &str, ours: &MeshBuffers, theirs: &Value, extent: f32) -> Worst {
    let positions = floats(&theirs["positions"]);
    let normals = floats(&theirs["normals"]);
    let uvs = floats(&theirs["uvs"]);
    let indices: Vec<u32> = theirs["indices"]
        .as_array()
        .expect("indices")
        .iter()
        .map(|i| u32::try_from(i.as_u64().expect("index")).expect("fits u32"))
        .collect();

    assert_eq!(
        ours.vertex_count(),
        positions.len() / 3,
        "{what}: vertex count"
    );
    assert_eq!(
        ours.normals.len(),
        normals.len() / 3,
        "{what}: normal count"
    );
    assert_eq!(ours.uvs.len(), uvs.len() / 2, "{what}: uv count");
    assert!(ours.indices == indices, "{what}: indices differ");

    let mut worst = Worst::default();
    for i in 0..ours.vertex_count() {
        let p = Vec3::from_slice(&positions[i * 3..]);
        let n = Vec3::from_slice(&normals[i * 3..]);
        let ours_p = Vec3::from_array(ours.positions[i]);
        let ours_n = Vec3::from_array(ours.normals[i]);
        update(&mut worst.position, ours_p.distance(p) / extent, i);
        update(&mut worst.normal, ours_n.distance(n), i);
        // Check each coordinate separately so a NaN in either one is reported.
        update(&mut worst.uv, (ours.uvs[i][0] - uvs[i * 2]).abs(), i);
        update(&mut worst.uv, (ours.uvs[i][1] - uvs[i * 2 + 1]).abs(), i);
    }
    worst
}

fn check(what: &str, worst: &Worst, tolerances: &Tolerances, normal_tolerance: f32) {
    println!(
        "{what:40} position {:.1e} (v{}) | normal {:.1e} (v{}) | uv {:.1e} (v{})",
        worst.position.0, worst.position.1, worst.normal.0, worst.normal.1, worst.uv.0, worst.uv.1
    );
    for (name, (value, vertex), tolerance) in [
        ("position", worst.position, tolerances.position),
        ("normal", worst.normal, normal_tolerance),
        ("uv", worst.uv, tolerances.uv),
    ] {
        assert!(
            value <= tolerance,
            "{what}: {name} differs by {value} (tolerance {tolerance}) at vertex {vertex}"
        );
    }
}

/// Returns the greatest distance from the origin among saved bark and leaf vertices.
/// A minimum of 1 keeps position-error scaling usable for tiny or empty meshes.
fn extent(fixture: &Value) -> f32 {
    ["branches", "leaves"]
        .iter()
        .flat_map(|part| floats(&fixture[part]["positions"]))
        .collect::<Vec<_>>()
        .chunks(3)
        .map(|p| Vec3::from_slice(p).length())
        .fold(1.0, f32::max)
}

#[test]
fn matches_ez_tree_meshes() {
    for case in CASES {
        let (options, grown, reference) = load_skeletons(case);
        for lod in 0..3 {
            let fixture = common::fixture_json(&format!("meshes/{case}__lod{lod}.json"));
            assert_eq!(fixture["skeleton"], case);
            let detail: LodDetail =
                serde_json::from_value(fixture["detail"].clone()).expect("detail parses");
            let extent = extent(&fixture);

            for (source, skeleton, tolerances) in [
                ("reference", &reference, &MESHING_ONLY),
                ("grown", &grown, &GROWN),
            ] {
                let mesh = skeleton.mesh(&options, &detail);
                let parts = [
                    ("branches", &mesh.branches, tolerances.branch_normal),
                    ("leaves", &mesh.leaves, tolerances.leaf_normal),
                ];
                for (part, ours, normal_tolerance) in parts {
                    let what = format!("{case} lod{lod} {source} {part}");
                    let worst = compare(&what, ours, &fixture[part], extent);
                    check(&what, &worst, tolerances, normal_tolerance);
                }
            }
        }
    }
}

/// Checks that spans cover every buffer entry exactly once and identify the source geometry.
#[test]
fn spans_cover_the_buffers() {
    let options = TreeOptions::default();
    let skeleton = Skeleton::generate(&options).unwrap();
    for level in default_lod_levels() {
        let mesh = skeleton.mesh(&options, &level.detail);
        let stride = level.detail.leaf_stride as usize;

        // Branch spans must cover the vertex and index buffers in order, without gaps or overlap.
        assert_eq!(mesh.branch_spans.len(), skeleton.branches.len());
        let (mut vertex, mut index) = (0, 0);
        for (i, span) in mesh.branch_spans.iter().enumerate() {
            assert_eq!(span.branch, i);
            assert_eq!(span.level, skeleton.branches[i].level);
            assert_eq!((span.vertices.start, span.indices.start), (vertex, index));
            let rings = span.sections.len() as u32;
            assert_eq!(span.vertices.len() as u32, rings * (span.segments + 1));
            assert_eq!(span.indices.len() as u32, (rings - 1) * span.segments * 6);
            assert_eq!(span.sections.first(), Some(&0));
            assert_eq!(
                span.sections.last(),
                Some(&(skeleton.branches[i].sections.len() - 1))
            );
            (vertex, index) = (span.vertices.end, span.indices.end);
        }
        assert_eq!(vertex as usize, mesh.branches.vertex_count());
        assert_eq!(index as usize, mesh.branches.indices.len());

        // Only retained leaves get spans; together they must cover both leaf buffers completely.
        let kept: Vec<usize> = (0..skeleton.leaves.len()).step_by(stride).collect();
        let leaves: Vec<usize> = mesh.leaf_spans.iter().map(|s| s.leaf).collect();
        assert_eq!(leaves, kept);
        let (mut vertex, mut index) = (0, 0);
        for span in &mesh.leaf_spans {
            assert_eq!((span.vertices.start, span.indices.start), (vertex, index));
            (vertex, index) = (span.vertices.end, span.indices.end);
        }
        assert_eq!(vertex as usize, mesh.leaves.vertex_count());
        assert_eq!(index as usize, mesh.leaves.indices.len());
    }
}

#[test]
fn every_index_is_in_range() {
    let options = TreeOptions::default();
    let mesh = Skeleton::generate(&options)
        .unwrap()
        .mesh(&options, &LodDetail::default());
    for buffers in [&mesh.branches, &mesh.leaves] {
        let count = buffers.vertex_count() as u32;
        assert!(buffers.indices.iter().all(|&i| i < count));
        assert_eq!(buffers.indices.len() % 3, 0);
    }
}

/// Returns default tree options with the requested bark texture layout,
/// followed by a skeleton generated from those options.
fn default_tree(uv_mode: BarkUvMode) -> (TreeOptions, Skeleton) {
    let mut options = TreeOptions::default();
    options.bark.uv_mode = uv_mode;
    let skeleton = Skeleton::generate(&options).unwrap();
    (options, skeleton)
}

/// Checks that changing the bark texture layout only changes coordinates
/// along branches, leaving geometry and leaves unchanged at every detail level.
#[test]
fn continuous_uvs_change_only_v() {
    let (mirrored_options, skeleton) = default_tree(BarkUvMode::Mirrored);
    let (continuous_options, _) = default_tree(BarkUvMode::Continuous);
    for level in default_lod_levels() {
        let mirrored = skeleton.mesh(&mirrored_options, &level.detail);
        let continuous = skeleton.mesh(&continuous_options, &level.detail);
        assert_eq!(continuous.leaves, mirrored.leaves);
        assert_eq!(continuous.branch_spans, mirrored.branch_spans);
        let (a, b) = (&continuous.branches, &mirrored.branches);
        assert_eq!(
            (&a.positions, &a.normals, &a.tangents, &a.indices),
            (&b.positions, &b.normals, &b.tangents, &b.indices)
        );
        let u = |m: &MeshBuffers| m.uvs.iter().map(|uv| uv[0]).collect::<Vec<_>>();
        assert_eq!(u(a), u(b));

        // All vertices in a ring keep the original section number as v,
        // including when lower detail skips intermediate sections.
        for span in &continuous.branch_spans {
            let ring = span.segments as usize + 1;
            for (k, &section) in span.sections.iter().enumerate() {
                let first = span.vertices.start as usize + k * ring;
                for uv in &a.uvs[first..first + ring] {
                    assert_eq!(uv[1], section as f32);
                }
            }
        }
    }
}

/// Checks that rounded leaf normals stay unit length on double billboards at
/// leaf size 2, where one corner's offset nearly cancels the face normal.
#[test]
fn rounded_leaf_normals_are_unit_at_size_two() {
    let mut options = TreeOptions::default();
    options.leaves.billboard = Billboard::Double;
    options.leaves.rounded_normals = true;
    let mut skeleton = Skeleton::generate(&options).unwrap();
    for leaf in &mut skeleton.leaves {
        leaf.size = 2.0;
    }
    let mesh = skeleton.mesh(&options, &LodDetail::default());
    assert!(!mesh.leaves.normals.is_empty());
    for n in &mesh.leaves.normals {
        let n = Vec3::from_array(*n);
        assert!(n.is_finite() && (n.length() - 1.0).abs() < 1e-5, "{n:?}");
    }
}

/// Checks that every bark vertex has a tangent of length one lying along
/// the surface, with a valid sign for the second texture direction.
/// Leaf meshes should leave the tangent buffer empty.
#[test]
fn bark_tangents_are_unit_and_perpendicular() {
    for preset in TreePreset::ALL {
        let mut options = preset.options();
        options.bark.uv_mode = BarkUvMode::Continuous;
        let skeleton = Skeleton::generate(&options).unwrap();
        for level in default_lod_levels() {
            let mesh = skeleton.mesh(&options, &level.detail);
            let bark = &mesh.branches;
            assert_eq!(bark.tangents.len(), bark.vertex_count());
            assert!(mesh.leaves.tangents.is_empty());
            for (t, n) in bark.tangents.iter().zip(&bark.normals) {
                let xyz = Vec3::new(t[0], t[1], t[2]);
                assert!((xyz.length() - 1.0).abs() < 1e-5, "{t:?}");
                assert!(xyz.dot(Vec3::from_array(*n)).abs() < 1e-5, "{t:?} {n:?}");
                assert!(t[3] == 1.0 || t[3] == -1.0, "{t:?}");
            }
        }
    }
}

/// Checks that bark tangents point in the same general directions as the
/// texture coordinates on a gently bending, twisting branch.
/// This fixture does not cover sharp bends or twists, where triangle texture
/// directions can differ from the smooth-surface directions of the tangents.
#[test]
fn bark_tangents_follow_the_texture_coordinates() {
    // Advance one unit along the branch, then bend by 0.15 radians and twist
    // by 0.1 radians for the next section.
    let mut sections = Vec::new();
    let (mut origin, mut orientation) = (Vec3::new(1.0, 2.0, 3.0), Quat::from_rotation_x(0.4));
    for i in 0..10 {
        sections.push(Section {
            origin,
            orientation,
            radius: 0.5 - 0.02 * i as f32,
        });
        origin += orientation * Vec3::Y;
        orientation = orientation * Quat::from_rotation_z(0.15) * Quat::from_rotation_y(0.1);
    }
    let skeleton = Skeleton {
        branches: vec![ze_tree::SkeletonBranch {
            level: 0,
            parent: None,
            sections,
            segment_count: 8,
            base_radius: 0.5,
        }],
        ..Skeleton::default()
    };
    let mut options = TreeOptions::default();
    options.bark.uv_mode = BarkUvMode::Continuous;
    options.bark.texture_scale.x = 4.0;

    for section_stride in [1, 3] {
        let detail = LodDetail {
            section_stride,
            ..LodDetail::default()
        };
        let bark = skeleton.mesh(&options, &detail).branches;
        for &tri in bark.indices.as_chunks::<3>().0 {
            let p = tri.map(|i| Vec3::from_array(bark.positions[i as usize]));
            let uv = tri.map(|i| bark.uvs[i as usize]);
            // Derive the directions of increasing u and v from the triangle
            // edges and their texture-coordinate changes.
            let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
            let (du1, dv1) = (uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]);
            let (du2, dv2) = (uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]);
            let det = du1 * dv2 - du2 * dv1;
            let t_tri = ((e1 * dv2 - e2 * dv1) / det).normalize();
            let b_tri = ((e2 * du1 - e1 * du2) / det).normalize();
            for &i in &tri {
                let t = bark.tangents[i as usize];
                let n = Vec3::from_array(bark.normals[i as usize]);
                let xyz = Vec3::new(t[0], t[1], t[2]);
                assert!(xyz.dot(t_tri) > 0.5, "tangent points away from +u");
                assert!(
                    n.cross(xyz).dot(b_tri) * t[3] > 0.5,
                    "bitangent points away from +v"
                );
            }
        }
    }
}
