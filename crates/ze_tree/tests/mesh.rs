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
    BarkUvMode, Billboard, BillboardMode, BranchShape, LodDetail, MeshBuffers, Section, Skeleton,
    SkeletonLeaf, TreeOptions, TreePreset, default_lod_levels,
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

/// Checks the default switching distances and branch and leaf settings.
/// Fruit and flower defaults are covered in `tests/cluster_mesh.rs`.
#[test]
fn default_lod_levels_have_the_documented_settings() {
    let levels = default_lod_levels();
    assert_eq!(levels.map(|l| l.distance), [0.0, 100.0, 250.0, 500.0]);
    assert_eq!(levels.map(|l| l.hysteresis), [0.0, 0.05, 0.05, 0.05]);
    assert_eq!(levels[0].detail, LodDetail::default());

    let [_, near, far, farthest] = levels.map(|l| l.detail);
    let bark = |d: LodDetail| (d.section_stride, d.segment_factor, d.branch_shape);
    assert_eq!(bark(near), (3, 0.75, BranchShape::Tube));
    assert_eq!(bark(far), (6, 0.4, BranchShape::Tube));
    assert_eq!(bark(farthest), (8, 1.0, BranchShape::Ribbons));
    assert_eq!(
        levels.map(|l| l.detail.min_branch_radius),
        [0.0, 0.0, 0.0, 0.15]
    );

    let leaves = |d: LodDetail| (d.leaf_stride, d.leaf_scale, d.billboard);
    assert_eq!(leaves(near), (2, 1.25, None));
    assert_eq!(leaves(far), (2, 1.3, Some(Billboard::Single)));
    assert_eq!(leaves(farthest), (4, 1.8, Some(Billboard::Single)));
    // Camera-facing leaf rotation is enabled only for LOD3.
    for detail in [near, far] {
        assert_eq!(detail.leaf_billboard, None);
    }
    assert_eq!(farthest.leaf_billboard, Some(BillboardMode::Camera));
    assert_eq!(farthest.leaf_billboard_scale, 1.25);
}

