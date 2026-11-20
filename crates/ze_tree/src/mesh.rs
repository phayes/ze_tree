//! Build bark and leaf triangle meshes from a [`Skeleton`].
//!
//! Levels of detail let an application use simpler meshes for distant trees.
//! Choose the mesh detail with [`LodDetail`]; camera-based switching is up to the application.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::ops::Range;

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::{
    BarkOptions, BarkUvMode, Billboard, Skeleton, SkeletonBranch, SkeletonLeaf, TreeOptions,
};

/// Settings for the amount of geometry produced by [`Skeleton::mesh`].
///
/// The default keeps all branch sections and leaves at their original size and detail.
/// Increase the strides or reduce `segment_factor` to create a simpler mesh for distant trees.
/// JSON uses ez-tree's `LODDetail` field names; omitted fields use their defaults.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LodDetail {
    /// Keep every Nth circular slice along each branch, starting at its base.
    /// The base and tip are always kept. For example, 2 skips every other interior slice.
    /// Both 0 and 1 keep every slice.
    pub section_stride: u32,
    /// Multiply each branch's original side count by this value, then round to a whole number.
    /// A value of 0.5 roughly halves the sides; each branch always has at least 3 sides.
    pub segment_factor: f32,
    /// Keep every Nth leaf in skeleton order, starting with leaf 0.
    /// For example, 2 keeps leaves 0, 2, 4, and so on. Both 0 and 1 keep every leaf.
    pub leaf_stride: u32,
    /// Multiply the width and height of each kept leaf by this value.
    /// Values above 1 can help fill gaps when fewer leaves are kept.
    pub leaf_scale: f32,
    /// Use one textured square per leaf or two squares crossed at right angles.
    /// These billboards have fixed orientations; they do not turn to face the camera.
    /// `None` uses [`LeafOptions::billboard`](crate::LeafOptions::billboard).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billboard: Option<Billboard>,
}

impl Default for LodDetail {
    fn default() -> Self {
        Self {
            section_stride: 1,
            segment_factor: 1.0,
            leaf_stride: 1,
            leaf_scale: 1.0,
            billboard: None,
        }
    }
}

/// A mesh detail setting and the camera distance at which an application should use it.
///
/// Pass `detail` to [`Skeleton::mesh`]. The application must select levels and switch meshes;
/// this crate does not track a camera or apply the distance and hysteresis settings.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LodLevel {
    /// Suggested starting distance from the camera, in the same units as the tree positions.
    pub distance: f32,
    /// Suggested switching margin as a fraction of `distance`.
    /// For example, 0.05 gives a 5% margin to help avoid rapid switching near a threshold.
    pub hysteresis: f32,
    /// Geometry settings to pass to [`Skeleton::mesh`] for this level.
    pub detail: LodDetail,
}

/// Returns ez-tree's three default detail levels, ordered from nearest to farthest.
///
/// Full detail starts at distance 0; simpler meshes start at distances 100 and 250.
/// The simpler levels keep fewer branch slices and leaves, with larger leaves to fill gaps.
/// Actual triangle counts depend on the skeleton. Applications handle switching between levels.
pub fn default_lod_levels() -> [LodLevel; 3] {
    [
        LodLevel::default(),
        LodLevel {
            distance: 100.0,
            hysteresis: 0.05,
            detail: LodDetail {
                section_stride: 3,
                segment_factor: 0.75,
                leaf_stride: 2,
                leaf_scale: 1.25,
                billboard: None,
            },
        },
        LodLevel {
            distance: 250.0,
            hysteresis: 0.05,
            detail: LodDetail {
                section_stride: 6,
                segment_factor: 0.4,
                leaf_stride: 2,
                leaf_scale: 1.3,
                billboard: Some(Billboard::Single),
            },
        },
    ]
}

