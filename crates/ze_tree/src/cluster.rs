//! Generate the stalks, positions, sizes, and directions of fruit and flower clusters.
//!
//! Clusters attach to existing branches. This pass does not build fruit or flower surfaces.

use std::f32::consts::TAU;

use glam::{Quat, Vec3};

use crate::skeleton::{place, shuffled_indices};
use crate::{
    Attachment, ClusterKind, ClusterPlacement, ClusterShape, Facing, Rng, SkeletonBranch,
    TreeOptions, TreeType,
};

/// Angle between successive items around a stem, about 137.5°.
/// This keeps items from lining up in repeated rows.
const GOLDEN_ANGLE: f32 = 2.399_963;
/// Number of straight sections used to approximate a curved item or side stalk.
const STALK_SEGMENTS: usize = 4;
/// Number of straight sections used to approximate a cluster's main stem.
const AXIS_SEGMENTS: usize = 8;
/// Fraction of the main stem used to turn from the branch toward the cluster direction.
const AXIS_TURN: f32 = 0.25;

/// A group of fruit or flowers attached to one place on a branch.
///
/// Each *item* is one fruit or flower. Positions use tree coordinates,
/// with the trunk base at the origin; lengths use tree units.
#[derive(Debug, Clone, PartialEq)]
pub struct SkeletonCluster {
    /// Branch attachment identifying where this cluster grows.
    pub parent: Attachment,
    /// Starting point of the cluster: on the branch surface for side clusters,
    /// or at the branch end for tip clusters.
    pub origin: Vec3,
    /// Rotation from cluster coordinates to tree coordinates. Local +Y follows
    /// the chosen facing direction after random tilt.
    pub orientation: Quat,
    /// Supporting stalks and main stems. Each appears after any stalk it grows from.
    pub stalks: Vec<ClusterStalk>,
    /// Positions, sizes, and directions of the individual fruit or flowers.
    pub items: Vec<ClusterItem>,
}

/// A supporting stalk or main stem, described by points from its base to its tip.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterStalk {
    /// Points along the stalk in tree coordinates, ordered from base to tip.
    /// Generated stalks have at least two points.
    pub points: Vec<Vec3>,
    /// Thickness of the stalk, measured as a radius in tree units.
    pub radius: f32,
    /// Distance from the cluster origin to this stalk's base, following the
    /// supporting stalks rather than measuring in a straight line.
    pub base_distance: f32,
    /// Supporting stalk's index in [`SkeletonCluster::stalks`]. The supporting
    /// stalk appears earlier in that list. `None` means this stalk starts
    /// directly at the cluster origin.
    pub parent: Option<usize>,
}

/// Position, size, and direction of one fruit or flower.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterItem {
    /// Attachment point of the fruit or flower, in tree coordinates.
    pub origin: Vec3,
    /// Rotation from item coordinates to tree coordinates. Local +Y points
    /// away from the attachment into the item; for hanging fruit, it points down.
    pub orientation: Quat,
    /// Size in tree units after random variation and cluster taper.
    /// Does not include any change for maturity or individual shape settings.
    pub size: f32,
    /// Index of the supporting stalk in [`SkeletonCluster::stalks`].
    /// `None` means the item attaches directly at the cluster origin.
    pub stalk: Option<usize>,
    /// Distance from the cluster origin to the item, measured along its
    /// supporting stalks in tree units.
    pub distance: f32,
}

