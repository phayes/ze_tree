//! Generate branch shapes and leaf placements without building a drawable mesh.

use std::collections::VecDeque;
use std::f32::consts::TAU;

use glam::{EulerRot, Mat3, Quat, Vec3};

use crate::{OptionsError, Rng, TreeOptions, TreeType};

/// A tree shape described by branch cross-sections and leaf placements.
///
/// Use [`Skeleton::generate`] to create a repeatable shape from tree settings,
/// including a random seed. This contains no mesh vertices or rendering data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skeleton {
    /// Generated branches, with the trunk first and each parent before its children.
    pub branches: Vec<SkeletonBranch>,
    /// Generated leaves, in the order they were placed.
    pub leaves: Vec<SkeletonLeaf>,
}

/// A branch represented by a sequence of circular cross-sections from base to tip.
#[derive(Debug, Clone, PartialEq)]
pub struct SkeletonBranch {
    /// Number of parent branches back to the trunk: 0 for the trunk, 1 for its children.
    pub level: u32,
    /// Attachment to the parent branch; `None` identifies the trunk.
    pub parent: Option<Attachment>,
    /// Cross-sections ordered from base to tip. Generated branches have at least two.
    pub sections: Vec<Section>,
    /// Intended number of sides around each cross-section when building a mesh.
    pub segment_count: u32,
    /// Branch thickness at its base, measured as a radius in tree units.
    pub base_radius: f32,
}

/// A circular slice through a branch, describing its position, direction, and thickness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    /// Centre of the slice in tree coordinates, with the trunk base at the origin.
    pub origin: Vec3,
    /// Rotation from local to tree coordinates. Local +Y points along branch growth.
    pub orientation: Quat,
    /// Radius in tree units. Generated tips at the deepest level have radius 0.001;
    /// evergreen tips at other levels have radius 0.
    pub radius: f32,
}

/// A leaf placement and size, ready for conversion into a drawable surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonLeaf {
    /// Attachment identifying the supporting branch and position along it.
    pub parent: Attachment,
    /// Point where the leaf meets the branch, in tree coordinates.
    pub origin: Vec3,
    /// Rotation from local to tree coordinates. The leaf extends along local +Y
    /// and faces local +Z.
    pub orientation: Quat,
    /// Leaf width and length in tree units, including the generated size variation.
    pub size: f32,
}

/// A position along a parent branch where a child branch or leaf is attached.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attachment {
    /// Parent branch index in the owning [`Skeleton::branches`] list.
    pub branch: usize,
    /// Index into the parent branch's sections. The final index identifies its tip.
    pub section: usize,
    /// Fraction from `section` to the next section: 0 is the start, 1 is the end.
    /// A tip continuation uses the final section with `t = 0`.
    pub t: f32,
}

impl Skeleton {
    /// Creates branch shapes and leaf placements from `options`, including its seed.
    /// Reusing the same options produces the same skeleton.
    ///
    /// Checks [`TreeOptions::validate`] before growing. That check does not cover
    /// dimensions or floating-point values; invalid values can produce non-finite results.
    ///
    /// # Errors
    ///
    /// Returns [`OptionsError`] for too many branch levels or zero section counts
    /// at any configured level.
    pub fn generate(options: &TreeOptions) -> Result<Self, OptionsError> {
        options.validate()?;

        let branch = &options.branch;
        let mut grower = Grower {
            options,
            rng: Rng::new(options.seed),
            queue: VecDeque::new(),
            skeleton: Skeleton::default(),
        };

        // Process the trunk, then its children, then their children. Preserve this
        // ez-tree order so each branch receives the same random draws.
        grower.queue.push_back(Bud {
            origin: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            length: branch.length[0],
            radius: branch.radius[0],
            level: 0,
            section_count: branch.sections[0],
            segment_count: branch.segments[0],
            parent: None,
        });
        while let Some(bud) = grower.queue.pop_front() {
            grower.grow_branch(bud);
        }

        Ok(grower.skeleton)
    }
}

/// Starting position, dimensions, and settings for a branch that has not grown yet.
struct Bud {
    origin: Vec3,
    orientation: Quat,
    length: f32,
    radius: f32,
    level: usize,
    section_count: u32,
    segment_count: u32,
    parent: Option<Attachment>,
}

