//! Create flowers with layered petals and a raised centre, or rounded catkin flowers.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Vec3, Vec4};

use super::{Grid, loop_angle, push_triangle, rgba, smoothstep};
use crate::mesh::{ItemDetail, MeshBuffers};
use crate::{ClusterItem, FlowerOptions};

/// Amount by which each inner layer of petals shrinks relative to the outer layer.
const RING_SHRINK: f32 = 0.15;
/// Minimum inner petal size as a fraction of the outer petal size.
const MIN_RING_SCALE: f32 = 0.3;
/// Bud petal length as a fraction of fully open petal length.
const BUD_LENGTH: f32 = 0.55;
/// Bud petal width as a fraction of fully open petal width.
const BUD_WIDTH: f32 = 0.6;
/// Upward angle at a petal’s base when `cup` is 1, in radians.
const CUP_BASE: f32 = 0.9;
/// Upward angle at a petal’s tip when `cup` is 1, in radians.
const CUP_TIP: f32 = 1.4;
/// Backward tip bend when `curl` is 1, in radians.
const CURL_ANGLE: f32 = 1.6;
/// Upward angle at a closed bud’s petal base, in radians.
const BUD_BASE: f32 = 1.45;
/// Inward-leaning tip angle that closes a bud, in radians.
const BUD_TIP: f32 = 1.95;
/// Downward bend added at a fully faded petal’s base, in radians.
const FADE_BASE: f32 = 0.45;
/// Downward bend added at a fully faded petal’s tip, in radians.
const FADE_TIP: f32 = 1.35;
/// Maximum tip indentation as a fraction of petal length.
const NOTCH_DEPTH: f32 = 0.3;
/// Rounded tip extension beyond the side edges, as a fraction of petal length.
const TIP_ROUND: f32 = 0.08;
/// Height of the flower’s raised centre as a fraction of its radius.
const DOME_HEIGHT: f32 = 0.35;
/// Number of sides around the flower’s raised centre.
const DOME_SEGMENTS: usize = 8;
/// Number of sides around a simplified flower centre.
const SIMPLE_DOME_SEGMENTS: usize = 6;
/// Maximum number of lengthwise divisions in a simplified petal.
const SIMPLE_PETAL_SEGMENTS: usize = 2;
/// Number of sides around each rounded catkin flower.
const BUMP_SEGMENTS: usize = 6;
/// Number of lengthwise divisions of each rounded catkin flower.
const BUMP_RINGS: usize = 4;
/// Number of sides around a simplified rounded catkin flower.
const SIMPLE_BUMP_SEGMENTS: usize = 4;
/// Number of lengthwise divisions in a rounded catkin flower at simple detail.
const SIMPLE_BUMP_RINGS: usize = 3;
/// Number of lengthwise divisions when drawing a single catkin flower at minimal detail.
const MINIMAL_BUMP_RINGS: usize = 2;
/// Number of steps along the petal curve used to position the star’s outline.
const STAR_SEGMENTS: usize = 4;
/// Maximum distance to a notch between star points, as a fraction of petal length.
const STAR_NOTCH: f32 = 0.55;
/// Length-to-width ratio of a rounded catkin flower.
const BUMP_ELONGATION: f32 = 1.4;

/// How far a flower has opened and faded, each measured from 0 to 1.
struct Bloom {
    open: f32,
    fade: f32,
}

impl Bloom {
    /// Converts maturity into opening and fading amounts: 0 is a bud, 0.5 is
    /// fully open, and 1 is fully faded. Values outside 0–1 are clamped.
    fn new(maturity: f32) -> Self {
        Self {
            open: (maturity * 2.0).clamp(0.0, 1.0),
            fade: (maturity * 2.0 - 1.0).clamp(0.0, 1.0),
        }
    }

    /// Returns the colour for this bloom stage, blending from `bud` to `open`
    /// and then to `fade`. Pass colours in linear RGBA.
    fn color(&self, open: Vec4, bud: Vec4, fade: Vec4) -> Vec4 {
        bud.lerp(open, self.open).lerp(fade, self.fade)
    }
}

