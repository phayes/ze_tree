//! Check generated trees against saved ez-tree reference skeletons.
//!
//! Branch, section, leaf, and mesh-side counts must match exactly. Floating-point
//! differences are measured relative to tree size, using separate position and
//! rotation checks as estimates of visible movement:
//! - positions use the distance between section or leaf origins;
//! - branch rotations use the angle in radians multiplied by the section radius;
//! - leaf rotations use the angle multiplied by the leaf size.
//!
//! Raw angle comparisons overstate differences on branches thinner than about
//! 1/1000 of the trunk. ez-tree amplifies bending by `1/sqrt(radius)`, and its growth
//! force can turn each section fully upright. The resulting XYZ Euler angles approach
//! gimbal lock, where the X and Z rotations become hard to distinguish. Tiny rounding
//! differences can then change roll by up to 0.05 radians. Scaling by radius accounts
//! for the small visible effect on thin branches; leaf checks account for leaf size.

mod common;

use glam::{Quat, Vec3};
use serde_json::Value;
use ze_tree::{OptionsError, Section, Skeleton, TreeOptions, TreeType};

/// Default limit of 1e-4 for relative position or rotation-based displacement.
/// Outside [`KNOWN_SENSITIVE_CASES`], the largest observed difference is 4.8e-5
/// on a leaf attached to a thin `bush_3` branch; most cases remain below 1e-5.
const DISPLACEMENT_TOLERANCE: f32 = 1e-4;

/// Reference cases that need a larger displacement limit because of rounding sensitivity.
///
/// In `evergreen_seed_42`, level-3 branch 274 starts near its parent's tip with a
/// radius of about 3.5e-5. ez-tree amplifies bends by `1/sqrt(radius)` and the growth
/// force turns each section fully upright, bringing the Euler angles near gimbal lock.
/// This makes the result sensitive to small numeric differences even in ez-tree.
/// The Rust f32 and JavaScript f64 calculations differ in roll by up to 0.05 radians;
/// the leaves inherit that rotation. The largest leaf displacement is 4.2e-4 of the
/// tree's extent, above the usual 1e-4 limit, so this case allows 1e-3. Using f64 for
/// placement moved the sensitivity to other cases rather than eliminating it.
const KNOWN_SENSITIVE_CASES: &[(&str, f32)] = &[("evergreen_seed_42", 1e-3)];
/// Limit on radius error relative to the trunk's base radius.
/// The largest observed fixture error is about 1e-7, below this 1e-6 limit.
const RADIUS_TOLERANCE: f32 = 1e-6;
/// Limit on leaf size error relative to the configured base leaf size.
/// The largest observed fixture error is about 3e-7, below this 1e-6 limit.
const SIZE_TOLERANCE: f32 = 1e-6;

