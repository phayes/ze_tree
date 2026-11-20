//! Settings and shader source for animating trees in the wind.
//! [`Skeleton::mesh`](crate::Skeleton::mesh) supplies the per-vertex animation
//! data in [`MeshBuffers::wind`](crate::MeshBuffers::wind).

use glam::Vec3;

use crate::{Attachment, Skeleton};

/// Shader source for adding wind motion to bark and leaves.
///
/// Register this module with your WESL compiler or shader loader, for example as
/// `ze_tree::wind`. In your vertex shader, call `wind_offset` and add its result
/// to the vertex position before transforming it for display. Your shader must
/// provide the time and a uniform binding for the module's `WindParams` settings.
pub const WIND_WESL: &str = include_str!("../shaders/wind.wesl");

/// Controls how the trunk, branches, and leaves move in the wind.
///
/// Trunk bend makes the tree lean, with more movement near the top. Branch sway
/// moves longer branches more, with movement increasing toward their tips. Leaf
/// flutter moves leaf tips while keeping their bases attached.
///
/// The defaults match ez-tree: only leaves move. Increase `trunk_bend` and
/// `branch_sway` to enable the other motions. Use [`Self::to_uniform`] to prepare
/// these settings for the shader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindParams {
    /// Direction and size of the wind motion, in tree coordinates. Leaf-tip
    /// displacement is at most this vector's length; trunk and branch motion
    /// have their own multipliers. Defaults to `(0.5, 0, 0.5)`.
    pub strength: Vec3,
    /// Animation speed. Larger values make all wind motion faster. Measured in
    /// radians per second for the slowest leaf oscillation; defaults to 0.5.
    pub frequency: f32,
    /// Distance over which the wind pattern varies, in tree units. Larger values
    /// make motion vary more gradually across the tree. Must be nonzero; defaults to 70.
    pub scale: f32,
    /// Trunk movement multiplier. At 1, the highest section moves by up to
    /// [`strength`](Self::strength). Defaults to 0, which disables trunk bend.
    pub trunk_bend: f32,
    /// Branch movement multiplier, applied to [`strength`](Self::strength) and
    /// each vertex's branch weight. Longer branches and points nearer branch tips
    /// have greater weights. Defaults to 0, which disables branch sway.
    pub branch_sway: f32,
}

impl Default for WindParams {
    fn default() -> Self {
        Self {
            strength: Vec3::new(0.5, 0.0, 0.5),
            frequency: 0.5,
            scale: 70.0,
            trunk_bend: 0.0,
            branch_sway: 0.0,
        }
    }
}

impl WindParams {
    /// Packs the settings into eight floats (32 bytes) for the shader's uniform buffer.
    /// The order is `strength.xyz`, `frequency`, `scale`, `trunk_bend`,
    /// `branch_sway`, and one padding float.
    pub fn to_uniform(&self) -> [f32; 8] {
        let s = self.strength;
        [
            s.x,
            s.y,
            s.z,
            self.frequency,
            self.scale,
            self.trunk_bend,
            self.branch_sway,
            0.0,
        ]
    }
}

/// Animation weights and timing for the sections of one skeleton branch.
pub(crate) struct BranchWind {
    /// Trunk movement at each section: squared height relative to the tree's
    /// highest section, clamped to 0–1.
    pub bend: Vec<f32>,
    /// Branch movement at each section, measured in tree units before applying
    /// the wind settings.
    pub weight: Vec<f32>,
    /// Timing offset in `[0, 1)`. A main limb and its descendants share this value
    /// so they sway together.
    pub phase: f32,
}

impl BranchWind {
    /// Interpolates `[bend, weight, phase]` from `section` toward the next section
    /// by fraction `t`. At the last section, returns that section's values.
    fn at(&self, section: usize, t: f32) -> [f32; 3] {
        let lerp = |values: &[f32]| match values.get(section + 1) {
            Some(next) => values[section] + (next - values[section]) * t,
            None => values[section],
        };
        [lerp(&self.bend), lerp(&self.weight), self.phase]
    }
}

