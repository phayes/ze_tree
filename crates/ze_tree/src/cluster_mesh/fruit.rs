//! Create fruit bodies, short stems, and calyxes (the flower remains at the bottom).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Quat, Vec3, Vec4};

use super::{
    Grid, SIMPLE_STALK_SIDES, STALK_SIDES, blush_direction, loop_angle, push_triangle, rgba,
    smoothstep, tube,
};
use crate::mesh::{ItemDetail, MeshBuffers};
use crate::{ClusterItem, FruitOptions};

/// Unripe fruit size as a fraction of fully ripe size.
const UNRIPE_SCALE: f32 = 0.3;
/// Maximum stem hollow depth as a fraction of fruit height.
const DIMPLE_DEPTH: f32 = 0.25;
/// Fraction of fruit length affected by the hollow around the stem.
const DIMPLE_REACH: f32 = 0.45;
/// Maximum sideways stem bend as a fraction of stem length.
const STEM_BOW: f32 = 0.3;
/// Number of points used to describe the curve of a fruit’s short stem.
const STEM_POINTS: usize = 5;
/// Number of small, pointed flower remains forming the calyx.
const SEPALS: u32 = 5;
/// Length of each calyx point as a fraction of fruit size.
const SEPAL_LENGTH: f32 = 0.15;

/// Lower limit for reduced fruit side counts, unless the shape already uses fewer.
const SIMPLE_MIN_SEGMENTS: usize = 6;
/// Lower limit for reduced lengthwise divisions, unless the shape already uses fewer.
const SIMPLE_MIN_RINGS: usize = 4;
/// Maximum side count for a minimal-detail fruit body.
const MINIMAL_SEGMENTS: usize = 5;
/// Maximum number of lengthwise divisions for a minimal-detail fruit body.
const MINIMAL_RINGS: usize = 3;
/// Number of sides around a flat fruit disc.
const DISC_SIDES: usize = 6;
/// Greatest lean of a flat fruit disc away from its facing direction, in radians.
const DISC_LEAN: f32 = 0.6;
/// Brightness of a flat fruit disc's rim relative to its centre.
const DISC_RIM_SHADE: f32 = 0.75;
/// Fractional part of the golden ratio; stepping by it spreads values evenly over 0–1.
const GOLDEN_RATIO_FRACTION: f32 = 0.618_034;

/// Shape detail and colour choices shared by fruit in one cluster.
pub(super) struct Style {
    /// Colour for stems and flower remains, in linear RGBA.
    stalk_color: Vec4,
    /// Unit direction in tree coordinates toward the fruit’s contrasting colour patch.
    blush: Vec3,
    /// Number of sides around a fruit stem; `None` omits the stem.
    stem_sides: Option<usize>,
    /// Number of sides around the fruit body.
    segments: usize,
    /// Number of divisions along the fruit body’s length.
    rings: usize,
    /// Whether to include the small flower remains at the end opposite the stem.
    calyx: bool,
}

impl Style {
    /// Selects shape detail without exceeding the configured side and division
    /// counts. Set `with_stem` to false for directly attached fruit. Pass the
    /// cluster origin in tree coordinates to orient the contrasting colour patch,
    /// and a linear RGBA colour for stems and flower remains.
    pub(super) fn new(
        fruit: &FruitOptions,
        detail: ItemDetail,
        stalk_color: Vec4,
        with_stem: bool,
        origin: Vec3,
    ) -> Self {
        let shape = &fruit.shape;
        let (segments, rings) = (shape.segments as usize, shape.rings as usize);
        let (segments, rings, stem_sides) = match detail {
            ItemDetail::Full => (segments, rings, Some(STALK_SIDES)),
            ItemDetail::Simple => (
                segments.min((segments / 2).max(SIMPLE_MIN_SEGMENTS)),
                rings.min((rings / 2).max(SIMPLE_MIN_RINGS)),
                Some(SIMPLE_STALK_SIDES),
            ),
            ItemDetail::Minimal => (
                segments.min(MINIMAL_SEGMENTS),
                rings.min(MINIMAL_RINGS),
                None,
            ),
        };
        Self {
            stalk_color,
            blush: blush_direction(origin),
            stem_sides: stem_sides.filter(|_| with_stem),
            segments,
            rings,
            calyx: shape.calyx && detail == ItemDetail::Full,
        }
    }
}

/// Returns fruit width and length in tree units, adjusted for maturity.
pub(super) fn body_size(item: &ClusterItem, fruit: &FruitOptions) -> (f32, f32) {
    let size = item.size * (UNRIPE_SCALE + (1.0 - UNRIPE_SCALE) * fruit.maturity);
    (size, size * fruit.shape.elongation)
}

