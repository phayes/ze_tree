//! Check cluster placement, supporting stalks, and individual fruit and flower sizes.

use glam::Vec3;
use ze_tree::{ClusterKind, Facing, Skeleton, SkeletonCluster, TreeOptions, TreePreset, TreeType};

/// Returns settings for a small broadleaf tree with two fruit clusters on
/// each branch at level 2 or deeper.
fn fruiting() -> TreeOptions {
    let mut options = TreePreset::AshSmall.options();
    options.fruit.placement.count = 2;
    options.fruit.placement.min_level = 2;
    options
}

/// Generates a skeleton and requires at least one fruit or flower cluster.
/// Checks both kinds of cluster for valid positions, orientations, and item sizes.
#[track_caller]
fn grow(options: &TreeOptions) -> Skeleton {
    let skeleton = Skeleton::generate(options).unwrap();
    assert!(
        !skeleton.fruit.is_empty() || !skeleton.flowers.is_empty(),
        "no clusters grew"
    );
    assert_well_formed(&skeleton.fruit);
    assert_well_formed(&skeleton.flowers);
    skeleton
}

/// Returns the cluster's attachment position as a fraction of its parent
/// branch's section count: 0 at the base and 1 at the tip.
fn along(skeleton: &Skeleton, cluster: &SkeletonCluster) -> f32 {
    let parent = cluster.parent;
    let sections = skeleton.branches[parent.branch].sections.len() - 1;
    (parent.section as f32 + parent.t) / sections as f32
}

/// Checks finite positions, unit rotations, and non-negative item sizes and
/// path distances. Stalks need at least two points, and item stalk references
/// must identify an existing stalk.
#[track_caller]
fn assert_well_formed(clusters: &[SkeletonCluster]) {
    for cluster in clusters {
        assert!(cluster.origin.is_finite() && cluster.orientation.is_normalized());
        for stalk in &cluster.stalks {
            assert!(stalk.points.len() >= 2);
            assert!(stalk.points.iter().all(|p| p.is_finite()));
            assert!(stalk.base_distance >= 0.0);
        }
        for item in &cluster.items {
            assert!(item.origin.is_finite() && item.orientation.is_normalized());
            assert!(item.size.is_finite() && item.size >= 0.0);
            assert!(item.distance.is_finite() && item.distance >= 0.0);
            if let Some(stalk) = item.stalk {
                assert!(stalk < cluster.stalks.len());
            }
        }
    }
}

#[test]
fn presets_grow_no_fruit_or_flowers() {
    for preset in TreePreset::ALL {
        let skeleton = Skeleton::generate(&preset.options()).unwrap();
        assert!(skeleton.fruit.is_empty(), "{}", preset.name());
        assert!(skeleton.flowers.is_empty(), "{}", preset.name());
    }
}

#[test]
fn fruit_and_flowers_leave_branches_and_leaves_unchanged() {
    let plain = Skeleton::generate(&TreePreset::OakMedium.options()).unwrap();

    let mut options = TreePreset::OakMedium.options();
    options.fruit.placement.count = 3;
    options.flowers.placement.tips = 1.0;
    let decorated = Skeleton::generate(&options).unwrap();

    assert!(!decorated.fruit.is_empty() && !decorated.flowers.is_empty());
    assert_eq!(decorated.branches, plain.branches);
    assert_eq!(decorated.leaves, plain.leaves);
}

#[test]
fn flowers_do_not_move_fruit() {
    let options = fruiting();
    let mut with_flowers = options.clone();
    with_flowers.flowers.placement.count = 4;
    with_flowers.flowers.cluster.kind = ClusterKind::Raceme;
    with_flowers.flowers.cluster.items = 8;

    let a = Skeleton::generate(&options).unwrap();
    let b = Skeleton::generate(&with_flowers).unwrap();
    assert!(a.flowers.is_empty() && !b.flowers.is_empty());
    assert_eq!(a.fruit, b.fruit);
}

#[test]
fn clusters_are_repeatable_and_follow_the_seed() {
    let options = fruiting();
    let a = Skeleton::generate(&options).unwrap();
    assert_eq!(a, Skeleton::generate(&options).unwrap());

    let mut reseeded = options.clone();
    reseeded.seed += 1;
    assert_ne!(a.fruit, Skeleton::generate(&reseeded).unwrap().fruit);
}

#[test]
fn side_clusters_respect_min_level_start_and_count() {
    let mut options = fruiting();
    options.fruit.placement.start = 0.5;
    let skeleton = grow(&options);
    assert_well_formed(&skeleton.fruit);

    // Each eligible branch should carry the two configured side clusters.
    let eligible = skeleton.branches.iter().filter(|b| b.level >= 2).count();
    assert_eq!(skeleton.fruit.len(), eligible * 2);
    for cluster in &skeleton.fruit {
        assert!(skeleton.branches[cluster.parent.branch].level >= 2);
        assert!(along(&skeleton, cluster) >= 0.5 - 1e-5);
    }
}

