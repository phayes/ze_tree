//! Prepare a tree for animation with joints: movable points along its branches.
//!
//! A [`Rig`] describes these points and their parent relationships. [`Rig::skin`]
//! assigns weights that tell a renderer how strongly each joint moves each mesh vertex.

use std::fmt;

use glam::{Mat4, Quat, Vec3};

use crate::{MAX_LEVELS, Skeleton, TreeMesh};

/// Maximum number of joints accepted by [`Rig::new`].
///
/// Check the target renderer's limit when choosing [`RigOptions::max_joints`].
pub const MAX_RIG_JOINTS: u32 = 256;

/// Controls how many movable joints a tree gets and how smoothly it bends.
///
/// By default, [`Rig::new`] fits the joints within the requested limit, favouring
/// shallower branch levels and adding bending detail from the trunk outward.
/// Some deeper branches may have no joints of their own; they still move with
/// the branches they grow from. Spare joints go to the largest branches at the
/// next depth or, once every depth is covered, give the largest branches closer
/// joint spacing. The limit is a maximum, not a guaranteed joint count.
///
/// Use [`Rig::level_strides`], [`Rig::branch_stride`], and [`Rig::is_rigged`] to
/// inspect the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RigOptions {
    /// Maximum joint count. Must be from 1 to [`MAX_RIG_JOINTS`]; defaults to 64.
    pub max_joints: u32,
    /// Deepest branch level allowed to have its own joints. Use 0 for just the
    /// trunk, 1 to include branches growing directly from it, and so on.
    /// Defaults to [`MAX_LEVELS`], allowing all generated branch levels.
    pub max_level: u32,
    /// Fixed spacing between joints, measured in branch sections (cross-sections
    /// along the branch). `Some(1)` places a joint at each section except the tip;
    /// `Some(0)` behaves the same. A branch with one section gets one joint.
    ///
    /// With `Some`, every branch through `max_level` receives joints, or
    /// [`Rig::new`] returns an error if they exceed `max_joints`.
    /// The default, `None`, chooses spacing and branch coverage automatically.
    pub section_stride: Option<u32>,
    /// Blends the movement of neighbouring joints for points between them.
    /// Defaults to `true` for smoother bends. If `false`, a point follows only
    /// the closest joint toward the branch base. Applies to bark and leaf attachments.
    pub blend: bool,
}

impl Default for RigOptions {
    fn default() -> Self {
        Self {
            max_joints: 64,
            max_level: MAX_LEVELS,
            section_stride: None,
            blend: true,
        }
    }
}

/// A movable point on a branch, with its position and orientation before animation.
///
/// Changing its transform in a renderer also moves the joints that descend from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Joint {
    /// Parent's index in [`Rig::joints`]. `None` marks the root at the trunk base.
    /// A parent always appears before its children in that list.
    pub parent: Option<usize>,
    /// Index in [`Skeleton::branches`] of the branch containing this joint.
    pub branch: usize,
    /// Index in that branch's sections of the cross-section containing this joint.
    pub section: usize,
    /// Position before animation, measured from the parent in the parent's axes.
    /// For the root, this is a position in the tree's coordinate system.
    pub translation: Vec3,
    /// Orientation before animation, relative to the parent or, for the root,
    /// to the tree. The joint's positive Y axis follows the branch direction.
    pub rotation: Quat,
    /// Converts a mesh point into this joint's coordinates before animation
    /// (the rest pose). A renderer combines this matrix with the animated joint
    /// transform to move the point without applying its initial placement twice.
    pub inverse_bind: Mat4,
}

/// Per-vertex joint assignments and movement weights, used to animate a mesh.
///
/// Each vertex has four slots. [`Rig::skin`] uses at most two, with weights
/// summing to 1; unused slots have joint index 0 and weight 0.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkinWeights {
    /// Four indices into [`Rig::joints`] for each vertex, in mesh vertex order.
    pub joints: Vec<[u16; 4]>,
    /// Four movement weights per vertex, matching [`Self::joints`]. A weight of
    /// 1 follows that joint fully; 0 means the joint has no effect.
    pub weights: Vec<[f32; 4]>,
}