/// Returns the skin colour for the configured maturity in linear RGBA,
/// without the contrasting blush patch.
pub(super) fn body_color(fruit: &FruitOptions) -> Vec4 {
    rgba(fruit.shape.unripe_color).lerp(rgba(fruit.shape.color), fruit.maturity)
}

/// Appends one fruit as a flat, rough disc the size of its body, centred where
/// the body would be. The disc faces roughly along the unit direction `facing`
/// (normally away from the trunk), leaning up to [`DISC_LEAN`] radians in a
/// direction that varies with `index`, the fruit's position in its cluster.
/// Its long axis follows the fruit's own axis. Discs have no stem or calyx.
/// Nonpositive sizes add nothing.
pub(super) fn mesh_disc(
    out: &mut MeshBuffers,
    item: &ClusterItem,
    index: usize,
    fruit: &FruitOptions,
    facing: Vec3,
    wind: [f32; 4],
) {
    let (size, height) = body_size(item, fruit);
    if size <= 0.0 {
        return;
    }

    // Lean each disc a different amount in a different direction away from `facing`.
    let t = (index as f32 * GOLDEN_RATIO_FRACTION).fract();
    let (u, v) = facing.any_orthonormal_pair();
    let turn = TAU * (index as f32 * GOLDEN_RATIO_FRACTION * 2.0).fract();
    let axis = u * turn.cos() + v * turn.sin();
    let normal = Quat::from_axis_angle(axis, DISC_LEAN * t) * facing;

    // Stretch the disc along the fruit's axis as seen in the disc's plane.
    let fruit_axis = item.orientation * Vec3::Y;
    let along = (fruit_axis - normal * normal.dot(fruit_axis))
        .normalize_or(normal.any_orthonormal_vector());
    let across = normal.cross(along);
    let (along, across) = (along * (height / 2.0), across * (size / 2.0));
    let centre = item.origin + fruit_axis * (height / 2.0);

    // A darker rim than centre suggests the fruit's roundness.
    let color = body_color(fruit);
    let rim = (color.truncate() * DISC_RIM_SHADE).extend(color.w);
    let mut grid = Grid::new(DISC_SIDES + 1, true);
    for j in 0..=DISC_SIDES {
        let phi = loop_angle(j, DISC_SIDES);
        let point = centre + along * phi.cos() + across * phi.sin();
        grid.push(point, [j as f32 / DISC_SIDES as f32, 0.0], rim);
    }
    for j in 0..=DISC_SIDES {
        grid.push(centre, [j as f32 / DISC_SIDES as f32, 1.0], color);
    }
    grid.emit(out, normal, |_| wind);
}

/// Adds a main fruit hexagon to `out` and a perpendicular fallback hexagon to
/// `cull`, without normals, stems or flower remains. Pass the outward unit
/// direction as `facing`; `index` varies the stored shape’s tilt as in [`mesh_disc`].
/// Only the main hexagon carries camera-facing data. Its rotated shape is regular
/// and centred on the fruit body, with a diameter equal to the mean of the body’s
/// width and height. A nonpositive body width adds nothing.
pub(super) fn mesh_billboard(
    [out, cull]: [&mut MeshBuffers; 2],
    item: &ClusterItem,
    index: usize,
    fruit: &FruitOptions,
    facing: Vec3,
    wind: [f32; 4],
) {
    let (size, height) = body_size(item, fruit);
    if size <= 0.0 {
        return;
    }

    // Give the fixed fallback shape the same tilt and proportions as a flat fruit disc.
    let t = (index as f32 * GOLDEN_RATIO_FRACTION).fract();
    let (u, v) = facing.any_orthonormal_pair();
    let turn = TAU * (index as f32 * GOLDEN_RATIO_FRACTION * 2.0).fract();
    let lean_axis = u * turn.cos() + v * turn.sin();
    let normal = Quat::from_axis_angle(lean_axis, DISC_LEAN * t) * facing;
    let fruit_axis = item.orientation * Vec3::Y;
    let along = (fruit_axis - normal * normal.dot(fruit_axis))
        .normalize_or(normal.any_orthonormal_vector());
    let across = normal.cross(along);
    let centre = item.origin + fruit_axis * (height / 2.0);
    // Rotate the fallback copy around the first hexagon’s long axis so they cross at 90°.
    let quarter_turn = Quat::from_axis_angle(along, FRAC_PI_2);
    let radius = (size + height) / 4.0;
    let color = body_color(fruit).to_array();

    // Order the corners so triangle fronts face outward from each hexagon.
    for (k, buffers) in [out, cull].into_iter().enumerate() {
        let first = buffers.next_index();
        for j in 0..DISC_SIDES {
            let phi = loop_angle(j, DISC_SIDES);
            let (sin, cos) = phi.sin_cos();
            let flat = along * (cos * height / 2.0) + across * (sin * size / 2.0);
            let point = if k == 0 {
                centre + flat
            } else {
                centre + quarter_turn * flat
            };
            buffers.push_unlit(point, [0.5 + cos / 2.0, 0.5 + sin / 2.0], wind);
            buffers.colors.push(color);
            if k == 0 {
                buffers.push_billboard(centre, Vec3::ZERO, [cos * radius, sin * radius]);
            }
        }
        for j in 1..DISC_SIDES as u32 - 1 {
            buffers.indices.extend([first, first + j, first + j + 1]);
        }
    }
}