/// Lists fixture names in sorted order, without file extensions, such as `"oak_small"`.
/// Fails the test if the fixture directory is unreadable or empty.
fn case_names() -> Vec<String> {
    let dir = common::fixture_path("skeletons");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("listing {}: {e}", dir.display()))
        .map(|entry| {
            let path = entry.expect("directory entry").path();
            path.file_stem()
                .expect("file stem")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no skeleton fixtures found");
    names
}

/// Loads the named reference skeleton and parses the settings used to generate it.
/// Missing files or invalid fixture options fail the test.
fn load_case(case: &str) -> (Value, TreeOptions) {
    let fixture = common::fixture_json(&format!("skeletons/{case}.json"));
    let options = TreeOptions::from_json(&fixture["options"].to_string())
        .unwrap_or_else(|e| panic!("{case}: options parse: {e}"));
    (fixture, options)
}

/// A fixture row: position, quaternion rotation, and section radius or leaf size.
/// The JSON order is `[ox, oy, oz, qx, qy, qz, qw, value]`.
struct Row {
    origin: Vec3,
    orientation: Quat,
    value: f32,
}

fn row(value: &Value) -> Row {
    let v: Vec<f32> = value
        .as_array()
        .expect("row is an array")
        .iter()
        .map(|x| x.as_f64().expect("row value is a number") as f32)
        .collect();
    assert_eq!(v.len(), 8, "row has 8 values");
    Row {
        origin: Vec3::new(v[0], v[1], v[2]),
        orientation: Quat::from_xyzw(v[3], v[4], v[5], v[6]).normalize(),
        value: v[7],
    }
}

/// Measures the smallest angle between unit rotations in radians.
/// Opposite quaternion signs represent the same rotation and give zero difference.
fn rotation_angle(a: Quat, b: Quat) -> f32 {
    // Use the relative rotation's vector part to retain precision for small differences.
    2.0 * (a.conjugate() * b).xyz().length().min(1.0).asin()
}

/// Tracks the largest error for one measurement and a label identifying its location.
#[derive(Default)]
struct Worst {
    value: f32,
    at: String,
}

impl Worst {
    fn update(&mut self, value: f32, at: impl FnOnce() -> String) {
        // Convert NaN to infinity so an invalid calculation cannot pass a tolerance check.
        if value.is_nan() || value > self.value {
            self.value = if value.is_nan() { f32::INFINITY } else { value };
            self.at = at();
        }
    }

    fn check(&self, case: &str, what: &str, tolerance: f32) {
        assert!(
            self.value <= tolerance,
            "{case}: {what} differs by {} (tolerance {tolerance}) at {}",
            self.value,
            self.at
        );
    }
}

/// Largest relative errors in displacement, branch radius, and leaf size for one tree.
#[derive(Default)]
struct Deviations {
    displacement: Worst,
    radius: Worst,
    size: Worst,
}

/// Checks exact structural agreement with a reference and returns the largest numeric errors.
/// `case` labels failures; `options` supplies the reference radius and leaf size scales.
fn compare(case: &str, fixture: &Value, options: &TreeOptions, skeleton: &Skeleton) -> Deviations {
    let branches = fixture["branches"].as_array().expect("branches array");
    let leaves = fixture["leaves"].as_array().expect("leaves array");

    // Require matching branch, leaf, section, and cross-section side counts.
    assert_eq!(
        skeleton.branches.len(),
        branches.len(),
        "{case}: branch count"
    );
    assert_eq!(skeleton.leaves.len(), leaves.len(), "{case}: leaf count");
    for (i, (ours, theirs)) in skeleton.branches.iter().zip(branches).enumerate() {
        let sections = theirs["sections"].as_array().expect("sections array");
        assert_eq!(
            ours.sections.len(),
            sections.len(),
            "{case}: branch {i} section count"
        );
        assert_eq!(
            u64::from(ours.segment_count),
            theirs["segmentCount"].as_u64().expect("segmentCount"),
            "{case}: branch {i} segment count"
        );
    }

    // Scale position and rotation errors by the farthest reference origin from zero,
    // with a minimum scale of 1. Scale radius errors by the configured trunk radius.
    let extent = branches
        .iter()
        .flat_map(|b| b["sections"].as_array().unwrap())
        .chain(leaves)
        .map(|r| row(r).origin.length())
        .fold(1.0_f32, f32::max);
    let trunk_radius = options.branch.radius[0];

    // Compare branch base radii and each slice's position, rotation, and radius.
    let mut d = Deviations::default();
    for (i, (ours, theirs)) in skeleton.branches.iter().zip(branches).enumerate() {
        let base = theirs["baseRadius"].as_f64().expect("baseRadius") as f32;
        d.radius
            .update((ours.base_radius - base).abs() / trunk_radius, || {
                format!("branch {i} base radius ({} vs {base})", ours.base_radius)
            });
        let sections = theirs["sections"].as_array().unwrap();
        for (j, (s, r)) in ours.sections.iter().zip(sections).enumerate() {
            let at = || format!("branch {i} section {j}");
            compare_section(&mut d, s, &row(r), extent, trunk_radius, at);
        }
    }

    // Check leaf positions and rotation-based displacement separately from size error.
    for (i, (ours, theirs)) in skeleton.leaves.iter().zip(leaves).enumerate() {
        let r = row(theirs);
        let at = || format!("leaf {i}");
        d.displacement
            .update(ours.origin.distance(r.origin) / extent, at);
        let turned = rotation_angle(ours.orientation, r.orientation) * r.value;
        d.displacement
            .update(turned / extent, || format!("leaf {i} orientation"));
        d.size
            .update((ours.size - r.value).abs() / options.leaves.size, at);
    }
    d
}

fn compare_section(
    d: &mut Deviations,
    s: &Section,
    r: &Row,
    extent: f32,
    trunk_radius: f32,
    at: impl Fn() -> String,
) {
    d.displacement
        .update(s.origin.distance(r.origin) / extent, &at);
    let turned = rotation_angle(s.orientation, r.orientation) * r.value.abs();
    d.displacement
        .update(turned / extent, || format!("{} orientation", at()));
    d.radius
        .update((s.radius - r.value).abs() / trunk_radius, &at);
}

#[test]
fn matches_ez_tree_skeletons() {
    for case in case_names() {
        let (fixture, options) = load_case(&case);
        let skeleton = Skeleton::generate(&options).unwrap_or_else(|e| panic!("{case}: {e}"));

        let d = compare(&case, &fixture, &options, &skeleton);
        println!(
            "{case:22} displacement {:.1e} ({}) | radius {:.1e} ({}) | size {:.1e} ({})",
            d.displacement.value,
            d.displacement.at,
            d.radius.value,
            d.radius.at,
            d.size.value,
            d.size.at,
        );
        let displacement_tolerance = KNOWN_SENSITIVE_CASES
            .iter()
            .find(|(name, _)| *name == case)
            .map_or(DISPLACEMENT_TOLERANCE, |&(_, tolerance)| tolerance);
        d.displacement
            .check(&case, "vertex displacement", displacement_tolerance);
        d.radius.check(&case, "radius", RADIUS_TOLERANCE);
        d.size.check(&case, "leaf size", SIZE_TOLERANCE);
    }
}

#[test]
fn hierarchy_is_consistent() {
    for case in case_names() {
        let (_, options) = load_case(&case);
        let skeleton = Skeleton::generate(&options).unwrap();
        let levels = options.branch.levels;

        // Require a root trunk at index 0 and a valid parent for every other branch.
        let trunk = &skeleton.branches[0];
        assert_eq!((trunk.level, trunk.parent), (0, None), "{case}: trunk");

        for (i, branch) in skeleton.branches.iter().enumerate().skip(1) {
            let at = branch
                .parent
                .unwrap_or_else(|| panic!("{case}: branch {i} has no parent"));
            assert!(at.branch < i, "{case}: branch {i} grows before its parent");
            let parent = &skeleton.branches[at.branch];
            assert_eq!(branch.level, parent.level + 1, "{case}: branch {i} level");
            assert!(branch.level <= levels, "{case}: branch {i} is too deep");

            // Reconstruct the attachment position and check the branch base within tolerance.
            let (origin, _) = attachment_point(&parent.sections, at.section, at.t);
            assert!(
                branch.sections[0].origin.abs_diff_eq(origin, 1e-5),
                "{case}: branch {i} starts away from its attachment"
            );
        }

        for (i, leaf) in skeleton.leaves.iter().enumerate() {
            let parent = &skeleton.branches[leaf.parent.branch];
            assert_eq!(
                parent.level, levels,
                "{case}: leaf {i} is not on the deepest level"
            );
            let (origin, _) =
                attachment_point(&parent.sections, leaf.parent.section, leaf.parent.t);
            assert!(
                leaf.origin.abs_diff_eq(origin, 1e-5),
                "{case}: leaf {i} sits away from its attachment"
            );
        }
    }
}

/// Reconstructs an attachment's position and radius from its parent sections.
/// Requires a valid `index` and `t` in 0..=1; the final section stays at the tip.
fn attachment_point(sections: &[Section], index: usize, t: f32) -> (Vec3, f32) {
    assert!((0.0..=1.0).contains(&t), "attachment t {t} out of range");
    let a = &sections[index];
    let b = sections.get(index + 1).unwrap_or(a);
    (
        a.origin.lerp(b.origin, t),
        (1.0 - t) * a.radius + t * b.radius,
    )
}

#[test]
fn generation_is_repeatable() {
    let options = TreeOptions::default();
    assert_eq!(Skeleton::generate(&options), Skeleton::generate(&options));
}

#[test]
fn deciduous_tips_continue_and_evergreen_tips_end() {
    // With side leaves disabled, a deciduous trunk at the deepest configured level
    // still gets one terminal leaf.
    let mut options = TreeOptions::default();
    options.branch.levels = 0;
    options.leaves.count = 0;

    let deciduous = Skeleton::generate(&options).unwrap();
    assert_eq!(deciduous.branches.len(), 1);
    assert_eq!(deciduous.leaves.len(), 1);
    let tip = deciduous.branches[0].sections.last().unwrap();
    assert_eq!(deciduous.leaves[0].origin, tip.origin);

    options.tree_type = TreeType::Evergreen;
    options.branch.levels = 1;
    options.branch.children[0] = 0;
    let evergreen = Skeleton::generate(&options).unwrap();
    assert_eq!(evergreen.branches.len(), 1);
    assert!(evergreen.leaves.is_empty());
    // This evergreen trunk is below the deepest configured level and has no children.
    // Its tip radius is exactly 0, unlike the 0.001 used at the deepest level.
    assert_eq!(evergreen.branches[0].sections.last().unwrap().radius, 0.0);
}

#[test]
fn deciduous_branches_tapered_to_a_point_end_there() {
    // A trunk with no side branches, tapered to a point, has nothing growing from it.
    let mut options = TreeOptions::default();
    options.branch.children[0] = 0;
    options.branch.taper[0] = 1.0;
    let skeleton = Skeleton::generate(&options).unwrap();
    assert_eq!(skeleton.branches.len(), 1);
    assert!(skeleton.leaves.is_empty());

    // Full taper at any level still grows a finite tree.
    for level in 0..3 {
        let mut options = TreeOptions::default();
        options.branch.taper[level] = 1.0;
        let skeleton = Skeleton::generate(&options).unwrap();
        for section in skeleton.branches.iter().flat_map(|b| &b.sections) {
            assert!(
                section.origin.is_finite() && section.orientation.is_finite(),
                "level {level}: {section:?}"
            );
        }
        for leaf in &skeleton.leaves {
            assert!(leaf.origin.is_finite(), "level {level}: {leaf:?}");
        }
    }
}

#[test]
fn invalid_options_are_rejected() {
    let mut options = TreeOptions::default();
    options.branch.sections[0] = 0;
    assert_eq!(
        Skeleton::generate(&options),
        Err(OptionsError::ZeroSections { level: 0 })
    );
}