/// Joint assignments for the bark and leaf vertices of a [`TreeMesh`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshSkin {
    /// Assignments for [`TreeMesh::branches`], in the same vertex order.
    pub branches: SkinWeights,
    /// Assignments for [`TreeMesh::leaves`], in the same vertex order.
    pub leaves: SkinWeights,
}

/// A hierarchy of movable joints for bending a tree in a renderer or 3D tool.
///
/// Create it from a [`Skeleton`], then call [`Rig::skin`] to connect mesh vertices
/// to its joints. The same rig can serve full-detail and simpler meshes made from
/// that skeleton. Before animation, applying the joints leaves the mesh unchanged.
///
/// This type supplies the rig data, not animation clips or a playback system.
#[derive(Debug, Clone, PartialEq)]
pub struct Rig {
    /// Joint transforms in parent-before-child order.
    joints: Vec<Joint>,
    /// Section spacing for each completely covered branch level.
    level_strides: Vec<u32>,
    /// Whether points between joints blend their movement.
    blend: bool,
    /// Joint assignments indexed by skeleton branch.
    bindings: Vec<Binding>,
    /// Attachment weights indexed by skeleton leaf.
    leaf_influences: Vec<Influence>,
}

/// A joint limit or spacing setting that prevents a rig from being created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RigError {
    /// The requested joint limit is outside 1 through [`MAX_RIG_JOINTS`].
    InvalidJointLimit {
        /// Value supplied as [`RigOptions::max_joints`].
        max_joints: u32,
    },
    /// The fixed section spacing requires more joints than the requested limit.
    TooManyJoints {
        /// Required joint count, capped at `u32::MAX` if larger.
        needed: u32,
        /// Joint limit supplied by the caller.
        max_joints: u32,
    },
}

impl fmt::Display for RigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJointLimit { max_joints } => {
                write!(
                    f,
                    "max_joints is {max_joints}; it must be from 1 to {MAX_RIG_JOINTS}"
                )
            }
            Self::TooManyJoints { needed, max_joints } => write!(
                f,
                "the section stride needs {needed} joints, more than max_joints ({max_joints})"
            ),
        }
    }
}

impl std::error::Error for RigError {}

/// The one or two joints controlling a point, with zero weights in unused slots.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Influence {
    joints: [u16; 2],
    weights: [f32; 2],
}

impl Influence {
    fn rigid(joint: usize) -> Self {
        Self {
            joints: [joint as u16, 0],
            weights: [1.0, 0.0],
        }
    }

    fn joints4(self) -> [u16; 4] {
        [self.joints[0], self.joints[1], 0, 0]
    }

    fn weights4(self) -> [f32; 4] {
        [self.weights[0], self.weights[1], 0.0, 0.0]
    }
}

/// Describes which joints control one branch's vertices.
#[derive(Debug, Clone, PartialEq)]
enum Binding {
    /// The branch has consecutive joints starting at index `first`, located at
    /// `sections` in increasing order from its base, every `stride` sections.
    Joints {
        first: usize,
        sections: Vec<usize>,
        stride: u32,
    },
    /// The branch inherits the joint weights at its attachment to its parent.
    Fixed(Influence),
}