/// Generates clusters on eligible branches in their existing order.
/// `options` identifies branch levels and whether tips have continuing branches.
pub(crate) fn grow_clusters(
    branches: &[SkeletonBranch],
    options: &TreeOptions,
    placement: &ClusterPlacement,
    shape: &ClusterShape,
    mut rng: Rng,
) -> Vec<SkeletonCluster> {
    let mut clusters = Vec::new();
    if placement.count == 0 && placement.tips <= 0.0 {
        return clusters;
    }

    for (index, branch) in branches.iter().enumerate() {
        if branch.level < placement.min_level {
            continue;
        }
        let sections = &branch.sections;

        // Distribute side clusters along the allowed branch length and around its surface.
        let count = placement.count;
        if count > 0 {
            let radial_offset = rng.next_f32();
            let step = (1.0 - placement.start) / count as f32;
            for (i, slot) in shuffled_indices(&mut rng, count).into_iter().enumerate() {
                let along = placement.start + (i as f32 + rng.next_f32()) * step;
                let at = place(sections, along, index);
                let jitter = rng.range(-0.5, 0.5);
                let angle = TAU * (radial_offset + (slot as f32 + jitter) / count as f32);
                let out = at.orientation * Quat::from_rotation_y(angle) * Vec3::Z;
                let origin = at.origin + out * at.radius;
                let builder = ClusterBuilder::new(placement, shape, &mut rng);
                clusters.push(builder.build(at.attachment, origin, out));
            }
        }

        // Add a possible tip cluster only where no branch continues from the tip.
        let continues =
            options.tree_type == TreeType::Deciduous && branch.level < options.branch.levels;
        if !continues && placement.tips > 0.0 && rng.next_f32() < placement.tips {
            let last = sections.len() - 1;
            let tip = sections[last];
            let attachment = Attachment {
                branch: index,
                section: last,
                t: 0.0,
            };
            let builder = ClusterBuilder::new(placement, shape, &mut rng);
            clusters.push(builder.build(attachment, tip.origin, tip.orientation * Vec3::Y));
        }
    }
    clusters
}

/// Collects stalks and items for one cluster using shared settings and random choices.
struct ClusterBuilder<'a> {
    placement: &'a ClusterPlacement,
    shape: &'a ClusterShape,
    rng: &'a mut Rng,
    stalks: Vec<ClusterStalk>,
    items: Vec<ClusterItem>,
}

impl<'a> ClusterBuilder<'a> {
    fn new(placement: &'a ClusterPlacement, shape: &'a ClusterShape, rng: &'a mut Rng) -> Self {
        Self {
            placement,
            shape,
            rng,
            stalks: Vec::new(),
            items: Vec::new(),
        }
    }

    /// Builds a cluster at a branch attachment. `out` is the unit direction
    /// away from the branch surface, or along the branch at a tip.
    fn build(mut self, parent: Attachment, origin: Vec3, out: Vec3) -> SkeletonCluster {
        // Apply random tilt to the chosen direction, then rotate around that direction.
        let facing = match self.placement.facing {
            Facing::Hang => Vec3::NEG_Y,
            Facing::Up => Vec3::Y,
            Facing::Outward => out,
        };
        let facing = self.tilt(facing, self.placement.facing_jitter.to_radians());
        let orientation = Quat::from_rotation_arc(Vec3::Y, facing)
            * Quat::from_rotation_y(TAU * self.rng.next_f32());

        // Build the supporting stalks and place items in the selected arrangement.
        let count = self.item_count();
        match self.shape.kind {
            ClusterKind::Single => self.single(origin, out, facing),
            ClusterKind::Umbel => self.umbel(origin, out, orientation, count),
            ClusterKind::Raceme => self.raceme(origin, out, facing, count),
            ClusterKind::Panicle => self.panicle(origin, out, facing, count),
            ClusterKind::Catkin => self.catkin(origin, out, facing, count),
        }

        SkeletonCluster {
            parent,
            origin,
            orientation,
            stalks: self.stalks,
            items: self.items,
        }
    }

    /// Places one item on a curved stalk, or directly at `origin` if the stalk
    /// length is zero. The item points along `facing`.
    fn single(&mut self, origin: Vec3, out: Vec3, facing: Vec3) {
        let length = self.shape.stalk_length;
        if length > 0.0 {
            let path = Path::curve(origin, out, facing, 1.0, 0.0, length, STALK_SEGMENTS);
            let end = path.end();
            let stalk = self.add_stalk(path.points, 0.0, None);
            self.add_item(end, facing, 1.0, Some(stalk), length);
        } else {
            self.add_item(origin, facing, 1.0, None, 0.0);
        }
    }