/// Positions, lighting normals, texture coordinates, and triangle indices for one mesh.
///
/// Meshes returned by [`Skeleton::mesh`] have one normal and one UV pair per position.
/// When editing these public buffers, keep their lengths and indices consistent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshBuffers {
    /// Vertex coordinates in the same coordinate system as the skeleton.
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex lighting directions, in position order.
    /// Normally unit length; degenerate rounded leaf normals can be non-finite.
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex texture coordinates `(u, v)`, in position order.
    /// Leaf coordinates cover 0 to 1; bark coordinates can exceed 1 for repeating textures.
    pub uvs: Vec<[f32; 2]>,
    /// Directions used to orient normal-map lighting detail on the surface.
    /// Bark has one `[x, y, z, w]` tangent per position; leaves have none.
    /// The unit vector `[x, y, z]` points around the branch toward increasing `u`.
    /// The sign `w` gives the other texture direction as `cross(normal, xyz) * w`.
    /// Use [`BarkUvMode::Continuous`](crate::BarkUvMode::Continuous) for normal maps.
    /// These directions approximate a smooth branch surface; they may differ
    /// from triangle texture directions at sharp bends or twists.
    pub tangents: Vec<[f32; 4]>,
    /// Groups of three vertex indices, one group per triangle.
    /// Each index addresses `positions`, `normals`, and `uvs`. Vertices are ordered
    /// counter-clockwise when viewed from the front of a triangle.
    pub indices: Vec<u32>,
}

impl MeshBuffers {
    /// Returns the number of positions in this mesh, including duplicated vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Returns the number of complete groups of three indices.
    /// Any trailing indices in a manually edited buffer are not counted.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Returns the index for the next vertex, panicking if it cannot fit in `u32`.
    fn next_index(&self) -> u32 {
        u32::try_from(self.positions.len()).expect("mesh has more than u32::MAX vertices")
    }

    fn push(&mut self, position: Vec3, normal: Vec3, uv: [f32; 2]) {
        self.positions.push(position.to_array());
        self.normals.push(normal.to_array());
        self.uvs.push(uv);
    }
}

/// Separate bark and leaf meshes for one skeleton at a chosen level of detail.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreeMesh {
    /// All branches as tubes with open ends, ready for a bark material.
    pub branches: MeshBuffers,
    /// All leaves retained by the detail settings, ready for a leaf material.
    pub leaves: MeshBuffers,
    /// Buffer ranges for each skeleton branch, in the order of [`Skeleton::branches`].
    pub branch_spans: Vec<BranchSpan>,
    /// Buffer ranges for retained leaves, in the order of [`Skeleton::leaves`].
    pub leaf_spans: Vec<LeafSpan>,
}

impl TreeMesh {
    /// Returns the combined vertex count of the bark and leaf meshes.
    pub fn vertex_count(&self) -> usize {
        self.branches.vertex_count() + self.leaves.vertex_count()
    }

    /// Returns the combined triangle count of the bark and leaf meshes.
    pub fn triangle_count(&self) -> usize {
        self.branches.triangle_count() + self.leaves.triangle_count()
    }
}

/// Buffer ranges and source information for one branch in [`TreeMesh::branches`].
///
/// Vertices are grouped into rings from base to tip, one per retained section.
/// Each ring has `segments + 1` vertices: the last repeats the first position with a
/// different texture coordinate. This UV seam lets a texture wrap around the branch.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchSpan {
    /// Source branch index in [`Skeleton::branches`].
    pub branch: usize,
    /// Branching depth copied from the skeleton; the trunk is level 0.
    pub level: u32,
    /// Start-inclusive, end-exclusive range in the bark position, normal, and UV buffers.
    pub vertices: Range<u32>,
    /// Start-inclusive, end-exclusive range of entries in the bark index buffer.
    /// These are index-buffer offsets, not triangle numbers or vertex indices.
    pub indices: Range<u32>,
    /// Number of sides around each ring, excluding the duplicated seam vertex.
    pub segments: u32,
    /// Retained section indices within the source branch, in ring order from base to tip.
    pub sections: Vec<usize>,
}

/// Buffer ranges for one retained leaf in [`TreeMesh::leaves`].
#[derive(Debug, Clone, PartialEq)]
pub struct LeafSpan {
    /// Source leaf index in [`Skeleton::leaves`], before any leaves were skipped.
    pub leaf: usize,
    /// Start-inclusive, end-exclusive range in the leaf position, normal, and UV buffers.
    /// A single billboard uses 4 vertices; a double billboard uses 8.
    pub vertices: Range<u32>,
    /// Start-inclusive, end-exclusive range of entries in the leaf index buffer.
    /// A single billboard uses 6 entries; a double billboard uses 12. These are buffer offsets.
    pub indices: Range<u32>,
}

