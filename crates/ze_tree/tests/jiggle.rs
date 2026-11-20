//! Check that tree variations respect the requested amount and preserve fixed settings.

use ze_tree::{PerLevel, Rng, Skeleton, TreeOptions, TreePreset};

/// Checks that the change is no larger than the given fraction of the original value.
/// Allows a small tolerance for floating-point calculations.
#[track_caller]
fn assert_near(before: f32, after: f32, amount: f32, what: &str) {
    let limit = before.abs() * amount * (1.0 + 1e-5);
    assert!(
        (after - before).abs() <= limit,
        "{what}: {before} became {after}, more than {amount} away"
    );
}

#[track_caller]
fn assert_levels_near(before: &PerLevel<f32>, after: &PerLevel<f32>, amount: f32, what: &str) {
    for level in 0..4 {
        assert_near(
            before[level],
            after[level],
            amount,
            &format!("{what}[{level}]"),
        );
    }
}

#[test]
fn zero_amount_changes_only_the_seed() {
    let original = TreePreset::OakMedium.options();
    let mut jiggled = original.clone();
    jiggled.jiggle(0.0, &mut Rng::new(1));

    assert_ne!(jiggled.seed, original.seed);
    jiggled.seed = original.seed;
    assert_eq!(jiggled, original);
}

#[test]
fn same_generator_state_gives_same_result() {
    let jiggle = |seed| {
        let mut options = TreePreset::AshLarge.options();
        options.jiggle(0.15, &mut Rng::new(seed));
        options
    };
    assert_eq!(jiggle(9), jiggle(9));
    assert_ne!(jiggle(9), jiggle(10));
}

#[test]
fn every_preset_stays_within_the_amount() {
    let amount = 0.2;
    let mut rng = Rng::new(42);
    for preset in TreePreset::ALL {
        let before = preset.options();
        for _ in 0..25 {
            let mut after = before.clone();
            after.jiggle(amount, &mut rng);
            let (b, a) = (&before.branch, &after.branch);

            // Shape settings stay within 20% of their originals; branch starts and taper stay in 0–1.
            assert_levels_near(&b.angle, &a.angle, amount, "angle");
            assert_levels_near(&b.gnarliness, &a.gnarliness, amount, "gnarliness");
            assert_levels_near(&b.length, &a.length, amount, "length");
            assert_levels_near(&b.radius, &a.radius, amount, "radius");
            assert_levels_near(&b.start, &a.start, amount, "start");
            assert_levels_near(&b.taper, &a.taper, amount, "taper");
            assert_levels_near(&b.twist, &a.twist, amount, "twist");
            assert_near(b.force.strength, a.force.strength, amount, "force");
            for level in 0..4 {
                assert!((0.0..=1.0).contains(&a.start[level]));
                assert!((0.0..=1.0).contains(&a.taper[level]));
            }
            let (bl, al) = (&before.leaves, &after.leaves);
            assert_near(bl.angle, al.angle, amount, "leaf angle");
            assert_near(bl.size, al.size, amount, "leaf size");
            assert_near(bl.size_variance, al.size_variance, amount, "leaf variance");
            assert_near(bl.start, al.start, amount, "leaf start");

            // Allow counts to round up or down to whole numbers after variation.
            // Zero counts stay zero, and non-zero counts stay at least 1.
            let count_ok = |before: u32, after: u32| {
                let reach = (before as f32 * amount).ceil() as u32;
                (before == 0) == (after == 0) && after.abs_diff(before) <= reach
            };
            for level in 0..4 {
                assert!(
                    count_ok(b.children[level], a.children[level]),
                    "children[{level}]"
                );
            }
            assert!(count_ok(bl.count, al.count), "leaf count");

            // Branching levels, mesh detail, growth direction and surface appearance stay unchanged.
            assert_eq!(a.levels, b.levels);
            assert_eq!(a.sections, b.sections);
            assert_eq!(a.segments, b.segments);
            assert_eq!(a.force.direction, b.force.direction);
            assert_eq!(after.tree_type, before.tree_type);
            assert_eq!(after.bark, before.bark);
            assert_eq!(after.trellis, before.trellis);
            assert_eq!(
                (&al.kind, al.billboard, al.tint),
                (&bl.kind, bl.billboard, bl.tint)
            );

            Skeleton::generate(&after).expect("jiggled options are valid");
        }
    }
}

#[test]
#[should_panic(expected = "between 0 and 1")]
fn negative_amount_panics() {
    TreeOptions::default().jiggle(-0.1, &mut Rng::new(0));
}

#[test]
#[should_panic(expected = "between 0 and 1")]
fn amount_above_one_panics() {
    TreeOptions::default().jiggle(1.5, &mut Rng::new(0));
}