    /// Builds an umbel: item stalks fan out from one point at the configured
    /// spread angle from the cluster direction.
    fn umbel(&mut self, origin: Vec3, out: Vec3, orientation: Quat, count: u32) {
        let (facing, u, v) = (
            orientation * Vec3::Y,
            orientation * Vec3::X,
            orientation * Vec3::Z,
        );
        let spread = self.shape.spread.to_radians();
        let length = self.shape.stalk_length;
        let slot = TAU / count as f32;
        for j in 0..count {
            // Space stalks around the cluster direction with a small random angular offset.
            let azimuth = (j as f32 + self.rng.range(-0.25, 0.25)) * slot;
            let direction = tilted(facing, u, v, spread, azimuth);
            if length > 0.0 {
                let path = Path::curve(origin, out, direction, 1.0, 0.0, length, STALK_SEGMENTS);
                let end = path.end();
                let stalk = self.add_stalk(path.points, 0.0, None);
                self.add_item(end, direction, 1.0, Some(stalk), length);
            } else {
                self.add_item(origin, direction, 1.0, None, 0.0);
            }
        }
    }

    /// Builds a raceme: individually stalked items distributed along one main stem.
    fn raceme(&mut self, origin: Vec3, out: Vec3, facing: Vec3, count: u32) {
        let axis = self.axis(origin, out, facing, self.shape.axis_length);
        let axis_stalk = self.add_stalk(axis.points.clone(), 0.0, None);
        let phase = TAU * self.rng.next_f32();
        for k in 0..count {
            let f = (k as f32 + 0.5) / count as f32;
            let (point, tangent) = axis.at(f);
            let shrink = 1.0 - self.shape.taper * f;
            let azimuth = phase + k as f32 * GOLDEN_ANGLE;
            self.add_stalked_item(point, tangent, azimuth, shrink, axis_stalk, f * axis.length);
        }
    }

    /// Builds a panicle: items distributed among side stems branching from a main stem.
    fn panicle(&mut self, origin: Vec3, out: Vec3, facing: Vec3, count: u32) {
        let shape = self.shape;
        let axis = self.axis(origin, out, facing, shape.axis_length);
        let axis_stalk = self.add_stalk(axis.points.clone(), 0.0, None);
        let phase = TAU * self.rng.next_f32();
        let sides = shape.side_stalks.min(count);
        let spread = shape.spread.to_radians();

        for j in 0..sides {
            // Apply taper and droop to each side stem according to its position on the main stem.
            let f = (j as f32 + 0.5) / sides as f32;
            let (point, tangent) = axis.at(f);
            let shrink = 1.0 - shape.taper * f;
            let (u, v) = tangent.any_orthonormal_pair();
            let direction = tilted(tangent, u, v, spread, phase + j as f32 * GOLDEN_ANGLE);
            let side_length = shape.side_length * shape.axis_length * shrink;
            let side = Path::curve(
                point,
                direction,
                direction,
                1.0,
                shape.droop,
                side_length,
                STALK_SEGMENTS,
            );
            let base_distance = f * axis.length;
            let side_stalk = self.add_stalk(side.points.clone(), base_distance, Some(axis_stalk));

            // Distribute items evenly; give any extra items to stems nearer the cluster base.
            let here = count / sides + u32::from(j < count % sides);
            for k in 0..here {
                let g = (k as f32 + 0.5) / here as f32;
                let (point, tangent) = side.at(g);
                let azimuth = j as f32 + k as f32 * GOLDEN_ANGLE;
                let distance = base_distance + g * side.length;
                self.add_stalked_item(point, tangent, azimuth, shrink, side_stalk, distance);
            }
        }
    }

    /// Builds a catkin: items attach directly along the main stem, after a bare
    /// section whose length is `stalk_length`.
    fn catkin(&mut self, origin: Vec3, out: Vec3, facing: Vec3, count: u32) {
        let shape = self.shape;
        let total = shape.stalk_length + shape.axis_length;
        let axis = self.axis(origin, out, facing, total);
        let axis_stalk = self.add_stalk(axis.points.clone(), 0.0, None);
        let (spread, phase) = (shape.spread.to_radians(), TAU * self.rng.next_f32());
        for k in 0..count {
            let body = (k as f32 + 0.5) / count as f32;
            let distance = shape.stalk_length + body * shape.axis_length;
            let f = if total > 0.0 { distance / total } else { 0.0 };
            let (point, tangent) = axis.at(f);
            let (u, v) = tangent.any_orthonormal_pair();
            let direction = tilted(tangent, u, v, spread, phase + k as f32 * GOLDEN_ANGLE);
            let shrink = 1.0 - shape.taper * body;
            self.add_item(point, direction, shrink, Some(axis_stalk), distance);
        }
    }