/// Appends a flower at `item.origin`, facing its local +Y direction. Supply
/// validated options, attachment wind values, and the desired shape detail.
/// Minimal detail uses one star-shaped surface in place of separate petals
/// and a raised centre. Adds nothing for nonpositive item sizes.
pub(super) fn mesh_item(
    out: &mut MeshBuffers,
    item: &ClusterItem,
    flowers: &FlowerOptions,
    wind: [f32; 4],
    detail: ItemDetail,
) {
    let shape = &flowers.shape;
    let bloom = Bloom::new(flowers.maturity);
    let size = item.size;
    if size <= 0.0 {
        return;
    }
    let to_tree = |local: Vec3| item.origin + item.orientation * local;
    let colors = PetalColors {
        base: rgba(shape.base_color),
        petal: rgba(shape.petal_color),
        bud: rgba(shape.bud_color),
        fade: rgba(shape.fade_color),
    };
    let centre_radius = shape.center_size * size / 2.0;
    let facing = item.orientation * Vec3::Y;
    if detail == ItemDetail::Minimal {
        let petal = Petal::layer(shape, &bloom, size, centre_radius, 0, STAR_SEGMENTS);
        let grid = star(&petal, shape, &bloom, &colors, &to_tree);
        grid.emit(out, facing, |_| wind);
        return;
    }

    // Inner layers use smaller petals, rotated to sit between the outer petals.
    let (rings, segments) = match detail {
        ItemDetail::Full => (shape.rings, shape.petal_segments as usize),
        ItemDetail::Simple | ItemDetail::Minimal => (
            shape.rings.min(1),
            (shape.petal_segments as usize).min(SIMPLE_PETAL_SEGMENTS),
        ),
    };
    let count = shape.petal_count;
    for ring in 0..rings {
        let petal = Petal::layer(shape, &bloom, size, centre_radius, ring, segments);
        for p in 0..count {
            let azimuth = TAU * p as f32 / count as f32 + PI * ring as f32 / count as f32;
            let grid = petal.grid(azimuth, shape, &bloom, &colors, &to_tree);
            grid.emit(out, facing, |_| wind);
        }
    }

    // Buds have a smaller raised centre than open flowers.
    let radius = centre_radius * lerp(0.5, 1.0, bloom.open);
    if radius > 0.0 {
        let (rows, sides): (&[f32], usize) = match detail {
            ItemDetail::Full => (&[0.0, 0.7, 1.0], DOME_SEGMENTS),
            ItemDetail::Simple | ItemDetail::Minimal => (&[0.0, 1.0], SIMPLE_DOME_SEGMENTS),
        };
        let color = rgba(shape.center_color);
        let mut grid = Grid::new(sides + 1, true);
        for (i, &f) in rows.iter().enumerate() {
            // Move from the rim at f = 0 to the top of the raised centre at f = 1.
            let angle = f * FRAC_PI_2;
            let (r, y) = (radius * angle.cos(), DOME_HEIGHT * radius * angle.sin());
            let r = if i == rows.len() - 1 { 0.0 } else { r };
            for j in 0..=sides {
                let phi = loop_angle(j, sides);
                let point = to_tree(Vec3::new(r * phi.cos(), y, r * phi.sin()));
                grid.push(point, [j as f32 / sides as f32, f], color);
            }
        }
        grid.emit(out, facing, |_| wind);
    }
}

/// Returns the width and length of a rounded catkin flower in tree units,
/// adjusted for how far the flower has opened.
pub(super) fn catkin_size(item: &ClusterItem, flowers: &FlowerOptions) -> (f32, f32) {
    let size = item.size * lerp(0.5, 1.0, Bloom::new(flowers.maturity).open);
    (size, size * BUMP_ELONGATION)
}

/// Appends one flower as a single triangle in its petal colour for the bloom
/// stage, with corners at three of its outer petal tips, opening toward the
/// item's local +Y. Nonpositive item sizes add nothing.
pub(super) fn mesh_triangle(
    out: &mut MeshBuffers,
    item: &ClusterItem,
    flowers: &FlowerOptions,
    wind: [f32; 4],
) {
    let shape = &flowers.shape;
    let bloom = Bloom::new(flowers.maturity);
    if item.size <= 0.0 {
        return;
    }

    // Put a corner at the tip of every third outer petal position, following the bloom stage.
    let centre_radius = shape.center_size * item.size / 2.0;
    let petal = Petal::layer(shape, &bloom, item.size, centre_radius, 0, STAR_SEGMENTS);
    let corners = [0.0, TAU / 3.0, 2.0 * TAU / 3.0].map(|azimuth| {
        let tip = *petal
            .spine(azimuth, shape, &bloom)
            .last()
            .expect("spine has points");
        item.origin + item.orientation * tip
    });
    let color = bloom.color(
        rgba(shape.petal_color),
        rgba(shape.bud_color),
        rgba(shape.fade_color),
    );
    push_triangle(out, corners, item.orientation * Vec3::Y, color, wind);
}

/// Returns the catkin flower colour in linear RGBA for the current bloom stage.
pub(super) fn catkin_color(flowers: &FlowerOptions) -> Vec4 {
    let shape = &flowers.shape;
    Bloom::new(flowers.maturity).color(
        rgba(shape.petal_color),
        rgba(shape.bud_color),
        rgba(shape.fade_color),
    )
}

