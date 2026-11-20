//! Verify that `Tree` produces the same layout and geometry as generating them
//! separately, matches ez-tree's geometry counts, and handles settings and detail levels.

mod common;

use ze_tree::{
    LodDetail, LodLevel, OptionsError, Skeleton, Tree, TreeError, TreeOptions, TreePreset,
    default_lod_levels,
};

#[test]
fn new_combines_skeleton_and_full_detail_mesh() {
    let options = TreePreset::OakSmall.options();
    let tree = Tree::new(options.clone()).unwrap();

    let skeleton = Skeleton::generate(&options).unwrap();
    let mesh = skeleton.mesh(&options, &LodDetail::default());
    assert_eq!(tree.options(), &options);
    assert_eq!(tree.skeleton(), &skeleton);
    assert_eq!(tree.mesh(), &mesh);
    assert_eq!(tree.vertex_count(), mesh.vertex_count());
    assert_eq!(tree.triangle_count(), mesh.triangle_count());

    let (parts_options, parts_skeleton, parts_mesh) = tree.into_parts();
    assert_eq!(
        (parts_options, parts_skeleton, parts_mesh),
        (options, skeleton, mesh)
    );
}

#[test]
fn counts_match_ez_tree() {
    // Compare against ez-tree's full-detail fixtures (LOD 0), which correspond
    // to the vertex and triangle counts it reports after generating a tree.
    for (case, preset) in [
        ("aspen_small", TreePreset::AspenSmall),
        ("bush_2", TreePreset::Bush2),
    ] {
        let fixture = common::fixture_json(&format!("meshes/{case}__lod0.json"));
        let count = |part: &str, buffer: &str| fixture[part][buffer].as_array().unwrap().len();
        let vertices = (count("branches", "positions") + count("leaves", "positions")) / 3;
        let triangles = (count("branches", "indices") + count("leaves", "indices")) / 3;

        let tree = Tree::from_preset(preset);
        assert_eq!(tree.vertex_count(), vertices, "{case}");
        assert_eq!(tree.triangle_count(), triangles, "{case}");
    }
}

#[test]
fn lod_meshes_are_sorted_by_distance_and_reuse_the_skeleton() {
    let tree = Tree::from_preset(TreePreset::AspenSmall);

    // Supply levels in reverse distance order to check sorting. Give the extra
    // level at distance 100 different leaf detail so the two tied levels can be
    // distinguished when checking that their input order is preserved.
    let mut levels = default_lod_levels().to_vec();
    levels.reverse();
    let mut tie = levels[1];
    tie.detail.leaf_stride = 3;
    levels.push(tie);

    let lods = tree.lod_meshes(&levels);
    let distances: Vec<f32> = lods.iter().map(|(level, _)| level.distance).collect();
    assert_eq!(distances, [0.0, 100.0, 100.0, 250.0]);
    assert_eq!(lods[1].0, levels[1]);
    assert_eq!(lods[2].0, tie);

    for (level, mesh) in &lods {
        assert_eq!(mesh, &tree.mesh_with_detail(&level.detail));
    }
    assert_eq!(&lods[0].1, tree.mesh());
}

#[test]
fn lod_meshes_accepts_no_levels() {
    let tree = Tree::from_preset(TreePreset::Bush1);
    assert!(tree.lod_meshes(&[] as &[LodLevel]).is_empty());
}

#[test]
fn every_preset_builds() {
    for preset in TreePreset::ALL {
        let tree = Tree::from_preset(preset);
        assert!(tree.triangle_count() > 0, "{}", preset.name());
    }
}

#[test]
fn from_json_merges_onto_defaults() {
    let tree = Tree::from_json(r#"{ "seed": 7, "branch": { "levels": 1 } }"#).unwrap();
    let mut expected = TreeOptions {
        seed: 7,
        ..TreeOptions::default()
    };
    expected.branch.levels = 1;
    assert_eq!(tree, Tree::new(expected).unwrap());
}

#[test]
fn from_json_reports_each_kind_of_error() {
    let err = Tree::from_json("not json").unwrap_err();
    assert!(matches!(err, TreeError::Json(_)), "{err:?}");

    let err = Tree::from_json(r#"{ "branch": { "levels": 4 } }"#).unwrap_err();
    assert!(
        matches!(
            err,
            TreeError::Options(OptionsError::TooManyLevels { levels: 4 })
        ),
        "{err:?}"
    );
    assert!(std::error::Error::source(&err).is_some());
}