/// An attachment position with the local radius and orientation used for new growth.
struct Placement {
    attachment: Attachment,
    origin: Vec3,
    orientation: Quat,
    radius: f32,
}

/// Options, random sequence, pending branches, and results for one generation run.
struct Grower<'a> {
    options: &'a TreeOptions,
    rng: Rng,
    queue: VecDeque<Bud>,
    skeleton: Skeleton,
}

impl Grower<'_> {
    /// Builds and stores one branch, queues its children, and places any leaves.
    fn grow_branch(&mut self, bud: Bud) {
        let options = self.options;
        let levels = options.branch.levels as usize;
        let level = bud.level;
        let section_count = bud.section_count as usize;
        let index = self.skeleton.branches.len();

        // TODO: ez-tree intends to divide deciduous section length by `levels - 1`,
        // but checks 'Deciduous' (capital D), which never matches the tree type.
        // Keep that behaviour for compatibility. Enabling the division requires
        // handling `levels <= 1` (for example with `max(1, levels - 1)`) and
        // retuning the deciduous presets, which would otherwise grow much shorter.
        let section_length = bud.length / bud.section_count as f32;

        // Store each slice before advancing and bending toward the following slice.
        let mut origin = bud.origin;
        let mut orientation = bud.orientation;
        let mut sections = Vec::with_capacity(section_count + 1);
        for i in 0..=section_count {
            let radius = self.section_radius(&bud, i);
            sections.push(Section {
                origin,
                orientation,
                radius,
            });

            // Draw random bends even at the tip to keep later draws aligned with ez-tree.
            // Tip draws are discarded; a zero tip radius can make them non-finite.
            let gnarliness = (1.0 / radius.sqrt()).max(1.0) * options.branch.gnarliness[level];
            let bend_x = self.rng.range(-gnarliness, gnarliness);
            let bend_z = self.rng.range(-gnarliness, gnarliness);
            if i == section_count {
                break;
            }

            origin += orientation * Vec3::new(0.0, section_length, 0.0);
            orientation = self.next_orientation(orientation, bend_x, bend_z, level, origin, radius);
        }

        // A deciduous tip carries a continuation branch at the next level, or one
        // terminal leaf when no deeper level remains.
        let tip = *sections.last().expect("a branch has at least two sections");
        let tip_attachment = Attachment {
            branch: index,
            section: section_count,
            t: 0.0,
        };
        if options.tree_type == TreeType::Deciduous {
            if level < levels {
                self.queue.push_back(Bud {
                    origin: tip.origin,
                    orientation: tip.orientation,
                    length: options.branch.length[level + 1],
                    radius: tip.radius,
                    level: level + 1,
                    // Inherit the parent's division and side counts, matching ez-tree.
                    section_count: bud.section_count,
                    segment_count: bud.segment_count,
                    parent: Some(tip_attachment),
                });
            } else {
                self.record_leaf(tip_attachment, tip.origin, tip.orientation);
            }
        }

        // Place side leaves on the deepest level and side branches on earlier levels.
        if level == levels {
            self.generate_leaves(&sections, index);
        } else {
            let count = options.branch.children[level];
            self.generate_child_branches(count, level + 1, &sections, index);
        }

        // Queuing children has not changed the branch list, so `index` is still valid.
        self.skeleton.branches.push(SkeletonBranch {
            level: level as u32,
            parent: bud.parent,
            sections,
            segment_count: bud.segment_count,
            base_radius: bud.radius,
        });
    }

    /// Computes thickness at section `i`, from 0 through the bud's section count.
    fn section_radius(&self, bud: &Bud, i: usize) -> f32 {
        let options = self.options;
        let section_count = bud.section_count as usize;
        let along = i as f32 / bud.section_count as f32;

        if i == section_count && bud.level == options.branch.levels as usize {
            // ez-tree uses a small nonzero tip radius at the deepest level for both types.
            0.001
        } else {
            match options.tree_type {
                TreeType::Deciduous => bud.radius * (1.0 - options.branch.taper[bud.level] * along),
                // Other evergreen tips taper to zero and have no continuation branch.
                TreeType::Evergreen => bud.radius * (1.0 - along),
            }
        }
    }

    /// Applies random bends, twist, growth force, and trellis pull to a section.
    /// Uses the current `radius` for bending strength and `next_origin` for trellis distance.
    fn next_orientation(
        &self,
        orientation: Quat,
        bend_x: f32,
        bend_z: f32,
        level: usize,
        next_origin: Vec3,
        radius: f32,
    ) -> Quat {
        let branch = &self.options.branch;

        // Match ez-tree by adding random bends to the three.js XYZ Euler angles.
        let euler = euler_xyz(orientation);
        let mut q = Quat::from_euler(EulerRot::XYZ, euler.x + bend_x, euler.y, euler.z + bend_z);

        // Apply twist around local +Y, the branch's growth axis.
        q *= Quat::from_rotation_y(branch.twist[level]);

        // Positive force turns toward the target without overshooting; negative force
        // turns away. Limit either step to the target angle. Thin sections bend more.
        let up = q * Vec3::Y;
        let target = branch.force.direction.normalize_or_zero();
        let axis = up.cross(target);
        let sin_full = axis.length();
        if sin_full > 1e-6 {
            let full_angle = sin_full.atan2(up.dot(target));
            let step = branch.force.strength / radius;
            let angle = step.clamp(-full_angle, full_angle);
            q = Quat::from_axis_angle(axis / sin_full, angle) * q;
        }

        // Apply the trellis influence only when enabled and within reach.
        let trellis = &self.options.trellis;
        if trellis.enabled
            && let Some(pull) = trellis.pull(next_origin, radius)
        {
            q = rotate_towards(q, rotation_from_y(pull.direction), pull.strength);
        }

        q
    }

    /// Queues `count` children at `level` along the supplied parent branch sections.
    /// `parent` is the branch index used in each child's attachment.
    fn generate_child_branches(
        &mut self,
        count: u32,
        level: usize,
        sections: &[Section],
        parent: usize,
    ) {
        let branch = &self.options.branch;

        // Divide the length and circumference into slots for the children. Draw the
        // angular offset even when count is zero to preserve ez-tree's random sequence.
        let radial_offset = self.rng.next_f32();
        let start_min = branch.start[level];
        let height_step = (1.0 - start_min) / count as f32;
        let angle_slots = self.shuffled_indices(count);

        for (i, slot) in angle_slots.into_iter().enumerate() {
            // Randomly position the child within its lengthwise slot.
            let start = start_min + (i as f32 + self.rng.next_f32()) * height_step;
            let placement = place(sections, start, parent);

            // Randomly vary the angle within the child's assigned slot around the branch.
            let radial_jitter = self.rng.range(-0.5, 0.5);
            let radial_angle = TAU * (radial_offset + (slot as f32 + radial_jitter) / count as f32);
            let orientation = placement.orientation
                * (Quat::from_rotation_y(radial_angle)
                    * Quat::from_rotation_x(branch.angle[level].to_radians()));

            // Scale evergreen child length by the distance remaining to the parent tip.
            let length = match self.options.tree_type {
                TreeType::Deciduous => branch.length[level],
                TreeType::Evergreen => branch.length[level] * (1.0 - start),
            };

            self.queue.push_back(Bud {
                origin: placement.origin,
                orientation,
                length,
                radius: branch.radius[level] * placement.radius,
                level,
                section_count: branch.sections[level],
                segment_count: branch.segments[level],
                parent: Some(placement.attachment),
            });
        }
    }

    /// Places side leaves along `sections`, attaching each to branch index `parent`.
    fn generate_leaves(&mut self, sections: &[Section], parent: usize) {
        let leaves = &self.options.leaves;

        // Use the same lengthwise and angular slots as side branches. Keep the offset
        // draw even for zero leaves so subsequent random draws match ez-tree.
        let radial_offset = self.rng.next_f32();
        let count = leaves.count;
        let start_min = leaves.start;
        let height_step = (1.0 - start_min) / count as f32;
        let angle_slots = self.shuffled_indices(count);

        for (i, slot) in angle_slots.into_iter().enumerate() {
            let start = start_min + (i as f32 + self.rng.next_f32()) * height_step;
            let placement = place(sections, start, parent);

            let radial_jitter = self.rng.range(-0.5, 0.5);
            let radial_angle = TAU * (radial_offset + (slot as f32 + radial_jitter) / count as f32);
            let orientation = placement.orientation
                * (Quat::from_rotation_y(radial_angle)
                    * Quat::from_rotation_x(leaves.angle.to_radians()));

            self.record_leaf(placement.attachment, placement.origin, orientation);
        }
    }

    /// Stores a leaf at the supplied attachment, position, and rotation with a random size.
    fn record_leaf(&mut self, parent: Attachment, origin: Vec3, orientation: Quat) {
        let leaves = &self.options.leaves;
        let variance = leaves.size_variance;
        let size = leaves.size * (1.0 + self.rng.range(-variance, variance));
        self.skeleton.leaves.push(SkeletonLeaf {
            parent,
            origin,
            orientation,
            size,
        });
    }

    /// Shuffles the indices from 0 up to, but excluding, `count` using Fisher–Yates.
    fn shuffled_indices(&mut self, count: u32) -> Vec<u32> {
        let mut indices: Vec<u32> = (0..count).collect();
        for k in (1..count as usize).rev() {
            // Keep the draw in f64: rounding to f32 could select the out-of-range index `k + 1`.
            let r = (self.rng.next_f64() * (k + 1) as f64) as usize;
            indices.swap(k, r);
        }
        indices
    }
}