/// Appends a rounded catkin flower from its attachment along local +Y.
/// Maturity controls its size and colour; `detail` controls its side and
/// lengthwise division counts. Uses only the shape’s `petal_color`,
/// `bud_color`, and `fade_color`. Adds nothing for nonpositive scaled sizes.
pub(super) fn mesh_catkin_item(
    out: &mut MeshBuffers,
    item: &ClusterItem,
    flowers: &FlowerOptions,
    wind: [f32; 4],
    detail: ItemDetail,
) {
    let (size, height) = catkin_size(item, flowers);
    if size <= 0.0 {
        return;
    }
    let color = catkin_color(flowers);
    let (segments, rings) = match detail {
        ItemDetail::Full => (BUMP_SEGMENTS, BUMP_RINGS),
        ItemDetail::Simple => (SIMPLE_BUMP_SEGMENTS, SIMPLE_BUMP_RINGS),
        ItemDetail::Minimal => (SIMPLE_BUMP_SEGMENTS, MINIMAL_BUMP_RINGS),
    };

    // Form a rounded body with rings that narrow to a point at each end.
    let radius = size / 2.0;
    let mut grid = Grid::new(segments + 1, true);
    for i in 0..=rings {
        let theta = PI * i as f32 / rings as f32;
        let r = if i == 0 || i == rings {
            0.0
        } else {
            radius * theta.sin()
        };
        let y = height / 2.0 * (1.0 - theta.cos());
        for j in 0..=segments {
            let phi = loop_angle(j, segments);
            let local = Vec3::new(r * phi.cos(), y, r * phi.sin());
            let uv = [j as f32 / segments as f32, i as f32 / rings as f32];
            grid.push(item.origin + item.orientation * local, uv, color);
        }
    }
    grid.emit(out, item.orientation * Vec3::Y, |_| wind);
}