/// Appends one fruit with the supplied shape detail and wind values. Pass
/// validated options. The fruit extends from its attachment along local +Y,
/// starting with its stem when present, then its body. Adds nothing when the
/// width after maturity scaling is nonpositive.
pub(super) fn mesh_item(
    out: &mut MeshBuffers,
    item: &ClusterItem,
    fruit: &FruitOptions,
    style: &Style,
    wind: [f32; 4],
) {
    let shape = &fruit.shape;
    let maturity = fruit.maturity;
    let (size, height) = body_size(item, fruit);
    if size <= 0.0 {
        return;
    }
    let to_tree = |local: Vec3| item.origin + item.orientation * local;
    let (stalk_color, blush) = (style.stalk_color, style.blush);

    // Scale the body, stem, and stem hollow to this fruit’s size.
    let radius = size / 2.0;
    let stem = match style.stem_sides {
        Some(_) => shape.stem_length * size,
        None => 0.0,
    };
    let dip = shape.top_dimple * DIMPLE_DEPTH * height;

    // Ripening changes the skin colour and strengthens its contrasting blush patch.
    let base = body_color(fruit);
    let blush_color = rgba(shape.blush_color);
    let blush_amount = shape.blush_amount * maturity;
    let centre = to_tree(Vec3::new(0.0, stem + height / 2.0, 0.0));

    // Build closed rings from the stem end to the bottom of the fruit.
    let (rings, segments) = (style.rings, style.segments);
    let mut grid = Grid::new(segments + 1, true);
    for i in 0..=rings {
        let theta = PI * i as f32 / rings as f32;
        let along = i as f32 / rings as f32;
        // Close both ends at exact points, avoiding tiny gaps from rounding.
        let sine = if i == 0 || i == rings {
            0.0
        } else {
            theta.sin()
        };
        let taper = 1.0 - shape.bottom_taper * smoothstep(0.3, 1.0, along);
        let hollow = (1.0 - along / DIMPLE_REACH).max(0.0).powi(2);
        let y = stem + height / 2.0 * (1.0 - theta.cos()) + dip * hollow;
        for j in 0..=segments {
            let phi = loop_angle(j, segments);
            let groove = (1.0 - (shape.lobes as f32 * phi).cos()) / 2.0;
            let r = radius * sine * taper * (1.0 - shape.lobe_depth * groove);
            let point = to_tree(Vec3::new(r * phi.cos(), y, r * phi.sin()));
            let facing = (point - centre).normalize_or_zero().dot(blush);
            let weight = blush_amount * ((facing + 0.3) / 1.3).clamp(0.0, 1.0).powi(2);
            let color = base.lerp(blush_color, weight);
            grid.push(point, [j as f32 / segments as f32, along], color);
        }
    }
    grid.emit(out, item.orientation * Vec3::Y, |_| wind);

    // Curve the stem from its attachment into the hollow at the top of the fruit.
    let stem_radius = shape.stem_radius * size;
    if let (Some(sides), true) = (style.stem_sides, stem > 0.0) {
        let points: Vec<Vec3> = (0..STEM_POINTS)
            .map(|k| {
                let t = k as f32 / (STEM_POINTS - 1) as f32;
                let bow = shape.stem_curve * STEM_BOW * stem * (PI * t).sin();
                to_tree(Vec3::new(bow, t * (stem + dip), 0.0))
            })
            .collect();
        if let Some(grid) = tube(&points, stem_radius, sides, stalk_color) {
            grid.emit(out, item.orientation * Vec3::Y, |_| wind);
        }
    }

    // Add the star-shaped flower remains at the end opposite the stem.
    if style.calyx {
        let end = stem + height;
        let lift = 0.01 * size;
        let half_width = 0.25 * SEPAL_LENGTH * size;
        let normal = item.orientation * Vec3::Y;
        for k in 0..SEPALS {
            let angle = TAU * k as f32 / SEPALS as f32;
            let out_dir = Vec3::new(angle.cos(), 0.0, angle.sin());
            let side = Vec3::Y.cross(out_dir);
            let root = Vec3::new(0.0, end + lift, 0.0);
            let corners = [
                root + side * half_width,
                root - side * half_width,
                root + out_dir * SEPAL_LENGTH * size,
            ]
            .map(to_tree);
            push_triangle(out, corners, normal, stalk_color, wind);
        }
    }
}