/// Finds a placement along a branch with at least two sections.
/// `t` runs from 0 at the base to 1 at the tip; `branch` identifies the parent.
/// Negative `t` extrapolates from the first pair; values above 1 use the final section.
fn place(sections: &[Section], t: f32, branch: usize) -> Placement {
    // Map the length fraction to a section index and a fraction within that interval.
    let last = sections.len() - 1;
    let scaled = t * last as f32;
    let index = (scaled.floor().max(0.0) as usize).min(last);
    let a = &sections[index];
    let b = sections.get(index + 1).unwrap_or(a);
    let alpha = scaled - index as f32;

    // Position and radius interpolate from `a` to `b`. Preserve ez-tree's reversed
    // orientation interpolation: slerp from `b` toward `a` at the same fraction.
    // At alpha = 0 the position is at `a`, but the orientation is that of `b`.
    Placement {
        attachment: Attachment {
            branch,
            section: index,
            t: alpha,
        },
        origin: a.origin.lerp(b.origin, alpha),
        orientation: b.orientation.slerp(a.orientation, alpha),
        radius: (1.0 - alpha) * a.radius + alpha * b.radius,
    }
}

/// Converts a unit quaternion to XYZ Euler angles in radians using three.js conventions.
fn euler_xyz(q: Quat) -> Vec3 {
    // three.js uses q = Rx * Ry * Rz. Here `mRC` names matrix row R, column C,
    // counting from 1; keep its angle selection and gimbal-lock threshold.
    let m = Mat3::from_quat(q);
    let (m11, m12, m13) = (m.x_axis.x, m.y_axis.x, m.z_axis.x);
    let (m22, m23) = (m.y_axis.y, m.z_axis.y);
    let (m32, m33) = (m.y_axis.z, m.z_axis.z);

    let y = m13.clamp(-1.0, 1.0).asin();
    if m13.abs() < 0.999_999_9 {
        Vec3::new((-m23).atan2(m33), y, (-m12).atan2(m11))
    } else {
        // Near gimbal lock, X and Z cannot be distinguished. Match three.js by
        // setting Z to zero and assigning the remaining rotation to X.
        Vec3::new(m32.atan2(m22), y, 0.0)
    }
}