/// Returns the case's options, a Rust-grown skeleton, and a skeleton using saved ez-tree shapes.
/// The saved shapes replace the grown branch sections, base radii, and leaf placements.
/// Other data, including hierarchy and branch side counts, stays from the Rust-grown skeleton.
fn load_skeletons(case: &str) -> (TreeOptions, Skeleton, Skeleton) {
    let fixture = common::fixture_json(&format!("skeletons/{case}.json"));
    let mut options = TreeOptions::from_json(&fixture["options"].to_string()).expect("options");
    // Match the JavaScript ez-tree fixtures, which use mirrored bark images,
    // instead of ze_tree's default continuous layout.
    options.bark.uv_mode = BarkUvMode::Mirrored;
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
                // Simplified leaves intentionally differ from the ez-tree fixtures:
                // selection starts at each branch tip, and resizing preserves centres.
                // Compare full-detail leaves and the matching bark levels instead.
                let mut parts = vec![("branches", &mesh.branches, tolerances.branch_normal)];
                if lod == 0 {
                    parts.push(("leaves", &mesh.leaves, tolerances.leaf_normal));
                }
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
            assert_eq!(span.shape, level.detail.branch_shape);

            // Omitted thin branches keep empty spans; the trunk is never omitted.
            let branch = &skeleton.branches[i];
            if branch.level > 0 && branch.base_radius < level.detail.min_branch_radius {
                assert!(span.sections.is_empty());
                assert!(span.vertices.is_empty() && span.indices.is_empty());
                continue;
            }
            let rings = span.sections.len() as u32;
            assert_eq!(span.vertices.len() as u32, rings * span.ring_vertices());
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
        let kept = kept_leaves(&skeleton, stride);
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

/// Returns expected leaf indices in skeleton order. On each branch, select
/// every `stride`th leaf starting nearest its tip.
fn kept_leaves(skeleton: &Skeleton, stride: usize) -> Vec<usize> {
    let stride = stride.max(1);
    let mut by_branch: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for (i, leaf) in skeleton.leaves.iter().enumerate() {
        by_branch.entry(leaf.parent.branch).or_default().push(i);
    }
    let along = |i: usize| {
        let parent = &skeleton.leaves[i].parent;
        parent.section as f32 + parent.t
    };
    let mut kept: Vec<usize> = by_branch
        .into_values()
        .flat_map(|mut group| {
            group.sort_by(|&a, &b| along(b).total_cmp(&along(a)).then(a.cmp(&b)));
            group.into_iter().step_by(stride).collect::<Vec<_>>()
        })
        .collect();
    kept.sort_unstable();
    kept
}

/// Checks that tip-first selection keeps each branch’s outermost leaf and
/// rounds the retained count up when `n` leaves are divided by the stride.
#[test]
fn leaf_stride_keeps_each_branch_tip() {
    for preset in [
        TreePreset::AspenSmall,
        TreePreset::OakMedium,
        TreePreset::PineMedium,
    ] {
        let options = preset.options();
        let skeleton = Skeleton::generate(&options).unwrap();
        for stride in [2, 3, 4, 7] {
            let detail = LodDetail {
                leaf_stride: stride,
                ..LodDetail::default()
            };
            let mesh = skeleton.mesh(&options, &detail);
            let kept: Vec<usize> = mesh.leaf_spans.iter().map(|s| s.leaf).collect();
            assert_eq!(
                kept,
                kept_leaves(&skeleton, stride as usize),
                "{preset:?} {stride}"
            );

            let mut counts: std::collections::BTreeMap<usize, (usize, usize)> = Default::default();
            for (i, leaf) in skeleton.leaves.iter().enumerate() {
                counts.entry(leaf.parent.branch).or_default().0 += 1;
                if kept.binary_search(&i).is_ok() {
                    counts.entry(leaf.parent.branch).or_default().1 += 1;
                }
            }
            for (branch, (all, kept_here)) in counts {
                let case = format!("{preset:?} stride {stride} branch {branch}");
                assert_eq!(kept_here, all.div_ceil(stride as usize), "{case}");
                let outermost = (0..skeleton.leaves.len())
                    .filter(|&i| skeleton.leaves[i].parent.branch == branch)
                    .max_by(|&a, &b| {
                        let along = |i: usize| {
                            let p = &skeleton.leaves[i].parent;
                            p.section as f32 + p.t
                        };
                        along(a).total_cmp(&along(b)).then(b.cmp(&a))
                    })
                    .unwrap();
                assert!(kept.binary_search(&outermost).is_ok(), "{case}");
            }
        }
    }
}

/// Checks that enlarging cards preserves their original centres, and that
/// a size multiplier of 1 preserves the original corners.
#[test]
fn scaled_leaves_stay_centred() {
    let options = TreePreset::OakSmall.options();
    let skeleton = Skeleton::generate(&options).unwrap();
    let mesh_at = |leaf_scale| {
        let detail = LodDetail {
            leaf_scale,
            billboard: Some(Billboard::Single),
            ..LodDetail::default()
        };
        skeleton.mesh(&options, &detail).leaves
    };
    let (unscaled, scaled) = (mesh_at(1.0), mesh_at(1.8));
    for (i, leaf) in skeleton.leaves.iter().enumerate() {
        let corners = |buffers: &MeshBuffers| -> Vec<Vec3> {
            (4 * i..4 * i + 4)
                .map(|v| Vec3::from_array(buffers.positions[v]))
                .collect()
        };
        let (a, b) = (corners(&unscaled), corners(&scaled));
        let centre = leaf.origin + leaf.orientation * Vec3::Y * (leaf.size / 2.0);
        let mean = |c: &[Vec3]| c.iter().sum::<Vec3>() / 4.0;
        let tolerance = 1e-4 * leaf.size.max(1.0);
        assert!(mean(&a).abs_diff_eq(centre, tolerance), "leaf {i}");
        assert!(mean(&b).abs_diff_eq(centre, tolerance), "leaf {i}");
        let side = |c: &[Vec3]| c[1].distance(c[2]);
        assert!((side(&a) - leaf.size).abs() < tolerance, "leaf {i}");
        assert!((side(&b) - leaf.size * 1.8).abs() < tolerance, "leaf {i}");
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
            let ring = span.ring_vertices() as usize;
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

/// Checks the lighting directions used by bark and leaf normal maps: tangents
/// are unit length and perpendicular to normals, with handedness +1 or -1.
/// Camera-facing buffers must omit both normals and tangents.
#[test]
fn bark_and_leaf_tangents_are_unit_and_perpendicular() {
    for preset in TreePreset::ALL {
        let mut options = preset.options();
        options.bark.uv_mode = BarkUvMode::Continuous;
        let skeleton = Skeleton::generate(&options).unwrap();
        for level in default_lod_levels() {
            let mesh = skeleton.mesh(&options, &level.detail);
            for buffers in [&mesh.branches, &mesh.leaves] {
                if !buffers.billboard.is_empty() {
                    // Billboard bark has no normals; billboard leaves keep one per card.
                    assert_eq!(buffers.billboard.len(), buffers.vertex_count());
                    assert!(buffers.tangents.is_empty());
                    let leaves = std::ptr::eq(buffers, &mesh.leaves);
                    let expected = if leaves { buffers.vertex_count() } else { 0 };
                    assert_eq!(buffers.normals.len(), expected);
                    continue;
                }
                assert_eq!(buffers.tangents.len(), buffers.vertex_count());
                for (t, n) in buffers.tangents.iter().zip(&buffers.normals) {
                    let xyz = Vec3::new(t[0], t[1], t[2]);
                    assert!((xyz.length() - 1.0).abs() < 1e-5, "{t:?}");
                    assert!(xyz.dot(Vec3::from_array(*n)).abs() < 1e-5, "{t:?} {n:?}");
                    assert!(t[3] == 1.0 || t[3] == -1.0, "{t:?}");
                }
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

/// Checks every preset at the farthest level: thin branches are omitted, the
/// trunk and every fourth leaf remain, and triangle counts fall by more than half.
#[test]
fn farthest_level_culls_thin_branches_and_keeps_leaves() {
    let [.., far, farthest] = default_lod_levels();
    for preset in TreePreset::ALL {
        let options = preset.options();
        let skeleton = Skeleton::generate(&options).unwrap();
        let mesh = skeleton.mesh(&options, &farthest.detail);
        let name = preset.name();

        // Keep the trunk and omit exactly those other branches below the radius threshold.
        assert!(!mesh.branch_spans[0].indices.is_empty(), "{name}");
        for (span, branch) in mesh.branch_spans.iter().zip(&skeleton.branches) {
            let thin = branch.level > 0 && branch.base_radius < farthest.detail.min_branch_radius;
            assert_eq!(
                span.indices.is_empty(),
                thin,
                "{name} branch {}",
                span.branch
            );
        }

        // Branch removal must not reduce the number of leaves selected by leaf_stride.
        let kept = kept_leaves(&skeleton, farthest.detail.leaf_stride as usize);
        assert_eq!(mesh.leaf_spans.len(), kept.len(), "{name}");

        let before = skeleton.mesh(&options, &far.detail).triangle_count();
        let after = mesh.triangle_count();
        assert!(after * 2 < before, "{name}: {after} vs {before} triangles");
    }
}