/// Builds animation data in the same order as the skeleton's branches.
///
/// Trunk movement increases with squared height. Branch movement starts with
/// the parent's weight at the attachment and increases toward the tip, keeping
/// connected branches moving together. Each main limb gets its own timing offset.
pub(crate) fn branch_winds(skeleton: &Skeleton) -> Vec<BranchWind> {
    let height = skeleton
        .branches
        .iter()
        .flat_map(|b| &b.sections)
        .map(|s| s.origin.y)
        .fold(0.0_f32, f32::max);

    let mut winds: Vec<BranchWind> = Vec::with_capacity(skeleton.branches.len());
    for (index, branch) in skeleton.branches.iter().enumerate() {
        // Use the section's height so all vertices around it bend together.
        let bend = branch
            .sections
            .iter()
            .map(|s| {
                if height > 0.0 {
                    (s.origin.y.max(0.0) / height).powi(2).min(1.0)
                } else {
                    0.0
                }
            })
            .collect();

        // Inherit movement at the attachment; give each child of the trunk
        // its own timing offset, shared by its descendants.
        let (base, phase) = match branch.parent {
            None => (None, 0.0),
            Some(Attachment {
                branch: p,
                section,
                t,
            }) => {
                let [_, weight, parent_phase] = winds[p].at(section, t);
                let phase = if skeleton.branches[p].parent.is_none() {
                    hash_phase(index)
                } else {
                    parent_phase
                };
                (Some(weight), phase)
            }
        };

        // For non-trunk branches, add squared distance divided by branch length.
        // This adds one branch length to the inherited weight at the tip.
        let weight = match base {
            None => vec![0.0; branch.sections.len()],
            Some(base) => {
                let mut along = Vec::with_capacity(branch.sections.len());
                let mut distance = 0.0;
                for (i, s) in branch.sections.iter().enumerate() {
                    if i > 0 {
                        distance += s.origin.distance(branch.sections[i - 1].origin);
                    }
                    along.push(distance);
                }
                let length = distance;
                along
                    .into_iter()
                    .map(|s| {
                        if length > 0.0 {
                            base + s * s / length
                        } else {
                            base
                        }
                    })
                    .collect()
            }
        };

        winds.push(BranchWind {
            bend,
            weight,
            phase,
        });
    }
    winds
}

/// Samples `[bend, weight, phase]` where a leaf or child branch attaches.
pub(crate) fn wind_at(winds: &[BranchWind], attachment: &Attachment) -> [f32; 3] {
    winds[attachment.branch].at(attachment.section, attachment.t)
}

/// Gives each branch index a repeatable timing offset in `[0, 1)` without
/// advancing the random number generator used to build the tree.
fn hash_phase(index: usize) -> f32 {
    // The lowbias32 hash makes adjacent indices produce different timings.
    let mut x = index as u32;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use glam::Quat;

    use super::*;
    use crate::{Section, SkeletonBranch};

    fn branch(parent: Option<Attachment>, level: u32, points: &[Vec3]) -> SkeletonBranch {
        SkeletonBranch {
            level,
            parent,
            sections: points
                .iter()
                .map(|&origin| Section {
                    origin,
                    orientation: Quat::IDENTITY,
                    radius: 0.1,
                })
                .collect(),
            segment_count: 3,
            base_radius: 0.1,
        }
    }

    /// Test tree: a trunk 4 units tall, a horizontal limb 2 units long at height 2,
    /// and a twig rising 1 unit from the limb's tip.
    fn skeleton() -> Skeleton {
        let at = |branch, section, t| Some(Attachment { branch, section, t });
        Skeleton {
            branches: vec![
                branch(
                    None,
                    0,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, 2.0, 0.0),
                        Vec3::new(0.0, 4.0, 0.0),
                    ],
                ),
                branch(
                    at(0, 0, 1.0),
                    1,
                    &[
                        Vec3::new(0.0, 2.0, 0.0),
                        Vec3::new(1.0, 2.0, 0.0),
                        Vec3::new(2.0, 2.0, 0.0),
                    ],
                ),
                branch(
                    at(1, 2, 0.0),
                    2,
                    &[Vec3::new(2.0, 2.0, 0.0), Vec3::new(2.0, 3.0, 0.0)],
                ),
            ],
            leaves: vec![],
        }
    }

    #[test]
    fn trunk_bends_with_height_squared() {
        let winds = branch_winds(&skeleton());
        assert_eq!(winds[0].bend, [0.0, 0.25, 1.0]);
        assert_eq!(winds[0].weight, [0.0; 3]);
        assert_eq!(winds[0].phase, 0.0);
        // The twig tip is three quarters of the tree's height: (3 / 4)².
        assert_eq!(winds[2].bend[1], 0.5625);
    }

    #[test]
    fn branch_weight_accumulates_from_the_attachment() {
        let winds = branch_winds(&skeleton());
        // On the 2-unit limb, squared distance divided by length gives these weights.
        assert_eq!(winds[1].weight, [0.0, 0.5, 2.0]);
        // The twig inherits weight 2 at its base and adds 1 at its tip.
        assert_eq!(winds[2].weight, [2.0, 3.0]);
        // Sharing a timing offset keeps the twig moving with its parent limb.
        assert_eq!(winds[2].phase, winds[1].phase);
        assert!((0.0..1.0).contains(&winds[1].phase));
    }

    #[test]
    fn attachments_interpolate_between_sections() {
        let winds = branch_winds(&skeleton());
        let a = Attachment {
            branch: 1,
            section: 1,
            t: 0.5,
        };
        assert_eq!(wind_at(&winds, &a), [0.25, 1.25, winds[1].phase]);
    }

    #[test]
    fn phases_differ_between_limbs() {
        assert_ne!(hash_phase(1), hash_phase(2));
        assert!((0.0..1.0).contains(&hash_phase(usize::MAX)));
    }
}