/// Returns a rotation aligning +Y with `to`, which must be a unit vector.
fn rotation_from_y(to: Vec3) -> Quat {
    let w = to.y + 1.0;
    // For the opposite direction, choose a half turn around +Z, as three.js does.
    if w < f32::EPSILON {
        Quat::from_xyzw(0.0, 0.0, 1.0, 0.0)
    } else {
        // Build the shortest rotation from the cross product and dot product with +Y.
        Quat::from_xyzw(to.z, 0.0, -to.x, w).normalize()
    }
}

/// Moves between unit rotations by `step` radians, stopping at `to` for positive steps.
/// Negative steps rotate away; identical rotations remain unchanged.
fn rotate_towards(from: Quat, to: Quat, step: f32) -> Quat {
    let angle = 2.0 * from.dot(to).clamp(-1.0, 1.0).abs().acos();
    if angle == 0.0 {
        return from;
    }
    from.slerp(to, (step / angle).min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn angles() -> impl Iterator<Item = Vec3> {
        let values = [-3.0_f32, -1.5, -0.7, 0.0, 0.3, 1.2, 2.9];
        values.into_iter().flat_map(move |x| {
            values
                .into_iter()
                .flat_map(move |y| values.into_iter().map(move |z| Vec3::new(x, y * 0.5, z)))
        })
    }

    /// Measures the smallest angle between unit rotations, in radians.
    fn rotation_angle(a: Quat, b: Quat) -> f32 {
        // Use the relative rotation's vector part to retain precision for small angles.
        2.0 * (a.conjugate() * b).xyz().length().min(1.0).asin()
    }

    #[test]
    fn glam_euler_xyz_matches_three_convention() {
        // Verify the multiplication order used by three.js for XYZ Euler rotations.
        for e in angles() {
            let glam = Quat::from_euler(EulerRot::XYZ, e.x, e.y, e.z);
            let three = Quat::from_rotation_x(e.x)
                * Quat::from_rotation_y(e.y)
                * Quat::from_rotation_z(e.z);
            assert!(rotation_angle(glam, three) < 1e-5, "{e}");
        }
    }

    #[test]
    fn euler_xyz_round_trips() {
        for e in angles() {
            let q = Quat::from_euler(EulerRot::XYZ, e.x, e.y, e.z);
            let back = euler_xyz(q);
            let q2 = Quat::from_euler(EulerRot::XYZ, back.x, back.y, back.z);
            assert!(rotation_angle(q, q2) < 1e-5, "{e} -> {back}");
            // These inputs avoid gimbal lock, so the original angles should also be recovered.
            assert!((back - e).abs().max_element() < 1e-4, "{e} -> {back}");
        }
    }

    #[test]
    fn euler_xyz_at_gimbal_lock_puts_rotation_in_x() {
        let q = Quat::from_rotation_x(0.4) * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let e = euler_xyz(q);
        assert_eq!(e.z, 0.0);
        assert!((e.x - 0.4).abs() < 1e-3, "{e}");
    }

    #[test]
    fn rotation_from_y_maps_y_onto_target() {
        for to in [
            Vec3::X,
            Vec3::Z,
            Vec3::new(1.0, 2.0, -3.0).normalize(),
            Vec3::NEG_Y,
        ] {
            let q = rotation_from_y(to);
            assert!((q * Vec3::Y).abs_diff_eq(to, 1e-6), "{to}");
        }
        assert_eq!(rotation_from_y(Vec3::Y), Quat::IDENTITY);
    }

    #[test]
    fn rotate_towards_steps_without_overshoot() {
        let to = Quat::from_rotation_z(1.0);
        let half = rotate_towards(Quat::IDENTITY, to, 0.25);
        assert!(rotation_angle(half, Quat::from_rotation_z(0.25)) < 1e-5);
        let full = rotate_towards(Quat::IDENTITY, to, 5.0);
        assert!(rotation_angle(full, to) < 1e-5);
        let away = rotate_towards(Quat::IDENTITY, to, -0.25);
        assert!(rotation_angle(away, Quat::from_rotation_z(-0.25)) < 1e-5);
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let options = TreeOptions::default();
        let mut grower = Grower {
            options: &options,
            rng: Rng::new(5),
            queue: VecDeque::new(),
            skeleton: Skeleton::default(),
        };
        for count in [0, 1, 2, 7, 30] {
            let mut v = grower.shuffled_indices(count);
            v.sort_unstable();
            assert_eq!(v, (0..count).collect::<Vec<_>>());
        }
    }
}
