//! Find nearby trellis wires and the bending influence they apply to branches.
//! A trellis is a flat grid of guide wires that attracts growth toward its surface.

use glam::Vec3;

use crate::TrellisOptions;

/// Direction and bending strength returned by [`TrellisOptions::pull`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrellisPull {
    /// Unit direction from the queried position toward the nearest guide wire.
    pub direction: Vec3,
    /// Angular step in radians: positive turns toward the wire, negative turns away.
    /// This can be non-finite for invalid inputs, such as a zero branch radius.
    pub strength: f32,
}

impl TrellisOptions {
    /// Finds the nearest guide-wire point to `position`, in tree coordinates.
    ///
    /// Supply finite coordinates, nonnegative grid dimensions, and positive finite
    /// [`spacing`](Self::spacing). Inputs are not validated.
    /// The grid lies at `self.position.z`; equal distances favour a vertical wire.
    /// If a dimension is not a multiple of spacing, a result may lie on the outer
    /// edge beyond the last regularly spaced wire.
    ///
    /// This query works even when [`enabled`](Self::enabled) is false.
    pub fn nearest_point(&self, position: Vec3) -> Vec3 {
        // Find the rectangle around the grid, parallel to the XY plane.
        let min_x = self.position.x - self.width / 2.0;
        let max_x = self.position.x + self.width / 2.0;
        let min_y = self.position.y;
        let max_y = self.position.y + self.height;

        // Keep the projected X and Y coordinates within the grid bounds.
        let clamped_x = position.x.max(min_x).min(max_x);
        let clamped_y = position.y.max(min_y).min(max_y);

        // Round to a wire in each direction, then clamp to the edge as ez-tree does.
        let h_line_y = ((clamped_y - min_y) / self.spacing).round() * self.spacing + min_y;
        let h_line_y = h_line_y.max(min_y).min(max_y);
        let v_line_x = ((clamped_x - min_x) / self.spacing).round() * self.spacing + min_x;
        let v_line_x = v_line_x.max(min_x).min(max_x);

        // Compare the horizontal and vertical candidates; preserve ez-tree's vertical tie-break.
        let on_h_line = Vec3::new(clamped_x, h_line_y, self.position.z);
        let on_v_line = Vec3::new(v_line_x, clamped_y, self.position.z);
        if position.distance(on_h_line) < position.distance(on_v_line) {
            on_h_line
        } else {
            on_v_line
        }
    }

    /// Calculates trellis influence at `position` for a branch with the given `radius`.
    /// Positions and radii use tree units. Thinner branches receive stronger influence.
    ///
    /// Returns `None` when the nearest wire is farther than `force.max_distance`
    /// or less than 0.001 units away. A returned pull can have zero strength,
    /// including at exactly the maximum distance.
    ///
    /// Use a positive finite radius, maximum distance, and falloff, a finite force
    /// strength, and valid grid settings as described by [`nearest_point`](Self::nearest_point).
    /// Inputs are not validated; a zero radius can yield infinite or NaN strength.
    /// This query ignores [`enabled`](Self::enabled); check it before applying the pull.
    pub fn pull(&self, position: Vec3, radius: f32) -> Option<TrellisPull> {
        let nearest = self.nearest_point(position);
        let distance = position.distance(nearest);
        if distance > self.force.max_distance || distance < 0.001 {
            return None;
        }

        // Reduce influence with distance and increase it for smaller branch radii.
        let distance_factor = 1.0 - (distance / self.force.max_distance).powf(self.force.falloff);
        Some(TrellisPull {
            direction: (nearest - position) / distance,
            strength: self.force.strength * distance_factor / radius,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> TrellisOptions {
        TrellisOptions {
            position: Vec3::new(1.0, 0.5, -2.0),
            width: 4.0,
            height: 3.0,
            spacing: 1.0,
            ..TrellisOptions::default()
        }
    }

    #[test]
    fn nearest_point_snaps_to_closer_wire() {
        let t = grid();
        // The grid spans x = -1..3 and y = 0.5..3.5, with wires every unit.
        // In XY, this point is 0.1 from the y = 1.5 wire and 0.4 from the x = 0 wire.
        // Both candidates have the same Z distance, so the horizontal wire wins.
        let p = t.nearest_point(Vec3::new(0.4, 1.6, 0.0));
        assert!(p.abs_diff_eq(Vec3::new(0.4, 1.5, -2.0), 1e-6), "{p}");
        // Moving closer to x = 0 makes the vertical wire the nearest candidate.
        let p = t.nearest_point(Vec3::new(0.1, 2.0, 0.0));
        assert!(p.abs_diff_eq(Vec3::new(0.0, 2.0, -2.0), 1e-6), "{p}");
    }

    #[test]
    fn nearest_point_clamps_outside_grid() {
        let t = grid();
        let p = t.nearest_point(Vec3::new(10.0, -5.0, -2.0));
        assert!(p.abs_diff_eq(Vec3::new(3.0, 0.5, -2.0), 1e-6), "{p}");
    }

    #[test]
    fn pull_respects_distance_limits() {
        let mut t = grid();
        t.force.strength = 0.5;
        t.force.max_distance = 1.0;
        t.force.falloff = 2.0;

        // A point already on a wire needs no pull.
        assert_eq!(t.pull(Vec3::new(0.0, 2.0, -2.0), 1.0), None);
        // A point beyond the maximum distance receives no pull.
        assert_eq!(t.pull(Vec3::new(0.0, 2.0, 0.0), 1.0), None);

        // At half the maximum distance, check the direction and radius-scaled strength.
        let pull = t.pull(Vec3::new(0.0, 2.0, -1.5), 0.5).expect("in reach");
        assert!(pull.direction.abs_diff_eq(Vec3::NEG_Z, 1e-6));
        let expected = 0.5 * (1.0 - 0.5_f32.powf(2.0)) / 0.5;
        assert!((pull.strength - expected).abs() < 1e-6);
    }
}