/// Creates one star-shaped surface to represent a flower, with a point for
/// each petal and a notch between points. The outline follows the outer petal
/// curve, so closed buds rise into a cone-like shape. The centre colour appears
/// as the flower opens. `to_tree` converts flower coordinates to tree coordinates.
fn star(
    petal: &Petal,
    shape: &crate::FlowerShape,
    bloom: &Bloom,
    colors: &PetalColors,
    to_tree: &impl Fn(Vec3) -> Vec3,
) -> Grid {
    let count = shape.petal_count as usize;
    let open_color = |s: f32| {
        let open = colors.base.lerp(colors.petal, smoothstep(0.0, 0.6, s));
        bloom.color(open, colors.bud, colors.fade)
    };

    // Narrow petals leave deeper notches between the star’s points.
    let widest = petal.root + 0.55 * petal.length;
    let cover = if widest > 0.0 {
        (petal.width * count as f32 / (TAU * widest)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let notch = STAR_NOTCH * cover;

    // Alternate tips and notches around the outline, then join them to the centre.
    let columns = 2 * count;
    let mut grid = Grid::new(columns + 1, true);
    for j in 0..=columns {
        let k = j % columns;
        let azimuth = PI * k as f32 / count as f32;
        let s = if k.is_multiple_of(2) { 1.0 } else { notch };
        let spine = petal.spine(azimuth, shape, bloom);
        grid.push(
            to_tree(spine_point(&spine, s)),
            [j as f32 / columns as f32, 0.0],
            open_color(s),
        );
    }
    // Blend in the centre colour as the bud opens.
    let centre_color = if shape.center_size > 0.0 {
        open_color(0.0).lerp(rgba(shape.center_color), bloom.open)
    } else {
        open_color(0.0)
    };
    for j in 0..=columns {
        grid.push(
            to_tree(Vec3::ZERO),
            [j as f32 / columns as f32, 1.0],
            centre_color,
        );
    }
    grid
}

/// Returns a position along a petal’s centre line, treating its segments as
/// equal lengths. Clamps `s` to 0–1, from the first point to the last.
/// Pass at least two points.
fn spine_point(spine: &[Vec3], s: f32) -> Vec3 {
    let scaled = s.clamp(0.0, 1.0) * (spine.len() - 1) as f32;
    let k = (scaled.floor() as usize).min(spine.len() - 2);
    spine[k].lerp(spine[k + 1], scaled - k as f32)
}

/// Linear RGBA colours for a petal’s base, main surface, bud, and faded stages.
struct PetalColors {
    base: Vec4,
    petal: Vec4,
    bud: Vec4,
    fade: Vec4,
}

/// Dimensions and curve settings shared by the petals in one layer.
/// Lengths use tree units; angles use radians.
struct Petal {
    length: f32,
    width: f32,
    /// Distance from the flower centre to the petal base, in tree units.
    root: f32,
    /// Height of the petal bases above the outer layer, in tree units.
    lift: f32,
    /// Additional upward tilt of this petal layer, in radians.
    rise: f32,
    segments: usize,
}

impl Petal {
    /// Creates petal dimensions and tilt for layer `ring`, counting inward from
    /// 0 at the outer layer. Pass flower size and centre radius in tree units;
    /// `segments` sets the number of divisions along each petal.
    fn layer(
        shape: &crate::FlowerShape,
        bloom: &Bloom,
        size: f32,
        centre_radius: f32,
        ring: u32,
        segments: usize,
    ) -> Self {
        let scale = (1.0 - RING_SHRINK * ring as f32).max(MIN_RING_SCALE);
        Self {
            length: shape.petal_length * size * scale * lerp(BUD_LENGTH, 1.0, bloom.open),
            width: shape.petal_width * size * scale * lerp(BUD_WIDTH, 1.0, bloom.open),
            root: centre_radius * 0.8,
            lift: 0.01 * size * ring as f32,
            rise: 0.1 * ring as f32,
            segments,
        }
    }

    /// Returns the petal’s tilt in radians at the current bloom stage. Pass `s`
    /// from 0 at the base to 1 at the tip; positive angles tilt upward.
    fn angle(&self, s: f32, shape: &crate::FlowerShape, bloom: &Bloom) -> f32 {
        let open = shape.cup * lerp(CUP_BASE, CUP_TIP, s) - shape.curl * CURL_ANGLE * s * s;
        let bud = lerp(BUD_BASE, BUD_TIP, s);
        let fade = lerp(FADE_BASE, FADE_TIP, s);
        lerp(bud, open, bloom.open) + self.rise - bloom.fade * fade
    }

    /// Returns the petal’s centre line from base to tip in flower coordinates.
    /// `azimuth` is the angle in radians around local +Y. The result contains
    /// `segments + 1` points separated by equal-length steps; use at least one segment.
    fn spine(&self, azimuth: f32, shape: &crate::FlowerShape, bloom: &Bloom) -> Vec<Vec3> {
        let out_dir = Vec3::new(azimuth.cos(), 0.0, azimuth.sin());
        let n = self.segments;
        let step = self.length / n as f32;
        let mut spine = vec![out_dir * self.root + Vec3::Y * self.lift];
        for k in 0..n {
            let a = self.angle((k as f32 + 0.5) / n as f32, shape, bloom);
            spine.push(spine[k] + (out_dir * a.cos() + Vec3::Y * a.sin()) * step);
        }
        spine
    }

    /// Builds a petal at `azimuth` radians around the flower’s local +Y axis.
    /// Uses the shape, bloom stage, and colours supplied, and calls `to_tree`
    /// to convert local points to tree coordinates. Requires at least one segment.
    fn grid(
        &self,
        azimuth: f32,
        shape: &crate::FlowerShape,
        bloom: &Bloom,
        colors: &PetalColors,
        to_tree: &impl Fn(Vec3) -> Vec3,
    ) -> Grid {
        let out_dir = Vec3::new(azimuth.cos(), 0.0, azimuth.sin());
        let side = Vec3::Y.cross(out_dir);
        let heading = |s: f32| {
            let a = self.angle(s, shape, bloom);
            out_dir * a.cos() + Vec3::Y * a.sin()
        };

        // Follow the petal’s bend from base to tip to place each row’s centre.
        let n = self.segments;
        let centre = self.spine(azimuth, shape, bloom);

        // Shape the petal’s width and raise its edges into a cup.
        let cupping = lerp(0.8, 0.05 + 0.25 * shape.cup, bloom.open);
        let mut grid = Grid::new(3, false);
        for (k, &middle) in centre.iter().enumerate() {
            let s = k as f32 / n as f32;
            let up = {
                let a = self.angle(s, shape, bloom);
                -out_dir * a.sin() + Vec3::Y * a.cos()
            };
            let half = self.width / 2.0 * outline(s);
            let edge_lift = up * cupping * half;

            // Extend the middle of the tip for a rounded end, or shorten it for an indentation.
            let mut middle = middle;
            if k == n {
                let reach = TIP_ROUND - shape.notch * NOTCH_DEPTH;
                middle += heading(1.0) * reach * self.length;
            }

            let open_color = colors.base.lerp(colors.petal, smoothstep(0.0, 0.6, s));
            let color = bloom.color(open_color, colors.bud, colors.fade);
            for (c, point) in [
                middle - side * half + edge_lift,
                middle,
                middle + side * half + edge_lift,
            ]
            .into_iter()
            .enumerate()
            {
                grid.push(to_tree(point), [c as f32 / 2.0, s], color);
            }
        }
        grid
    }
}

/// Returns the fraction of maximum petal width at `s`, from 0 (base) to
/// 1 (tip). The petal is narrow at the base and widest just past its middle.
fn outline(s: f32) -> f32 {
    let x = (s - 0.55) / 0.55;
    (1.0 - x * x).max(0.0).sqrt().max(0.12)
}

/// Blends from `a` at `t = 0` to `b` at `t = 1`. Values outside 0–1
/// continue beyond the endpoints rather than being clamped.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