impl Rig {
    /// Creates movable joints for `skeleton` within the limits in `options`.
    ///
    /// Pass a generated skeleton, or one with nonempty branches in parent-before-child
    /// order and valid attachment indices. An empty skeleton yields no joints.
    /// Use [`Self::skin`] to get vertex assignments for meshes from this skeleton.
    ///
    /// # Errors
    ///
    /// Returns [`RigError::InvalidJointLimit`] for a limit outside 1 through
    /// [`MAX_RIG_JOINTS`]. Returns [`RigError::TooManyJoints`] if the fixed
    /// [`RigOptions::section_stride`] would exceed the limit.
    ///
    /// # Panics
    ///
    /// May panic if a manually built skeleton has empty branches, invalid parent
    /// attachments, or branch levels inconsistent with its parent relationships.
    pub fn new(skeleton: &Skeleton, options: &RigOptions) -> Result<Self, RigError> {
        if options.max_joints == 0 || options.max_joints > MAX_RIG_JOINTS {
            return Err(RigError::InvalidJointLimit {
                max_joints: options.max_joints,
            });
        }

        // Collect eligible branches by depth and measure their size for joint allocation.
        let deepest = skeleton.branches.iter().map(|b| b.level).max();
        let deepest = deepest.map_or(0, |d| d.min(options.max_level) + 1) as usize;
        let mut levels: Vec<Vec<BranchSize>> = vec![Vec::new(); deepest];
        for (index, branch) in skeleton.branches.iter().enumerate() {
            if let Some(level) = levels.get_mut(branch.level as usize) {
                let length: f32 = branch
                    .sections
                    .windows(2)
                    .map(|w| w[0].origin.distance(w[1].origin))
                    .sum();
                level.push(BranchSize {
                    index,
                    segments: segment_count(branch.sections.len()),
                    size: length * branch.base_radius,
                });
            }
        }
        // Generated levels have no gaps; ignore levels beyond a gap in custom input.
        if let Some(gap) = levels.iter().position(Vec::is_empty) {
            levels.truncate(gap);
        }

        let (level_strides, overrides) = match options.section_stride {
            Some(stride) => (
                explicit_strides(&levels, stride.max(1), options.max_joints)?,
                Vec::new(),
            ),
            None => fitted_strides(&levels, options.max_joints),
        };

        let mut rig = Self {
            joints: Vec::new(),
            level_strides,
            blend: options.blend,
            bindings: Vec::with_capacity(skeleton.branches.len()),
            leaf_influences: Vec::with_capacity(skeleton.leaves.len()),
        };
        rig.place_joints(skeleton, &overrides);
        for leaf in &skeleton.leaves {
            let influence =
                rig.influence_at(leaf.parent.branch, leaf.parent.section, leaf.parent.t);
            rig.leaf_influences.push(influence);
        }
        Ok(rig)
    }