impl Skeleton {
    /// Builds bark and leaf triangle meshes using the supplied detail settings.
    ///
    /// Pass the options used to grow this skeleton and [`LodDetail::default()`] for full detail.
    /// From `options`, this uses only `bark.texture_scale.x`, `bark.uv_mode`,
    /// `leaves.billboard`, and `leaves.rounded_normals`; changing growth options
    /// here does not reshape the skeleton.
    /// A billboard override in `detail` takes precedence over the options.
    ///
    /// Repeated calls with the same inputs produce the same mesh without changing the skeleton.
    /// Use different detail settings to prepare meshes for different camera distances.
    /// Settings are not validated here. Rounded normals on double billboards can be unstable
    /// when the scaled leaf size is near 2, and can be non-finite at size 2.
    ///
    /// # Panics
    ///
    /// Panics if a branch has no sections; [`Skeleton::generate`] always supplies sections.
    /// Also panics if a vertex count or index-buffer length cannot fit in `u32`.
    pub fn mesh(&self, options: &TreeOptions, detail: &LodDetail) -> TreeMesh {
        let section_stride = detail.section_stride.max(1) as usize;
        let leaf_stride = detail.leaf_stride.max(1) as usize;
        let billboard = detail.billboard.unwrap_or(options.leaves.billboard);

        let mut mesh = TreeMesh::default();
        for (index, branch) in self.branches.iter().enumerate() {
            let span = mesh_branch(
                &mut mesh.branches,
                branch,
                index,
                section_stride,
                detail.segment_factor,
                &options.bark,
            );
            mesh.branch_spans.push(span);
        }
        for (index, leaf) in self.leaves.iter().enumerate().step_by(leaf_stride) {
            let span = mesh_leaf(
                &mut mesh.leaves,
                leaf,
                index,
                detail.leaf_scale,
                billboard,
                options.leaves.rounded_normals,
            );
            mesh.leaf_spans.push(span);
        }
        mesh
    }
}

/// Appends an uncapped branch tube and returns its buffer ranges and retained sections.
/// Requires at least one section and a nonzero `section_stride`.
fn mesh_branch(
    out: &mut MeshBuffers,
    branch: &SkeletonBranch,
    index: usize,
    section_stride: usize,
    segment_factor: f32,
    bark: &BarkOptions,
) -> BranchSpan {
    // A branch continuing its parent's tip inherits its side count. Applying the same
    // multiplier preserves matching rings at their joint.
    let segments = (branch.segment_count as f32 * segment_factor)
        .round()
        .max(3.0) as u32;

    // Repeat the bark texture a whole number of times around the branch. Wider bases
    // get more repeats, keeping bark features roughly the same size across branches.
    let wraps_x = (branch.base_radius * bark.texture_scale.x).round().max(1.0);

    // Always retain the tip, even when the stride would skip it.
    let last = branch.sections.len() - 1;
    let mut kept: Vec<usize> = (0..=last).step_by(section_stride).collect();
    if !last.is_multiple_of(section_stride) {
        kept.push(last);
    }

    // Duplicate the first vertex at each ring's texture seam: the same position needs
    // both the starting u coordinate (0) and the ending one (wraps_x).
    let first_vertex = out.next_index();
    let first_index = out.indices.len();
    for (k, &s) in kept.iter().enumerate() {
        let section = &branch.sections[s];
        let v = match bark.uv_mode {
            // Reverse the texture at each mesh ring, even at reduced detail.
            BarkUvMode::Mirrored => (k % 2) as f32,
            // Keep the original section coordinate when lower detail skips rings.
            BarkUvMode::Continuous => s as f32,
        };
        let mut seam = None;
        for j in 0..segments {
            let angle = TAU * j as f32 / segments as f32;
            let (sin, cos) = angle.sin_cos();
            let direction = section.orientation * Vec3::new(cos, 0.0, sin);
            let position = direction * section.radius + section.origin;
            let normal = direction.normalize();
            out.push(position, normal, [j as f32 / segments as f32 * wraps_x, v]);
            // The tangent follows increasing u around the ring. With w = -1,
            // cross(normal, tangent) * w points along the section's axis toward
            // the tip. Mirrored UVs reverse v without reversing this direction.
            let tangent = (section.orientation * Vec3::new(-sin, 0.0, cos)).normalize();
            out.tangents.push(tangent.extend(-1.0).to_array());
            seam.get_or_insert((position, normal, tangent));
        }
        let (position, normal, tangent) = seam.expect("a ring has at least 3 vertices");
        out.push(position, normal, [wraps_x, v]);
        out.tangents.push(tangent.extend(-1.0).to_array());
    }

    // Join each pair of neighboring rings with two triangles per side.
    let ring = segments + 1;
    for i in 0..kept.len() as u32 - 1 {
        for j in 0..segments {
            let v1 = first_vertex + i * ring + j;
            let v2 = v1 + 1;
            let v3 = v1 + ring;
            let v4 = v2 + ring;
            out.indices.extend_from_slice(&[v1, v3, v2, v2, v3, v4]);
        }
    }

    BranchSpan {
        branch: index,
        level: branch.level,
        vertices: first_vertex..out.next_index(),
        indices: index_range(first_index, out.indices.len()),
        segments,
        sections: kept,
    }
}