    /// Builds the main stem, curving away from the branch toward `facing`
    /// and applying the configured downward bend.
    fn axis(&self, origin: Vec3, out: Vec3, facing: Vec3, length: f32) -> Path {
        Path::curve(
            origin,
            out,
            facing,
            AXIS_TURN,
            self.shape.droop,
            length,
            AXIS_SEGMENTS,
        )
    }

    /// Adds an item on a straight stalk at the configured spread angle from
    /// `tangent`. `shrink` scales both stalk length and item size; `distance`
    /// is the path distance to `point`. With zero stalk length, the item
    /// attaches directly to the supporting stalk identified by `parent`.
    fn add_stalked_item(
        &mut self,
        point: Vec3,
        tangent: Vec3,
        azimuth: f32,
        shrink: f32,
        parent: usize,
        distance: f32,
    ) {
        let (u, v) = tangent.any_orthonormal_pair();
        let direction = tilted(tangent, u, v, self.shape.spread.to_radians(), azimuth);
        let length = self.shape.stalk_length * shrink;
        if length > 0.0 {
            let end = point + direction * length;
            let stalk = self.add_stalk(vec![point, end], distance, Some(parent));
            self.add_item(end, direction, shrink, Some(stalk), distance + length);
        } else {
            self.add_item(point, direction, shrink, Some(parent), distance);
        }
    }

    /// Records a stalk with the configured radius and returns its index.
    fn add_stalk(&mut self, points: Vec<Vec3>, base_distance: f32, parent: Option<usize>) -> usize {
        self.stalks.push(ClusterStalk {
            points,
            radius: self.shape.stalk_radius,
            base_distance,
            parent,
        });
        self.stalks.len() - 1
    }

    /// Records an item pointing along unit vector `direction`. Applies random
    /// size variation, the `shrink` factor, and a random rotation around that direction.
    fn add_item(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        shrink: f32,
        stalk: Option<usize>,
        distance: f32,
    ) {
        let variance = self.placement.size_variance;
        let size = self.placement.size * (1.0 + self.rng.range(-variance, variance)) * shrink;
        let orientation = Quat::from_rotation_arc(Vec3::Y, direction)
            * Quat::from_rotation_y(TAU * self.rng.next_f32());
        self.items.push(ClusterItem {
            origin,
            orientation,
            size,
            stalk,
            distance,
        });
    }

    /// Chooses the item count after random variation and rounding, with a
    /// minimum of one. Single-item clusters always contain exactly one.
    fn item_count(&mut self) -> u32 {
        if self.shape.kind == ClusterKind::Single {
            return 1;
        }
        let variance = self.shape.items_variance;
        let varied = self.shape.items as f32 * (1.0 + self.rng.range(-variance, variance));
        (varied.round() as u32).max(1)
    }

    /// Tilts the unit vector `direction` by a randomly chosen angle from zero
    /// to `max_angle` radians, in a randomly chosen direction around it.
    fn tilt(&mut self, direction: Vec3, max_angle: f32) -> Vec3 {
        let angle = max_angle * self.rng.next_f32();
        let azimuth = TAU * self.rng.next_f32();
        let (u, v) = direction.any_orthonormal_pair();
        tilted(direction, u, v, angle, azimuth)
    }
}

/// Returns a unit direction tilted `angle` radians from `axis`.
/// `azimuth` chooses the direction of tilt in the plane defined by `u` and `v`;
/// these vectors and `axis` must be mutually perpendicular unit vectors.
fn tilted(axis: Vec3, u: Vec3, v: Vec3, angle: f32, azimuth: f32) -> Vec3 {
    let side = u * azimuth.cos() + v * azimuth.sin();
    axis * angle.cos() + side * angle.sin()
}