    /// Returns the joints in parent-before-child order, starting at the trunk base.
    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }

    /// Returns how many joints this rig contains; an empty rig has zero.
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    /// Returns the usual joint spacing for levels where every branch has its own joints.
    ///
    /// Entry 0 is the trunk level. A value of N places a joint every N sections,
    /// starting at each branch base and excluding its tip. With automatic spacing,
    /// some of the largest branches may use finer spacing to fill the joint limit,
    /// and the next level may have joints on only some branches. Use
    /// [`Self::branch_stride`] for a specific branch.
    pub fn level_strides(&self) -> &[u32] {
        &self.level_strides
    }

    /// Returns a branch's joint spacing in sections, using its index in the source
    /// skeleton. A branch with a single joint reports its section interval count.
    /// Returns `None` for an unknown index or a branch that only follows its
    /// parent's movement.
    pub fn branch_stride(&self, branch: usize) -> Option<u32> {
        match self.bindings.get(branch) {
            Some(Binding::Joints { stride, .. }) => Some(*stride),
            _ => None,
        }
    }

    /// Reports whether a branch has its own joints, using its index in the source
    /// skeleton. Returns `false` for an unknown index or a branch that only follows
    /// its parent's movement.
    pub fn is_rigged(&self, branch: usize) -> bool {
        self.branch_stride(branch).is_some()
    }

    /// Assigns joints and movement weights to every vertex in `mesh`.
    ///
    /// The mesh must come from the same skeleton as this rig, but may use any detail
    /// level. Bark follows nearby joints, and branches with no joints inherit the
    /// weights at their parent attachment. All corners of a leaf share its attachment
    /// weights. Blending two joint transforms can still change a leaf's shape.
    ///
    /// # Panics
    ///
    /// Panics if a mesh span refers to a branch or leaf missing from the rig, or if
    /// an assigned vertex range is out of bounds. Other mismatched meshes may produce
    /// incorrect assignments without a panic.
    pub fn skin(&self, mesh: &TreeMesh) -> MeshSkin {
        let mut branches = SkinWeights::rigid_to_root(mesh.branches.positions.len());
        for span in &mesh.branch_spans {
            let ring = span.ring_vertices() as usize;
            let first = span.vertices.start as usize;
            for (k, &section) in span.sections.iter().enumerate() {
                let influence = self.influence_at(span.branch, section, 0.0);
                branches.fill(first + k * ring..first + (k + 1) * ring, influence);
            }
        }

        let mut leaves = SkinWeights::rigid_to_root(mesh.leaves.positions.len());
        for span in &mesh.leaf_spans {
            let range = span.vertices.start as usize..span.vertices.end as usize;
            leaves.fill(range, self.leaf_influences[span.leaf]);
        }

        MeshSkin { branches, leaves }
    }

    /// Adds joints and bindings in parent-before-child branch order.
    /// `overrides` must be sorted by branch index and gives `(branch, stride)` pairs
    /// that replace the level stride, including branches beyond the fully covered levels.
    /// A stride larger than the branch leaves only its base joint.
    fn place_joints(&mut self, skeleton: &Skeleton, overrides: &[(usize, usize)]) {
        for (index, branch) in skeleton.branches.iter().enumerate() {
            let stride = match overrides.binary_search_by_key(&index, |&(b, _)| b) {
                Ok(found) => Some(overrides[found].1),
                Err(_) => self
                    .level_strides
                    .get(branch.level as usize)
                    .map(|&s| s as usize),
            };
            let Some(stride) = stride else {
                // Reuse the parent's attachment weights for this entire branch.
                let parent = branch.parent.expect("the trunk is always rigged");
                let influence = self.influence_at(parent.branch, parent.section, parent.t);
                self.bindings.push(Binding::Fixed(influence));
                continue;
            };

            // Attach the base joint to the parent's joint below the branch attachment.
            // Chain the remaining joints along this branch.
            let mut parent = branch.parent.map(|a| self.joint_below(a.branch, a.section));
            let first = self.joints.len();
            let segments = segment_count(branch.sections.len());
            let stride = stride.min(segments);
            let sections: Vec<usize> = (0..segments).step_by(stride).collect();
            for &s in &sections {
                let section = &branch.sections[s];
                let rotation = section.orientation.normalize();
                let (translation, local_rotation) = match parent {
                    None => (section.origin, rotation),
                    Some(p) => {
                        let (p_origin, p_rotation) = self.world(p, skeleton);
                        let inverse = p_rotation.inverse();
                        (
                            inverse * (section.origin - p_origin),
                            (inverse * rotation).normalize(),
                        )
                    }
                };
                let inverse_rotation = rotation.inverse();
                self.joints.push(Joint {
                    parent,
                    branch: index,
                    section: s,
                    translation,
                    rotation: local_rotation,
                    inverse_bind: Mat4::from_rotation_translation(
                        inverse_rotation,
                        -(inverse_rotation * section.origin),
                    ),
                });
                parent = Some(self.joints.len() - 1);
            }
            self.bindings.push(Binding::Joints {
                first,
                sections,
                stride: stride as u32,
            });
        }
    }

    /// Looks up a joint's original position and orientation in tree coordinates.
    fn world(&self, joint: usize, skeleton: &Skeleton) -> (Vec3, Quat) {
        let joint = &self.joints[joint];
        let section = &skeleton.branches[joint.branch].sections[joint.section];
        (section.origin, section.orientation.normalize())
    }

    /// Finds the joint at or below `section` toward the base of a rigged branch.
    /// The branch must already have a `Joints` binding.
    fn joint_below(&self, branch: usize, section: usize) -> usize {
        match &self.bindings[branch] {
            Binding::Joints {
                first, sections, ..
            } => {
                first
                    + sections
                        .partition_point(|&s| s <= section)
                        .saturating_sub(1)
            }
            Binding::Fixed(_) => unreachable!("children of unrigged branches are unrigged"),
        }
    }

    /// Returns movement weights at `section + t` on an already bound branch.
    /// `t` is the fraction from that section to the next, normally between 0 and 1.
    fn influence_at(&self, branch: usize, section: usize, t: f32) -> Influence {
        let (first, sections) = match &self.bindings[branch] {
            Binding::Joints {
                first, sections, ..
            } => (*first, sections),
            Binding::Fixed(influence) => return *influence,
        };

        // Locate the joint at or below the requested section position.
        let position = section as f32 + t;
        let k = sections
            .partition_point(|&s| s as f32 <= position)
            .saturating_sub(1);
        let Some(&next) = sections.get(k + 1).filter(|_| self.blend) else {
            return Influence::rigid(first + k);
        };

        // Interpolate by section position, not by physical distance between joints.
        let f = (position - sections[k] as f32) / (next - sections[k]) as f32;
        if f <= 0.0 {
            return Influence::rigid(first + k);
        }
        Influence {
            joints: [(first + k) as u16, (first + k + 1) as u16],
            weights: [1.0 - f, f],
        }
    }
}

