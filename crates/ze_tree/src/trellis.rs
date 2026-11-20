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

/// Endpoints of a straight trellis guide wire, in tree coordinates.
/// Obtain these from [`TrellisOptions::wires`] to draw the trellis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrellisWire {
    /// Starting point: the left end for horizontal wires, the bottom end for vertical wires.
    pub start: Vec3,
    /// Finishing point: the right end for horizontal wires, the top end for vertical wires.
    pub end: Vec3,
}

impl TrellisOptions {
    /// Returns the endpoints needed to draw the trellis wires. Draw a cylinder
    /// between each pair using [`cylinder_radius`](Self::cylinder_radius).
    ///
    /// Horizontal wires come first, from bottom to top, followed by vertical wires
    /// from left to right. Each wire spans the full width or height. Wires start at
    /// the bottom or left edge and repeat at [`spacing`](Self::spacing) intervals;
    /// no extra wire is added at the far edge for a leftover partial interval.
    ///
    /// The count includes the starting wire and one wire for each full spacing
    /// interval. When the width or height is a whole multiple of the spacing,
    /// both edges have wires. Small rounding errors in decimal values do not
    /// leave out the last wire: a width of 3 with spacing 0.1 gives 31 vertical
    /// wires. Zero spacing produces no wires.
    ///
    /// This method returns wires regardless of [`enabled`](Self::enabled) and
    /// [`visible`](Self::visible). To match ez-tree, draw them only when both are true.
    pub fn wires(&self) -> Vec<TrellisWire> {
        let horizontal = line_count(self.height, self.spacing);
        let vertical = line_count(self.width, self.spacing);
        let half_width = self.width / 2.0;
        let mut wires = Vec::with_capacity(horizontal + vertical);

        // Horizontal wires span the full width at heights 0, spacing, 2 × spacing, ...
        for i in 0..horizontal {
            let y = self.position.y + i as f32 * self.spacing;
            wires.push(TrellisWire {
                start: Vec3::new(self.position.x - half_width, y, self.position.z),
                end: Vec3::new(self.position.x + half_width, y, self.position.z),
            });
        }

        // Vertical wires span the full height, starting at the left edge.
        for i in 0..vertical {
            let x = self.position.x - half_width + i as f32 * self.spacing;
            wires.push(TrellisWire {
                start: Vec3::new(x, self.position.y, self.position.z),
                end: Vec3::new(x, self.position.y + self.height, self.position.z),
            });
        }
        wires
    }

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

/// Counts wires along a width or height (`extent`), including the starting wire.
/// Includes the far-edge wire when a whole number of spacing intervals fits,
/// allowing for small rounding errors as defined by [`WHOLE_TOLERANCE`].
/// Returns 0 if the calculated count is nonpositive or non-finite.
fn line_count(extent: f32, spacing: f32) -> usize {
    let count = (f64::from(extent) / f64::from(spacing) + WHOLE_TOLERANCE).floor() + 1.0;
    if count.is_finite() && count > 0.0 {
        count as usize
    } else {
        0
    }
}

/// Rounding allowance for [`line_count`], measured as a fraction of one spacing
/// interval. A wire this close to the far edge is included even if it falls just beyond it.
const WHOLE_TOLERANCE: f64 = 1e-4;

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
    fn wires_follow_ez_tree_layout() {
        let t = grid();
        let wires = t.wires();
        // Including both edges gives four horizontal and five vertical wires.
        assert_eq!(wires.len(), 9);
        assert_eq!(
            wires[0],
            TrellisWire {
                start: Vec3::new(-1.0, 0.5, -2.0),
                end: Vec3::new(3.0, 0.5, -2.0),
            }
        );
        assert_eq!(wires[3].start.y, 3.5);
        assert_eq!(
            wires[4],
            TrellisWire {
                start: Vec3::new(-1.0, 0.5, -2.0),
                end: Vec3::new(-1.0, 3.5, -2.0),
            }
        );
        assert_eq!(wires[8].start.x, 3.0);
    }

    #[test]
    fn wires_stop_short_of_a_partial_spacing() {
        let t = TrellisOptions {
            width: 2.5,
            height: 0.5,
            ..grid()
        };
        let wires = t.wires();
        // The height is less than one spacing, so only the bottom horizontal wire exists.
        // Three vertical wires fit; the remaining half-spacing adds no wire.
        assert_eq!(wires.len(), 4);
        assert_eq!(wires[0].end.x - wires[0].start.x, 2.5);
        let xs: Vec<f32> = wires[1..].iter().map(|w| w.start.x).collect();
        assert_eq!(xs, [-0.25, 0.75, 1.75]);
    }

    #[test]
    fn line_count_includes_the_far_edge_for_whole_multiples() {
        // Whole numbers of spacing intervals include both edge wires, even when
        // decimal values such as 0.1 cannot be represented exactly in floating point.
        assert_eq!(line_count(3.0, 0.1), 31);
        assert_eq!(line_count(0.3, 0.1), 4);
        assert_eq!(line_count(4.0, 2.0), 3);
        // A leftover interval larger than the rounding allowance adds no wire.
        assert_eq!(line_count(2.5, 1.0), 3);
        assert_eq!(line_count(0.299, 0.1), 3);
        // These inputs cannot produce a positive, finite wire count.
        assert_eq!(line_count(1.0, 0.0), 0);
        assert_eq!(line_count(-3.0, 1.0), 0);
        assert_eq!(line_count(1.0, f32::NAN), 0);
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