#[test]
fn side_clusters_start_on_the_branch_surface() {
    let skeleton = grow(&fruiting());
    for cluster in &skeleton.fruit {
        // The cluster should begin on the surface, one radius from the branch centre.
        let branch = &skeleton.branches[cluster.parent.branch];
        let p = cluster.parent;
        let a = &branch.sections[p.section];
        let b = branch.sections.get(p.section + 1).unwrap_or(a);
        let centre = a.origin.lerp(b.origin, p.t);
        let radius = a.radius + (b.radius - a.radius) * p.t;
        let gap = cluster.origin.distance(centre);
        assert!(
            (gap - radius).abs() < 1e-4 * radius.max(1.0),
            "{gap} vs {radius}"
        );
    }
}

#[test]
fn tips_grow_only_on_branch_ends_without_a_continuation() {
    // Only the deepest branches of a deciduous tree have tips without continuations.
    let mut options = TreePreset::AshSmall.options();
    options.flowers.placement.tips = 1.0;
    options.flowers.placement.min_level = 0;
    let skeleton = grow(&options);
    let levels = options.branch.levels;
    let outer = skeleton
        .branches
        .iter()
        .filter(|b| b.level == levels)
        .count();
    assert_eq!(skeleton.flowers.len(), outer);
    for cluster in &skeleton.flowers {
        let branch = &skeleton.branches[cluster.parent.branch];
        assert_eq!(branch.level, levels);
        assert_eq!(cluster.parent.section, branch.sections.len() - 1);
        assert_eq!(cluster.origin, branch.sections.last().unwrap().origin);
    }

    // Evergreen branches have no tip continuations, so every eligible level can flower.
    let mut options = TreePreset::PineSmall.options();
    assert_eq!(options.tree_type, TreeType::Evergreen);
    options.flowers.placement.tips = 1.0;
    options.flowers.placement.min_level = 1;
    let skeleton = grow(&options);
    let eligible = skeleton.branches.iter().filter(|b| b.level >= 1).count();
    assert_eq!(skeleton.flowers.len(), eligible);
}

#[test]
fn tip_chance_thins_out_tip_clusters() {
    let mut options = TreePreset::OakMedium.options();
    options.flowers.placement.tips = 0.3;
    let skeleton = grow(&options);
    let levels = options.branch.levels;
    let outer = skeleton
        .branches
        .iter()
        .filter(|b| b.level == levels)
        .count();
    let share = skeleton.flowers.len() as f32 / outer as f32;
    assert!((0.2..0.4).contains(&share), "{share} of {outer} tips");
}

#[test]
fn hanging_single_fruit_hangs_below_its_stalk() {
    let mut options = fruiting();
    options.fruit.placement.facing_jitter = 0.0;
    let skeleton = grow(&options);
    for cluster in &skeleton.fruit {
        assert_eq!(cluster.items.len(), 1);
        assert_eq!(cluster.stalks.len(), 1);
        let item = cluster.items[0];
        let stalk = &cluster.stalks[0];

        // The item should point downward at the stalk tip. Its path distance from
        // the branch should equal the stalk length, not the straight-line distance.
        assert!((item.orientation * Vec3::Y).abs_diff_eq(Vec3::NEG_Y, 1e-5));
        assert!((cluster.orientation * Vec3::Y).abs_diff_eq(Vec3::NEG_Y, 1e-5));
        assert_eq!(item.origin, *stalk.points.last().unwrap());
        assert_eq!(stalk.points[0], cluster.origin);
        assert!(item.distance == options.fruit.cluster.stalk_length);

        // The last straight section of the curved stalk should point mostly downward.
        let n = stalk.points.len();
        let last = stalk.points[n - 1] - stalk.points[n - 2];
        assert!(last.normalize().y < -0.9);
    }
}

#[test]
fn facing_jitter_limits_the_tilt() {
    let mut options = fruiting();
    options.fruit.placement.facing = Facing::Up;
    options.fruit.placement.facing_jitter = 20.0;
    let skeleton = grow(&options);
    let mut largest: f32 = 0.0;
    for cluster in &skeleton.fruit {
        let facing = cluster.orientation * Vec3::Y;
        largest = largest.max(facing.angle_between(Vec3::Y).to_degrees());
    }
    assert!(largest <= 20.0 + 1e-3, "{largest}");
    assert!(largest > 5.0, "{largest}");
}

#[test]
fn item_sizes_stay_within_the_variance() {
    let mut options = fruiting();
    options.fruit.placement.size = 2.0;
    options.fruit.placement.size_variance = 0.25;
    let skeleton = grow(&options);
    for item in skeleton.fruit.iter().flat_map(|c| &c.items) {
        assert!(
            (1.5 - 1e-5..=2.5 + 1e-5).contains(&item.size),
            "{}",
            item.size
        );
    }
}

