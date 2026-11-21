//! Vary tree settings to grow trees that look similar without being identical.

use crate::{PerLevel, Rng, TreeOptions};

impl TreeOptions {
    /// Varies this tree's shape to create another tree with a similar appearance.
    ///
    /// `amount` controls how much each setting can change: 0.1 allows changes of
    /// up to 10% in either direction, before rounding counts to whole numbers.
    /// Try 0.05–0.2 for trees that still look like the same species.
    /// A new random seed controls branch and leaf placement, even when `amount` is 0.
    ///
    /// Variation affects branch shape, length, thickness and number, the strength
    /// of the force that guides growth, the pull toward level growth, and leaf
    /// placement, size and number. Multi-stemmed trees also vary their stem count
    /// and tilt, keeping at least two stems; a single-trunk tree keeps one trunk.
    /// Zero values stay zero, and non-zero counts stay at least 1. Start positions
    /// and taper stay between 0 and 1. The number of branching levels, mesh detail,
    /// textures, colours, tree type and trellis stay unchanged.
    ///
    /// To reproduce a variation, use the same starting options, `amount` and
    /// random generator state.
    ///
    /// # Panics
    ///
    /// Panics unless `amount` is between 0 and 1 inclusive. Larger amounts could
    /// make lengths, radii and sizes negative.
    pub fn jiggle(&mut self, amount: f32, rng: &mut Rng) {
        assert!(
            (0.0..=1.0).contains(&amount),
            "jiggle amount must be between 0 and 1, got {amount}"
        );
        // Pick a seed to vary where branches and leaves grow.
        self.seed = rng.next_u32() as i32;

        let mut scale = |value: f32| value * (1.0 + rng.range(-amount, amount));

        // Vary branch shape while keeping start positions and taper between 0 and 1.
        let branch = &mut self.branch;
        jiggle_levels(&mut branch.angle, &mut scale);
        jiggle_levels(&mut branch.gnarliness, &mut scale);
        jiggle_levels(&mut branch.length, &mut scale);
        jiggle_levels(&mut branch.radius, &mut scale);
        jiggle_levels(&mut branch.start, &mut |v| scale(v).clamp(0.0, 1.0));
        jiggle_levels(&mut branch.taper, &mut |v| scale(v).clamp(0.0, 1.0));
        jiggle_levels(&mut branch.twist, &mut scale);
        jiggle_levels(&mut branch.flatten, &mut scale);
        branch.force.strength = scale(branch.force.strength);

        // Vary where leaves grow, their angle and their size.
        let leaves = &mut self.leaves;
        leaves.angle = scale(leaves.angle);
        leaves.start = scale(leaves.start).clamp(0.0, 1.0);
        leaves.size = scale(leaves.size);
        leaves.size_variance = scale(leaves.size_variance);
        leaves.flatten = scale(leaves.flatten).clamp(0.0, 1.0);

        // Vary branch and leaf counts, rounding the results to whole numbers.
        for level in 0..4 {
            branch.children[level] = jiggle_count(branch.children[level], amount, rng);
        }
        leaves.count = jiggle_count(leaves.count, amount, rng);

        // Vary multi-stem trees only, so a single trunk never splits into several.
        if branch.stems > 1 {
            branch.stems = jiggle_count(branch.stems, amount, rng).max(2);
            branch.stem_angle = (branch.stem_angle * (1.0 + rng.range(-amount, amount))).min(90.0);
        }
    }
}

/// Updates the setting for each of the four branching levels, in order.
fn jiggle_levels(values: &mut PerLevel<f32>, f: &mut impl FnMut(f32) -> f32) {
    for value in &mut values.0 {
        *value = f(*value);
    }
}

/// Varies a branch or leaf count by the given fraction, then rounds up or down.
/// The fractional part sets the chance of rounding up: 3.2 has a 20% chance of
/// becoming 4. Non-zero counts stay at least 1, and zero stays zero.
fn jiggle_count(count: u32, amount: f32, rng: &mut Rng) -> u32 {
    let scaled = count as f32 * (1.0 + rng.range(-amount, amount));
    let rounded = (scaled + rng.next_f32()).floor().max(0.0) as u32;
    if count > 0 { rounded.max(1) } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_round_randomly_but_keep_their_floor() {
        let mut rng = Rng::new(3);
        // After rounding, varying 7 by 10% can give 6, 7 or 8.
        for _ in 0..200 {
            let c = jiggle_count(7, 0.1, &mut rng);
            assert!((6..=8).contains(&c), "{c}");
        }
        // Even a 90% variation preserves at least one branch or leaf, and keeps zero at zero.
        for _ in 0..200 {
            assert!(jiggle_count(1, 0.9, &mut rng) >= 1);
            assert_eq!(jiggle_count(0, 0.9, &mut rng), 0);
        }
    }
}