/// Appends one leaf as one square or two crossed squares, each made of two triangles.
/// Returns the appended ranges and records `index` as the source leaf index.
fn mesh_leaf(
    out: &mut MeshBuffers,
    leaf: &SkeletonLeaf,
    index: usize,
    scale: f32,
    billboard: Billboard,
    rounded_normals: bool,
) -> LeafSpan {
    let size = leaf.size * scale;
    let first_vertex = out.next_index();
    let first_index = out.indices.len();

    // The second square of a double billboard is rotated 90 degrees around the leaf stem.
    let turns: &[f32] = match billboard {
        Billboard::Single => &[0.0],
        Billboard::Double => &[0.0, FRAC_PI_2],
    };
    for &turn in turns {
        let rotation = leaf.orientation * Quat::from_rotation_y(turn);
        let corners = [
            Vec3::new(-size / 2.0, size, 0.0),
            Vec3::new(-size / 2.0, 0.0, 0.0),
            Vec3::new(size / 2.0, 0.0, 0.0),
            Vec3::new(size / 2.0, size, 0.0),
        ]
        .map(|corner| rotation * corner + leaf.origin);

        // The face normal ignores the billboard quarter turn, as in ez-tree:
        // both quads of a double billboard share it. Rounded normals add the offset
        // from the leaf origin to each corner to give the canopy softer lighting.
        let face = leaf.orientation * Vec3::Z;
        let uvs = [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let i = out.next_index();
        for (corner, uv) in corners.into_iter().zip(uvs) {
            // If the sum cancels to zero, the normal is zero, as with three.js
            // `normalize`, rather than NaN.
            let normal = if rounded_normals {
                (face + corner - leaf.origin).normalize_or_zero()
            } else {
                face
            };
            out.push(corner, normal, uv);
        }
        out.indices
            .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }

    LeafSpan {
        leaf: index,
        vertices: first_vertex..out.next_index(),
        indices: index_range(first_index, out.indices.len()),
    }
}

/// Converts buffer offsets to a half-open range, panicking if either offset exceeds `u32::MAX`.
fn index_range(start: usize, end: usize) -> Range<u32> {
    let convert = |i: usize| u32::try_from(i).expect("index buffer longer than u32::MAX");
    convert(start)..convert(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Section;

    fn straight_branch(sections: usize, segment_count: u32) -> SkeletonBranch {
        SkeletonBranch {
            level: 0,
            parent: None,
            sections: (0..sections)
                .map(|i| Section {
                    origin: Vec3::new(0.0, i as f32, 0.0),
                    orientation: Quat::IDENTITY,
                    radius: 1.0,
                })
                .collect(),
            segment_count,
            base_radius: 1.0,
        }
    }

    /// Builds test settings with the requested texture scale around the branch
    /// and texture direction along it. Other settings keep their defaults.
    fn bark(texture_scale_x: f32, uv_mode: BarkUvMode) -> BarkOptions {
        let mut bark = BarkOptions {
            uv_mode,
            ..BarkOptions::default()
        };
        bark.texture_scale.x = texture_scale_x;
        bark
    }

    #[test]
    fn stride_keeps_first_and_last_sections() {
        let branch = straight_branch(8, 4);
        let mirrored = bark(1.0, BarkUvMode::Mirrored);
        let mut out = MeshBuffers::default();
        let span = mesh_branch(&mut out, &branch, 0, 3, 1.0, &mirrored);
        assert_eq!(span.sections, vec![0, 3, 6, 7]);
        assert_eq!(span.vertices, 0..4 * 5);
        assert_eq!(span.indices, 0..3 * 4 * 6);

        // Retain the tip only once when the stride already includes it.
        let span = mesh_branch(&mut out, &straight_branch(7, 4), 1, 3, 1.0, &mirrored);
        assert_eq!(span.sections, vec![0, 3, 6]);
        assert_eq!(span.vertices, 20..35);
        assert_eq!(out.tangents.len(), out.positions.len());
    }

    #[test]
    fn segments_never_drop_below_three() {
        let mut out = MeshBuffers::default();
        let mirrored = bark(1.0, BarkUvMode::Mirrored);
        let span = mesh_branch(&mut out, &straight_branch(2, 5), 0, 1, 0.1, &mirrored);
        assert_eq!(span.segments, 3);
    }

    /// Checks texture coordinates along a branch when some mesh rings are skipped.
    #[test]
    fn uv_modes_set_v_per_ring() {
        let branch = straight_branch(8, 4);
        let ring_v = |mode| {
            let mut out = MeshBuffers::default();
            mesh_branch(&mut out, &branch, 0, 3, 1.0, &bark(1.0, mode));
            out.uvs
                .iter()
                .step_by(5)
                .map(|uv| uv[1])
                .collect::<Vec<_>>()
        };
        // Only sections 0, 3, 6, and 7 remain. Mirrored coordinates alternate;
        // continuous coordinates keep the original section numbers.
        assert_eq!(ring_v(BarkUvMode::Mirrored), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(ring_v(BarkUvMode::Continuous), [0.0, 3.0, 6.0, 7.0]);
    }

    #[test]
    fn ring_seam_repeats_first_vertex() {
        let mut out = MeshBuffers::default();
        mesh_branch(
            &mut out,
            &straight_branch(2, 4),
            0,
            1,
            1.0,
            &bark(2.6, BarkUvMode::Mirrored),
        );
        // A base radius of 1 and texture scale of 2.6 give three full texture repeats.
        assert_eq!(out.positions[4], out.positions[0]);
        assert_eq!(out.uvs[0], [0.0, 0.0]);
        assert_eq!(out.uvs[4], [3.0, 0.0]);
        assert_eq!(out.uvs[5], [0.0, 1.0]);
        // Check that the first triangle faces outward using its vertex order.
        let p = |i: u32| Vec3::from_array(out.positions[i as usize]);
        let [a, b, c] = [out.indices[0], out.indices[1], out.indices[2]];
        let facing = (p(b) - p(a)).cross(p(c) - p(a));
        assert!(facing.dot(Vec3::from_array(out.normals[a as usize])) > 0.0);
    }

    #[test]
    fn double_billboard_has_two_quads() {
        let leaf = SkeletonLeaf {
            parent: crate::Attachment {
                branch: 0,
                section: 0,
                t: 0.0,
            },
            origin: Vec3::new(1.0, 2.0, 3.0),
            orientation: Quat::IDENTITY,
            size: 2.0,
        };
        let mut out = MeshBuffers::default();
        let span = mesh_leaf(&mut out, &leaf, 7, 0.5, Billboard::Double, false);
        assert_eq!(span.vertices, 0..8);
        assert_eq!(span.indices, 0..12);
        assert_eq!(out.positions[0], [0.5, 3.0, 3.0]);
        // With this leaf orientation, the second square rotates 90 degrees around +Y.
        let turned = Vec3::from_array(out.positions[4]);
        assert!(
            turned.abs_diff_eq(Vec3::new(1.0, 3.0, 3.5), 1e-6),
            "{turned}"
        );
        // The second square shares the first square's lighting normal, matching ez-tree.
        assert_eq!(out.normals[4], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn lod_detail_reads_partial_json() {
        let detail: LodDetail = serde_json::from_str(r#"{"leafStride": 2}"#).unwrap();
        assert_eq!(
            detail,
            LodDetail {
                leaf_stride: 2,
                ..LodDetail::default()
            }
        );
    }
}