impl SkinWeights {
    /// Assigns all `count` vertices entirely to joint 0.
    fn rigid_to_root(count: usize) -> Self {
        let root = Influence::rigid(0);
        Self {
            joints: vec![root.joints4(); count],
            weights: vec![root.weights4(); count],
        }
    }

    /// Assigns the same joints and weights to every vertex in an in-bounds range.
    fn fill(&mut self, range: std::ops::Range<usize>, influence: Influence) {
        self.joints[range.clone()].fill(influence.joints4());
        self.weights[range].fill(influence.weights4());
    }
}

/// Counts joints for a branch with the given segment count and nonzero stride.
fn joints_for(segments: usize, stride: usize) -> usize {
    segments.div_ceil(stride)
}

/// Counts intervals between sections, with a minimum of one to allow a base joint.
/// The caller must still ensure that the branch has at least one section.
fn segment_count(sections: usize) -> usize {
    sections.saturating_sub(1).max(1)
}

/// Branch measurements used to allocate joints: segment count and length times base radius.
#[derive(Debug, Clone, Copy)]
struct BranchSize {
    index: usize,
    segments: usize,
    size: f32,
}

/// Applies the same nonzero stride to all levels, returning an error if it exceeds the limit.
fn explicit_strides(
    levels: &[Vec<BranchSize>],
    stride: u32,
    max_joints: u32,
) -> Result<Vec<u32>, RigError> {
    let needed: usize = levels
        .iter()
        .flatten()
        .map(|b| joints_for(b.segments, stride as usize))
        .sum();
    if needed > max_joints as usize {
        return Err(RigError::TooManyJoints {
            needed: needed.try_into().unwrap_or(u32::MAX),
            max_joints,
        });
    }
    Ok(vec![stride; levels.len()])
}

/// Fits joint spacing and branch coverage within `max_joints`.
///
/// First reserve one joint per branch for as many complete levels as fit. Then
/// add joints from the trunk outward, and give any remainder to the largest
/// branches in the next level. If every level is covered, spend what is left on
/// finer spacing for individual branches, trunk outward and largest first.
///
/// Returns strides for complete levels and `(branch index, stride)` overrides
/// sorted by branch index. A partially covered level's branches get `usize::MAX`,
/// which leaves only a base joint.
fn fitted_strides(levels: &[Vec<BranchSize>], max_joints: u32) -> (Vec<u32>, Vec<(usize, usize)>) {
    // Reserve a base joint for every branch in each level that fits.
    let mut remaining = max_joints as usize;
    let mut rigged = 0;
    for level in levels {
        if level.len() > remaining {
            break;
        }
        remaining -= level.len();
        rigged += 1;
    }

    // Use the remaining capacity to reduce spacing, starting with the trunk.
    let mut strides = Vec::with_capacity(rigged);
    for level in &levels[..rigged] {
        let longest = level.iter().map(|b| b.segments).max().unwrap_or(1);
        let count = |stride: usize| {
            level
                .iter()
                .map(|b| joints_for(b.segments, stride))
                .sum::<usize>()
        };
        let coarsest = level.len();
        let stride = (1..=longest)
            .find(|&s| count(s) - coarsest <= remaining)
            .unwrap_or(longest);
        remaining -= count(stride) - coarsest;
        strides.push(stride as u32);
    }

    // Give base joints to the largest branches of the next level.
    let mut overrides = Vec::new();
    if let Some(next) = levels.get(rigged) {
        let next = by_size(next);
        let count = next.len().min(remaining);
        overrides.extend(next[..count].iter().map(|b| (b.index, usize::MAX)));
        remaining -= count;
    }

    // Refine individual branches with what is left, trunk outward and largest first.
    // Each branch takes the finest spacing that still fits.
    for (level, &stride) in levels.iter().zip(&strides) {
        let stride = stride as usize;
        for branch in by_size(level) {
            let current = joints_for(branch.segments, stride);
            if let Some(finer) =
                (1..stride).find(|&s| joints_for(branch.segments, s) - current <= remaining)
            {
                remaining -= joints_for(branch.segments, finer) - current;
                if joints_for(branch.segments, finer) > current {
                    overrides.push((branch.index, finer));
                }
            }
        }
    }

    overrides.sort_unstable();
    (strides, overrides)
}

