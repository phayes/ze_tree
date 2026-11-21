//! Checks that tree identifiers are repeatable and reflect changes to settings.
//! Covers presets, saved settings, and the relationship between a tree and its options.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use ze_tree::{ClusterKind, Facing, Tree, TreeOptions, TreePreset, TreeType};

/// Uses the standard library's hasher to test the `Hash` implementations.
/// This hash is separate from the stable identifier returned by `tree_id()`.
fn std_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn equal_options_share_an_id() {
    for preset in TreePreset::ALL {
        let a = preset.options();
        let b = preset.options();
        assert_eq!(a.tree_id(), b.tree_id(), "{}", preset.name());
        assert_eq!(std_hash(&a), std_hash(&b), "{}", preset.name());
    }
}

#[test]
fn presets_have_distinct_ids() {
    let ids: HashSet<u64> = TreePreset::ALL
        .iter()
        .map(|p| p.options().tree_id())
        .collect();
    assert_eq!(ids.len(), TreePreset::ALL.len());
}

#[test]
fn tree_id_matches_its_options() {
    let tree = Tree::from_preset(TreePreset::AshSmall);
    assert_eq!(tree.tree_id(), tree.options().tree_id());
    assert_eq!(std_hash(&tree), std_hash(tree.options()));
}

#[test]
fn json_round_trip_keeps_the_id() {
    for preset in TreePreset::ALL {
        let options = preset.options();
        let restored = TreeOptions::from_json(&options.to_json()).unwrap();
        assert_eq!(options.tree_id(), restored.tree_id(), "{}", preset.name());
    }
}

#[test]
fn every_kind_of_setting_changes_the_id() {
    let base = TreeOptions::default();
    type Edit = (&'static str, fn(&mut TreeOptions));
    let edits: Vec<Edit> = vec![
        ("seed", |o| o.seed += 1),
        ("tree_type", |o| o.tree_type = TreeType::Evergreen),
        ("bark.kind", |o| o.bark.kind.push('x')),
        ("bark.tint", |o| o.bark.tint ^= 1),
        ("bark.texture_scale", |o| o.bark.texture_scale.y += 1.0),
        ("branch.levels", |o| o.branch.levels -= 1),
        ("branch.length[3]", |o| o.branch.length[3] += 0.5),
        ("branch.sections[2]", |o| o.branch.sections[2] += 1),
        ("branch.force.direction", |o| {
            o.branch.force.direction.x = 0.5
        }),
        ("leaves.count", |o| o.leaves.count += 1),
        ("leaves.rounded_normals", |o| {
            o.leaves.rounded_normals ^= true
        }),
        ("leaves.flatten", |o| o.leaves.flatten = 0.5),
        ("trellis.position", |o| o.trellis.position.z += 1.0),
        ("trellis.force.falloff", |o| o.trellis.force.falloff += 1.0),
        ("fruit.placement.facing", |o| {
            o.fruit.placement.facing = Facing::Up
        }),
        ("fruit.cluster.kind", |o| {
            o.fruit.cluster.kind = ClusterKind::Umbel
        }),
        ("fruit.shape.calyx", |o| o.fruit.shape.calyx ^= true),
        ("fruit.maturity", |o| o.fruit.maturity = 0.5),
        ("flowers.shape.fade_color", |o| {
            o.flowers.shape.fade_color ^= 1
        }),
        ("flowers.maturity", |o| o.flowers.maturity = 0.75),
    ];

    let mut ids = HashSet::from([base.tree_id()]);
    for (name, edit) in edits {
        let mut options = base.clone();
        edit(&mut options);
        assert!(
            ids.insert(options.tree_id()),
            "changing {name} did not give a new id"
        );
    }
}

#[test]
fn swapping_values_between_fields_changes_the_id() {
    let mut a = TreeOptions::default();
    a.leaves.size = 1.0;
    a.leaves.size_variance = 2.0;
    let mut b = a.clone();
    b.leaves.size = 2.0;
    b.leaves.size_variance = 1.0;
    assert_ne!(a.tree_id(), b.tree_id());
}

#[test]
fn string_boundaries_are_part_of_the_id() {
    let mut a = TreeOptions::default();
    a.bark.kind = "ab".into();
    a.leaves.kind = "c".into();
    let mut b = a.clone();
    b.bark.kind = "a".into();
    b.leaves.kind = "bc".into();
    assert_ne!(a.tree_id(), b.tree_id());
}

#[test]
fn negative_zero_has_its_own_id() {
    let mut a = TreeOptions::default();
    a.branch.twist[0] = 0.0;
    let mut b = a.clone();
    b.branch.twist[0] = -0.0;
    assert_eq!(a, b);
    assert_ne!(a.tree_id(), b.tree_id());
}