#[test]
fn item_counts_stay_within_the_variance() {
    for kind in [
        ClusterKind::Umbel,
        ClusterKind::Raceme,
        ClusterKind::Panicle,
        ClusterKind::Catkin,
    ] {
        let mut options = fruiting();
        options.fruit.cluster.kind = kind;
        options.fruit.cluster.items = 10;
        options.fruit.cluster.items_variance = 0.3;
        let skeleton = grow(&options);
        assert_well_formed(&skeleton.fruit);

        let counts: Vec<usize> = skeleton.fruit.iter().map(|c| c.items.len()).collect();
        assert!(
            counts.iter().all(|n| (7..=13).contains(n)),
            "{kind:?}: {counts:?}"
        );
        let (min, max) = (counts.iter().min().unwrap(), counts.iter().max().unwrap());
        assert!(min < max, "{kind:?}: counts never vary");
    }
}

#[test]
fn umbel_stalks_fan_out_by_the_spread() {
    let mut options = fruiting();
    options.fruit.cluster.kind = ClusterKind::Umbel;
    options.fruit.cluster.items = 3;
    options.fruit.cluster.spread = 30.0;
    let skeleton = grow(&options);
    for cluster in &skeleton.fruit {
        assert_eq!(cluster.stalks.len(), 3);
        let facing = cluster.orientation * Vec3::Y;
        for (item, stalk) in cluster.items.iter().zip(&cluster.stalks) {
            assert_eq!(stalk.points[0], cluster.origin);
            let direction = item.orientation * Vec3::Y;
            assert!((direction.angle_between(facing).to_degrees() - 30.0).abs() < 1e-2);
        }
    }
}

#[test]
fn raceme_items_line_the_axis_and_taper_toward_the_tip() {
    let mut options = fruiting();
    options.fruit.cluster.kind = ClusterKind::Raceme;
    options.fruit.cluster.items = 6;
    options.fruit.cluster.taper = 0.8;
    options.fruit.placement.size_variance = 0.0;
    let skeleton = grow(&options);
    for cluster in &skeleton.fruit {
        // Expect one main stem and six item stalks, each attached to that stem.
        assert_eq!(cluster.stalks.len(), 7);
        let axis = &cluster.stalks[0];
        assert_eq!(axis.parent, None);
        for (item, stalk) in cluster.items.iter().zip(&cluster.stalks[1..]) {
            assert_eq!(stalk.parent, Some(0));
            assert_eq!(
                item.stalk,
                Some(cluster.stalks.iter().position(|s| s == stalk).unwrap())
            );
            assert!(stalk.base_distance <= options.fruit.cluster.axis_length + 1e-4);
            assert!(axis.points.windows(2).any(|w| {
                let (a, b) = (w[0], w[1]);
                let t = (stalk.points[0] - a).dot(b - a) / (b - a).length_squared();
                (0.0..=1.0).contains(&t) && a.lerp(b, t).distance(stalk.points[0]) < 1e-4
            }));
        }
        let sizes: Vec<f32> = cluster.items.iter().map(|i| i.size).collect();
        assert!(sizes.windows(2).all(|w| w[1] < w[0]), "{sizes:?}");
    }
}

#[test]
fn panicle_shares_items_among_side_stalks() {
    let mut options = fruiting();
    options.fruit.cluster.kind = ClusterKind::Panicle;
    options.fruit.cluster.items = 11;
    options.fruit.cluster.side_stalks = 4;
    let skeleton = grow(&options);
    for cluster in &skeleton.fruit {
        assert_eq!(cluster.items.len(), 11);
        // Expect one main stem, four side stems, and eleven individual item stalks.
        assert_eq!(cluster.stalks.len(), 1 + 4 + 11);
        // The path to an item must be at least as long as the path to its stalk base.
        for item in &cluster.items {
            let stalk = &cluster.stalks[item.stalk.unwrap()];
            assert!(item.distance >= stalk.base_distance);
        }
        // Each item connects to a side stem, and each side stem connects to the main stem.
        assert_eq!(cluster.stalks[0].parent, None);
        let sides: Vec<usize> = (1..cluster.stalks.len())
            .filter(|&k| cluster.stalks[k].parent == Some(0))
            .collect();
        assert_eq!(sides.len(), 4);
        for item in &cluster.items {
            let parent = cluster.stalks[item.stalk.unwrap()].parent.unwrap();
            assert!(sides.contains(&parent));
        }
    }
}

#[test]
fn catkin_items_sit_on_the_axis_after_the_bare_stalk() {
    let mut options = fruiting();
    options.fruit.cluster.kind = ClusterKind::Catkin;
    options.fruit.cluster.items = 30;
    options.fruit.cluster.stalk_length = 0.5;
    options.fruit.cluster.axis_length = 2.0;
    let skeleton = grow(&options);
    for cluster in &skeleton.fruit {
        assert_eq!(cluster.stalks.len(), 1);
        for item in &cluster.items {
            assert_eq!(item.stalk, Some(0));
            assert!((0.5..=2.5).contains(&item.distance), "{}", item.distance);
        }
    }
}

#[test]
fn zero_stalk_length_puts_a_single_item_on_the_branch() {
    let mut options = fruiting();
    options.fruit.cluster.stalk_length = 0.0;
    let skeleton = grow(&options);
    for cluster in &skeleton.fruit {
        assert!(cluster.stalks.is_empty());
        assert_eq!(cluster.items[0].origin, cluster.origin);
        assert_eq!(cluster.items[0].stalk, None);
    }
}