/// Returns a level's branches from largest to smallest, breaking ties by branch index.
fn by_size(level: &[BranchSize]) -> Vec<BranchSize> {
    let mut sorted = level.to_vec();
    sorted.sort_by(|a, b| b.size.total_cmp(&a.size).then(a.index.cmp(&b.index)));
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates test levels from `(branch_count, segment_count)` pairs.
    /// Branch indices are consecutive; size is index plus one, so later branches rank larger.
    fn levels(counts: &[(usize, usize)]) -> Vec<Vec<BranchSize>> {
        let mut index = 0;
        counts
            .iter()
            .map(|&(branches, segments)| {
                (0..branches)
                    .map(|_| {
                        index += 1;
                        BranchSize {
                            index: index - 1,
                            segments,
                            size: index as f32,
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn fitting_covers_levels_before_refining() {
        // Three depths: one trunk, seven main branches, and 28 smaller branches.
        let levels = levels(&[(1, 12), (7, 10), (28, 4)]);
        // Of 64 available joints, 36 cover all branches and 25 improve spacing.
        // The last three cannot improve a whole level, so they refine the two
        // largest main branches: stride 2 (two more joints) and stride 3 (one more).
        assert_eq!(
            fitted_strides(&levels, 64),
            (vec![1, 4, 4], vec![(6, 3), (7, 2)])
        );
        // With 20 joints, eight cover the first two levels and eleven refine the trunk.
        // The last joint goes to the largest branch at the next depth.
        assert_eq!(
            fitted_strides(&levels, 20),
            (vec![1, 10], vec![(35, usize::MAX)])
        );
        // A budget of one leaves only the trunk's base joint.
        assert_eq!(fitted_strides(&levels, 1), (vec![12], vec![]));
        // A larger budget permits a joint at every section except each branch tip.
        assert_eq!(fitted_strides(&levels, 256), (vec![1, 1, 1], vec![]));
    }

    #[test]
    fn leftover_joints_go_to_the_largest_branches() {
        // The trunk uses 12 joints, leaving 52 for the largest of 90 limbs.
        // Those limbs have indices 39 through 90.
        let levels = levels(&[(1, 12), (90, 4)]);
        let (strides, partial) = fitted_strides(&levels, 64);
        assert_eq!(strides, [1]);
        let expected: Vec<(usize, usize)> = (39..=90).map(|b| (b, usize::MAX)).collect();
        assert_eq!(partial, expected);
    }

    #[test]
    fn explicit_stride_must_fit() {
        let levels = levels(&[(1, 12), (7, 10)]);
        assert_eq!(explicit_strides(&levels, 2, 64), Ok(vec![2, 2]));
        assert_eq!(
            explicit_strides(&levels, 1, 64),
            Err(RigError::TooManyJoints {
                needed: 82,
                max_joints: 64
            })
        );
    }
}