/// Rotates unit vector `from` toward unit vector `to`: `t = 0` keeps
/// `from`, and `t = 1` reaches `to`.
fn turn_toward(from: Vec3, to: Vec3, t: f32) -> Vec3 {
    Quat::IDENTITY.slerp(Quat::from_rotation_arc(from, to), t) * from
}

/// A stalk path made of equal-length straight sections, each with a unit direction.
struct Path {
    points: Vec<Vec3>,
    headings: Vec<Vec3>,
    length: f32,
}

impl Path {
    /// Approximates a curved stalk with `segments` straight sections totalling
    /// `length`, beginning at `start`. Unit directions `from` and `to` describe
    /// the initial and target headings. `turn` sets the fraction of the path
    /// over which the heading approaches the target; `droop` progressively
    /// turns that target downward. Headings are sampled at section midpoints,
    /// so the first and last sections need not exactly match the endpoint headings.
    fn curve(
        start: Vec3,
        from: Vec3,
        to: Vec3,
        turn: f32,
        droop: f32,
        length: f32,
        segments: usize,
    ) -> Self {
        let step = length / segments as f32;
        let mut points = Vec::with_capacity(segments + 1);
        let mut headings = Vec::with_capacity(segments);
        let mut point = start;
        points.push(point);
        for k in 0..segments {
            // Use the desired direction at the midpoint to set this straight section's heading.
            let f = (k as f32 + 0.5) / segments as f32;
            let target = turn_toward(to, Vec3::NEG_Y, droop * f);
            let heading = turn_toward(from, target, (f / turn).min(1.0));
            point += heading * step;
            points.push(point);
            headings.push(heading);
        }
        Self {
            points,
            headings,
            length,
        }
    }

    /// Returns the point at the tip of the path.
    fn end(&self) -> Vec3 {
        *self.points.last().expect("a path has at least one piece")
    }

    /// Returns a position and unit heading along the path. `f = 0` is the base
    /// and `f = 1` is the tip; values outside that range are clamped.
    fn at(&self, f: f32) -> (Vec3, Vec3) {
        let segments = self.headings.len();
        let scaled = f.clamp(0.0, 1.0) * segments as f32;
        let i = (scaled.floor() as usize).min(segments - 1);
        let point = self.points[i].lerp(self.points[i + 1], scaled - i as f32);
        (point, self.headings[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_turns_from_start_heading_to_end_heading() {
        let path = Path::curve(Vec3::ZERO, Vec3::X, Vec3::NEG_Y, 1.0, 0.0, 4.0, 8);
        assert_eq!(path.points.len(), 9);
        assert!(path.headings[0].dot(Vec3::X) > 0.9);
        assert!(path.headings[7].dot(Vec3::NEG_Y) > 0.9);

        // The straight sections together must have the requested path length.
        let measured: f32 = path.points.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((measured - 4.0).abs() < 1e-4, "{measured}");
    }

    #[test]
    fn droop_bends_the_end_downward() {
        let straight = Path::curve(Vec3::ZERO, Vec3::X, Vec3::X, 1.0, 0.0, 1.0, 4);
        let drooped = Path::curve(Vec3::ZERO, Vec3::X, Vec3::X, 1.0, 1.0, 1.0, 4);
        assert!(straight.end().y.abs() < 1e-6);
        assert!(drooped.end().y < -0.2);
    }

    #[test]
    fn path_at_interpolates_points() {
        let path = Path::curve(Vec3::ZERO, Vec3::Y, Vec3::Y, 1.0, 0.0, 2.0, 4);
        let (point, heading) = path.at(0.5);
        assert!(point.abs_diff_eq(Vec3::new(0.0, 1.0, 0.0), 1e-6));
        assert!(heading.abs_diff_eq(Vec3::Y, 1e-6));
        assert!(path.at(1.0).0.abs_diff_eq(path.end(), 1e-6));
    }

    #[test]
    fn tilted_keeps_unit_length_and_angle() {
        let (u, v) = Vec3::Y.any_orthonormal_pair();
        let d = tilted(Vec3::Y, u, v, 0.5, 1.3);
        assert!((d.length() - 1.0).abs() < 1e-6);
        assert!((d.dot(Vec3::Y) - 0.5_f32.cos()).abs() < 1e-6);
    }
}
