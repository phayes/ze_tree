//! Build bark, leaf, fruit and flower triangle meshes from a [`Skeleton`].
//!
//! Levels of detail let an application use simpler meshes for distant trees.
//! Choose the mesh detail with [`LodDetail`]; camera-based switching is up to the application.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::ops::Range;

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::cluster_mesh::{mesh_flowers, mesh_fruit};
use crate::wind::{BranchWind, branch_winds, wind_at};
use crate::{
    BarkOptions, BarkUvMode, Billboard, Section, Skeleton, SkeletonBranch, SkeletonLeaf,
    TreeOptions,
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
    /// Scale the number of sides used to draw tube branches. For example, 0.5
    /// roughly halves the side count. The result is rounded, with at least three
    /// sides per branch. Has no effect on [`BranchShape::Ribbons`].
    pub segment_factor: f32,
    /// Branch surface: a round tube or two flat strips that cross at right angles.
    pub branch_shape: BranchShape,
    /// Minimum branch radius at the base, in tree units. Thinner branches are
    /// omitted, but the trunk is always kept. This does not affect leaf selection.
    /// Omitted branches still have an entry in [`TreeMesh::branch_spans`], with
    /// empty buffer ranges. Set to 0 to keep all branches.
    pub min_branch_radius: f32,
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
    /// Keep every Nth group of fruit and every Nth group of flowers, starting
    /// with group 0 in each skeleton list. Both 0 and 1 keep all groups.
    pub cluster_stride: u32,
    /// Keep every Nth fruit or flower in each retained cluster, starting with
    /// item 0, plus its supporting stalks. Both 0 and 1 keep all items.
    /// Ignored for catkins at [`ItemDetail::Minimal`].
    pub item_stride: u32,
    /// Multiply the dimensions of each retained fruit or flower by this value.
    /// Values above 1 help fill gaps left by skipped items. Supporting cluster
    /// stalks keep their size. Ignored for catkins at [`ItemDetail::Minimal`].
    pub item_scale: f32,
    /// Amount of shape detail to use for fruit, flowers, and their stalks.
    pub item_detail: ItemDetail,
    /// Draw fruit and flowers flat when their configured size
    /// ([`ClusterPlacement::size`](crate::ClusterPlacement::size)) is below
    /// this many tree units: each fruit becomes a rough disc and each flower a
    /// single triangle. Flat items keep every item, ignoring
    /// [`item_stride`](Self::item_stride), but are still resized by
    /// [`item_scale`](Self::item_scale). Catkins are not affected. 0 turns this off.
    pub flat_item_size: f32,
    /// Minimum configured fruit or flower size, in tree units. If
    /// [`size`](crate::ClusterPlacement::size) is below this threshold, that
    /// entire category is omitted, including stalks and catkins (hanging clusters).
    /// Its [`TreeMesh`] buffer and span list are empty. Set to 0 to disable this filter.
    pub min_item_size: f32,
}

impl Default for LodDetail {
    fn default() -> Self {
        Self {
            section_stride: 1,
            segment_factor: 1.0,
            branch_shape: BranchShape::Tube,
            min_branch_radius: 0.0,
            leaf_stride: 1,
            leaf_scale: 1.0,
            billboard: None,
            cluster_stride: 1,
            item_stride: 1,
            item_scale: 1.0,
            item_detail: ItemDetail::Full,
            flat_item_size: 0.0,
            min_item_size: 0.0,
        }
    }
}

/// How branch surfaces are drawn in [`TreeMesh::branches`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BranchShape {
    /// A round tube, open at both ends.
    #[default]
    Tube,
    /// Two flat strips running along the branch and crossing at right angles.
    /// Each is as wide as the branch. This uses fewer triangles than a tube
    /// and is intended for distant trees. Render both sides of the strips.
    Ribbons,
}

/// Controls how much geometry is used for fruit, flowers, and their stalks.
///
/// Lower detail uses fewer triangles for distant trees. It still uses the
/// configured maturity and colours, but small features may disappear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemDetail {
    /// Use the full fruit and flower shape settings.
    #[default]
    Full,
    /// Use fewer sides for fruit and stalks, and omit the calyx (the small
    /// flower remains on a fruit). Flowers have one petal layer with at most
    /// two divisions along each petal, and a cone-shaped centre.
    Simple,
    /// Use coarse fruit bodies without individual stems or calyxes, and a
    /// single star-shaped surface for each flower. Stalks become flat strips
    /// that face away from the trunk, so draw them two-sided. A catkin (a dense cluster
    /// along one stem) becomes one tapered body; its size and shape ignore
    /// [`LodDetail::item_stride`] and [`LodDetail::item_scale`].
    Minimal,
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

/// Returns four mesh detail levels, ordered from nearest to farthest.
///
/// Switching distances are 0, 100, 250 and 500 tree units. The first three match
/// ez-tree's branch and leaf settings: farther levels use fewer branch sections
/// and fewer, larger leaves. Your application must handle switching between meshes.
///
/// Fruit and flowers use [`ItemDetail::Simple`] at 100 and [`ItemDetail::Minimal`]
/// at 250. At 250, every other item is kept and enlarged by 25%; catkins become
/// a single body. Except for catkins, items with a configured size below 0.75
/// are drawn flat and are not skipped by the item stride.
///
/// At 500, branches use [`BranchShape::Ribbons`], which must be rendered on both
/// sides. Branches with a base radius below 0.15 are omitted, except for the trunk.
/// Every fourth leaf is kept at 1.8 times its original size, even on omitted branches.
/// Fruit and flowers with a configured size below 0.5 are omitted. Larger ones
/// use the settings at 250, with every other cluster kept, except that items
/// with a configured size below 1.5 are drawn flat.
pub fn default_lod_levels() -> [LodLevel; 4] {
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
                item_detail: ItemDetail::Simple,
                ..LodDetail::default()
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
                cluster_stride: 1,
                item_stride: 2,
                item_scale: 1.25,
                item_detail: ItemDetail::Minimal,
                flat_item_size: 0.75,
                ..LodDetail::default()
            },
        },
        LodLevel {
            distance: 500.0,
            hysteresis: 0.05,
            detail: LodDetail {
                section_stride: 8,
                branch_shape: BranchShape::Ribbons,
                min_branch_radius: 0.15,
                leaf_stride: 4,
                leaf_scale: 1.8,
                billboard: Some(Billboard::Single),
                cluster_stride: 2,
                item_stride: 2,
                item_scale: 1.25,
                item_detail: ItemDetail::Minimal,
                min_item_size: 0.5,
                flat_item_size: 1.5,
                ..LodDetail::default()
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
    /// Always unit length.
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex texture coordinates `(u, v)`, in position order.
    /// Leaf coordinates cover 0 to 1; bark coordinates can exceed 1 for repeating textures.
    pub uvs: Vec<[f32; 2]>,
    /// Per-vertex directions that orient normal-map lighting detail on the surface.
    /// Bark and leaves have one `[x, y, z, w]` entry per position; fruit and flowers
    /// have none. `[x, y, z]` is a unit tangent perpendicular to the vertex normal.
    /// The sign `w` gives a second direction, the bitangent, as `cross(normal, xyz) * w`.
    ///
    /// On bark, the tangent points around the branch toward increasing texture `u`,
    /// and the bitangent follows increasing `v`. Use [`BarkUvMode::Continuous`] for
    /// normal maps. These directions approximate a smooth surface and may differ
    /// from triangle texture directions at sharp bends or twists.
    ///
    /// On leaves, the tangent runs across the first square toward increasing `u`,
    /// adjusted to remain perpendicular to the lighting normal. The bitangent points
    /// toward the tip, or perpendicular to it for some rounded normals. Both squares
    /// of a double billboard use the first square's reference directions, matching
    /// the shared face normal; lighting on the second square does not follow its rotation.
    pub tangents: Vec<[f32; 4]>,
    /// Animation data for each vertex, in the same order as [`positions`](Self::positions).
    /// Pass each four-float value to `wind_offset` in [`WIND_WESL`](crate::WIND_WESL).
    ///
    /// The values are `[trunk_bend, branch_weight, branch_phase, flutter]`:
    /// - `trunk_bend` controls trunk movement, from 0 at ground level to 1 at the
    ///   highest skeleton section.
    /// - `branch_weight` controls branch movement, in tree units before applying
    ///   the wind settings. It is 0 on the trunk and increases toward branch tips.
    /// - `branch_phase` is a timing offset in `[0, 1)`. Each main limb shares it
    ///   with its descendants so they sway together.
    /// - `flutter` controls leaf movement: 0 for bark and leaf bases, 1 for leaf tips.
    ///
    /// Leaves inherit the first three values from their attachment points to follow
    /// their branches. Reducing mesh detail preserves the values at retained vertices.
    pub wind: Vec<[f32; 4]>,
    /// Per-vertex `[red, green, blue, alpha]` colours in linear colour space,
    /// ready for lighting. Fruit and flowers have one colour per position;
    /// multiply these by the material’s base colour when rendering. Bark and
    /// leaves have an empty buffer and use their textures instead.
    pub colors: Vec<[f32; 4]>,
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
    pub(crate) fn next_index(&self) -> u32 {
        u32::try_from(self.positions.len()).expect("mesh has more than u32::MAX vertices")
    }

    fn push(&mut self, position: Vec3, normal: Vec3, uv: [f32; 2], wind: [f32; 4]) {
        self.positions.push(position.to_array());
        self.normals.push(normal.to_array());
        self.uvs.push(uv);
        self.wind.push(wind);
    }

    /// Appends a position, lighting normal, texture coordinates, wind values,
    /// and linear RGBA colour. Use for buffers with one colour per vertex;
    /// does not add triangle indices or tangents.
    pub(crate) fn push_colored(
        &mut self,
        position: Vec3,
        normal: Vec3,
        uv: [f32; 2],
        wind: [f32; 4],
        color: [f32; 4],
    ) {
        self.push(position, normal, uv, wind);
        self.colors.push(color);
    }
}

/// Bark, leaf, fruit, and flower meshes for a skeleton, with ranges identifying
/// the source branches, leaves, and clusters. Detail settings affect bark and
/// leaves; fruit and flowers retain their full detail.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreeMesh {
    /// All branches as tubes with open ends, ready for a bark material.
    pub branches: MeshBuffers,
    /// All leaves retained by the detail settings, ready for a leaf material.
    pub leaves: MeshBuffers,
    /// Fruit bodies and supporting stalks, with per-vertex colours. Catkin fruit
    /// have no individual stems. Empty if the skeleton has no fruit clusters.
    pub fruit: MeshBuffers,
    /// Flowers and supporting stalks, with per-vertex colours. Empty if the
    /// skeleton has no flower clusters. Render both sides of petals by disabling
    /// back-face culling, since petals have no thickness.
    pub flowers: MeshBuffers,
    /// Buffer ranges for each skeleton branch, in the order of [`Skeleton::branches`].
    pub branch_spans: Vec<BranchSpan>,
    /// Buffer ranges for retained leaves, in the order of [`Skeleton::leaves`].
    pub leaf_spans: Vec<LeafSpan>,
    /// Ranges in [`fruit`](Self::fruit), one per cluster in [`Skeleton::fruit`]
    /// and in the same order. A cluster that produces no surface has empty ranges.
    pub fruit_spans: Vec<ClusterSpan>,
    /// Ranges in [`flowers`](Self::flowers), one per cluster in [`Skeleton::flowers`]
    /// and in the same order. A cluster that produces no surface has empty ranges.
    pub flower_spans: Vec<ClusterSpan>,
}

impl TreeMesh {
    /// Counts vertices across bark, leaves, fruit, and flowers, including duplicates.
    pub fn vertex_count(&self) -> usize {
        self.branches.vertex_count()
            + self.leaves.vertex_count()
            + self.fruit.vertex_count()
            + self.flowers.vertex_count()
    }

    /// Counts complete triangles across bark, leaves, fruit, and flowers.
    /// Trailing indices in a manually edited buffer are not counted.
    pub fn triangle_count(&self) -> usize {
        self.branches.triangle_count()
            + self.leaves.triangle_count()
            + self.fruit.triangle_count()
            + self.flowers.triangle_count()
    }
}

/// Identifies a source branch and its buffer ranges in [`TreeMesh::branches`].
///
/// Vertices are grouped by retained section, from base to tip. Each group (a
/// ring) contains [`ring_vertices`](Self::ring_vertices) vertices. For tubes,
/// the last vertex repeats the first position with a different texture coordinate
/// to close the texture seam. For ribbons, the four vertices are the two edges
/// of the first strip followed by the two edges of the second strip. Adjacent
/// rings are connected by `segments * 2` triangles.
///
/// Branches omitted by [`LodDetail::min_branch_radius`] have empty buffer ranges
/// and no retained sections.
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
    /// Whether this branch uses a tube or crossed strips.
    pub shape: BranchShape,
    /// Number of tube sides, excluding the repeated vertex at the texture seam.
    /// For ribbons, this counts strips instead and is always 2.
    pub segments: u32,
    /// Retained section indices within the source branch, in ring order from base to tip.
    pub sections: Vec<usize>,
}

impl BranchSpan {
    /// Returns the vertex count per retained section: `segments + 1` for tubes,
    /// or 4 for crossed ribbons. This describes the layout even for an omitted branch.
    pub fn ring_vertices(&self) -> u32 {
        match self.shape {
            BranchShape::Tube => self.segments + 1,
            BranchShape::Ribbons => self.segments * 2,
        }
    }
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

/// Locates one cluster’s fruit or flowers and supporting stalks in the
/// corresponding [`TreeMesh`] buffers. Ranges can be empty.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterSpan {
    /// Index of the source cluster in [`Skeleton::fruit`] for fruit spans,
    /// or [`Skeleton::flowers`] for flower spans.
    pub cluster: usize,
    /// Vertex offsets shared by the position, normal, UV, wind, and colour
    /// buffers of the corresponding fruit or flower mesh. Includes the start
    /// offset and excludes the end offset.
    pub vertices: Range<u32>,
    /// Entry offsets in the corresponding fruit or flower index buffer,
    /// including the start and excluding the end. Each triangle uses three
    /// entries; these offsets are not triangle numbers or vertex indices.
    pub indices: Range<u32>,
}

impl Skeleton {
    /// Builds bark, leaf, fruit, and flower triangle meshes from this skeleton.
    ///
    /// Pass the options used to generate the skeleton and [`LodDetail::default()`]
    /// for full detail. You can change fruit and flower shapes, maturity, and
    /// cluster stalk colours before meshing to change their appearance. Cluster
    /// kind selects catkin surfaces: rounded flowers and fruit without individual
    /// stems. Changing placement or growth settings here does not move clusters
    /// or reshape branches; generate a new skeleton for those changes.
    ///
    /// Bark texture scale and UV mode, leaf billboard and normal settings also
    /// affect the result. A billboard override in `detail` takes precedence.
    /// Use `detail` to reduce geometry for bark, leaves, fruit, and flowers.
    /// Each fruit or flower span identifies its original skeleton cluster,
    /// even when the detail settings skip other clusters.
    /// Repeated calls with identical inputs give identical meshes
    /// and leave the skeleton unchanged.
    ///
    /// Settings are not validated here; pass validated options. With rounded
    /// normals on double leaf billboards, one corner’s lighting direction flips
    /// as the scaled leaf size passes 2; at exactly 2 it uses the face normal.
    ///
    /// # Panics
    ///
    /// Panics if a branch has no sections; [`Skeleton::generate`] supplies them.
    /// Also panics if a vertex count or index-buffer length cannot fit in `u32`.
    pub fn mesh(&self, options: &TreeOptions, detail: &LodDetail) -> TreeMesh {
        let section_stride = detail.section_stride.max(1) as usize;
        let leaf_stride = detail.leaf_stride.max(1) as usize;
        let billboard = detail.billboard.unwrap_or(options.leaves.billboard);

        let winds = branch_winds(self);
        let mut mesh = TreeMesh::default();
        for (index, branch) in self.branches.iter().enumerate() {
            let shape = Shape::new(detail, branch);
            let span = if branch.level > 0 && branch.base_radius < detail.min_branch_radius {
                culled_branch(&mesh.branches, branch, index, shape)
            } else {
                mesh_branch(
                    &mut mesh.branches,
                    branch,
                    index,
                    section_stride,
                    shape,
                    &options.bark,
                    &winds[index],
                )
            };
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
                wind_at(&winds, &leaf.parent),
            );
            mesh.leaf_spans.push(span);
        }
        mesh.fruit_spans = mesh_fruit(&mut mesh.fruit, &self.fruit, &options.fruit, &winds, detail);
        mesh.flower_spans = mesh_flowers(
            &mut mesh.flowers,
            &self.flowers,
            &options.flowers,
            &winds,
            detail,
        );
        mesh
    }
}

/// Branch surface type and the number of sides or strips to generate.
#[derive(Debug, Clone, Copy)]
struct Shape {
    kind: BranchShape,
    /// Number of tube sides or ribbon strips.
    segments: u32,
}

impl Shape {
    /// Selects the surface from `detail` and scales tube side counts, keeping at
    /// least three sides. Ribbons always use two strips.
    fn new(detail: &LodDetail, branch: &SkeletonBranch) -> Self {
        match detail.branch_shape {
            // Tip continuations have the same original side count as their parent,
            // so scaling both counts equally keeps their connecting rings aligned.
            BranchShape::Tube => Self {
                kind: BranchShape::Tube,
                segments: (branch.segment_count as f32 * detail.segment_factor)
                    .round()
                    .max(3.0) as u32,
            },
            BranchShape::Ribbons => Self {
                kind: BranchShape::Ribbons,
                segments: 2,
            },
        }
    }
}

/// Records an omitted branch with empty ranges at the end of `out`.
/// Preserves its source index and shape, but retains no sections.
fn culled_branch(
    out: &MeshBuffers,
    branch: &SkeletonBranch,
    index: usize,
    shape: Shape,
) -> BranchSpan {
    let vertex = out.next_index();
    let entry = index_range(out.indices.len(), out.indices.len());
    BranchSpan {
        branch: index,
        level: branch.level,
        vertices: vertex..vertex,
        indices: entry,
        shape: shape.kind,
        segments: shape.segments,
        sections: Vec::new(),
    }
}

/// Appends a branch surface and returns its buffer ranges and source sections.
/// `shape` selects an open-ended tube or crossed strips. `branch` must contain
/// at least one section, and `section_stride` must be nonzero.
fn mesh_branch(
    out: &mut MeshBuffers,
    branch: &SkeletonBranch,
    index: usize,
    section_stride: usize,
    shape: Shape,
    bark: &BarkOptions,
    wind: &BranchWind,
) -> BranchSpan {
    let segments = shape.segments;

    // Use more texture repeats on wider branches to keep bark features
    // roughly the same size. Round to whole repeats for a matching tube seam.
    let wraps_x = (branch.base_radius * bark.texture_scale.x).round().max(1.0);

    // Always retain the tip, even when the stride would skip it.
    let last = branch.sections.len() - 1;
    let mut kept: Vec<usize> = (0..=last).step_by(section_stride).collect();
    if !last.is_multiple_of(section_stride) {
        kept.push(last);
    }

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
        // Give every vertex in this section the same wind motion, without leaf flutter.
        let ring_wind = [wind.bend[s], wind.weight[s], wind.phase, 0.0];
        match shape.kind {
            BranchShape::Tube => push_tube_ring(out, section, segments, wraps_x, v, ring_wind),
            BranchShape::Ribbons => push_ribbon_ring(out, section, wraps_x, v, ring_wind),
        }
    }

    // Connect adjacent sections with two triangles per tube side or ribbon strip.
    // Tube sides share vertices; each ribbon strip has its own two edges.
    let span = BranchSpan {
        branch: index,
        level: branch.level,
        vertices: first_vertex..first_vertex,
        indices: 0..0,
        shape: shape.kind,
        segments,
        sections: kept,
    };
    let ring = span.ring_vertices();
    let side_start = |j: u32| match shape.kind {
        BranchShape::Tube => j,
        BranchShape::Ribbons => 2 * j,
    };
    for i in 0..span.sections.len() as u32 - 1 {
        for j in 0..segments {
            let v1 = first_vertex + i * ring + side_start(j);
            let v2 = v1 + 1;
            let v3 = v1 + ring;
            let v4 = v2 + ring;
            out.indices.extend_from_slice(&[v1, v3, v2, v2, v3, v4]);
        }
    }

    BranchSpan {
        vertices: first_vertex..out.next_index(),
        indices: index_range(first_index, out.indices.len()),
        ..span
    }
}

/// Appends a tube cross-section with `segments` sides and a repeated first vertex.
/// The repeated position has a different texture coordinate so the bark image
/// can wrap around the tube. `segments` must be nonzero.
fn push_tube_ring(
    out: &mut MeshBuffers,
    section: &Section,
    segments: u32,
    wraps_x: f32,
    v: f32,
    ring_wind: [f32; 4],
) {
    let mut seam = None;
    for j in 0..segments {
        let angle = TAU * j as f32 / segments as f32;
        let (sin, cos) = angle.sin_cos();
        let direction = section.orientation * Vec3::new(cos, 0.0, sin);
        let position = direction * section.radius + section.origin;
        let normal = direction.normalize();
        out.push(
            position,
            normal,
            [j as f32 / segments as f32 * wraps_x, v],
            ring_wind,
        );
        // The tangent points around the tube in the direction of increasing u.
        // Its -1 handedness makes the bitangent point toward the branch tip,
        // including where mirrored texture coordinates reverse v.
        let tangent = (section.orientation * Vec3::new(-sin, 0.0, cos)).normalize();
        out.tangents.push(tangent.extend(-1.0).to_array());
        seam.get_or_insert((position, normal, tangent));
    }
    let (position, normal, tangent) = seam.expect("a ring has at least 3 vertices");
    out.push(position, normal, [wraps_x, v], ring_wind);
    out.tangents.push(tangent.extend(-1.0).to_array());
}

/// Appends four vertices: two edges for each of two perpendicular strips.
/// Each strip spans the branch diameter along the section's local X or Z axis.
/// Texture coordinates across each strip cover half of `wraps_x`.
fn push_ribbon_ring(
    out: &mut MeshBuffers,
    section: &Section,
    wraps_x: f32,
    v: f32,
    ring_wind: [f32; 4],
) {
    // Match normals to the triangle winding. As with tubes, tangent
    // handedness is -1 so the bitangent points toward the branch tip.
    for (width, face) in [(Vec3::X, Vec3::NEG_Z), (Vec3::Z, Vec3::X)] {
        let across = (section.orientation * width).normalize();
        let normal = (section.orientation * face).normalize();
        let tangent = across.extend(-1.0).to_array();
        for (side, u) in [(-1.0, 0.0), (1.0, wraps_x / 2.0)] {
            let position = section.origin + across * (side * section.radius);
            out.push(position, normal, [u, v], ring_wind);
            out.tangents.push(tangent);
        }
    }
}

/// Appends one leaf as one square or two crossed squares, each made of two triangles.
/// Returns the appended ranges and records `index` as the source leaf index.
/// `wind` supplies `[trunk_bend, branch_weight, branch_phase]` from the attachment
/// point so the leaf follows its branch.
fn mesh_leaf(
    out: &mut MeshBuffers,
    leaf: &SkeletonLeaf,
    index: usize,
    scale: f32,
    billboard: Billboard,
    rounded_normals: bool,
    wind: [f32; 3],
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
        let across = leaf.orientation * Vec3::X;
        let tip = leaf.orientation * Vec3::Y;
        let uvs = [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        // Let the tip corners flutter while keeping the base corners attached to the branch.
        let flutter = [1.0, 0.0, 0.0, 1.0];
        let i = out.next_index();
        for ((corner, uv), flutter) in corners.into_iter().zip(uvs).zip(flutter) {
            // If the sum cancels to zero, use the face normal so every normal stays unit length.
            let normal = if rounded_normals {
                (face + corner - leaf.origin).normalize_or(face)
            } else {
                face
            };
            out.push(corner, normal, uv, [wind[0], wind[1], wind[2], flutter]);

            // Project the first square's width direction onto the lighting surface.
            // It is never parallel to the normal, so the result can be normalized.
            // Choose w to keep the bitangent, cross(normal, tangent) * w, from
            // pointing away from the tip, even when rounded normals lean past it.
            let tangent = (across - normal * normal.dot(across)).normalize();
            let w = if normal.cross(tangent).dot(tip) < 0.0 {
                -1.0
            } else {
                1.0
            };
            out.tangents.push(tangent.extend(w).to_array());
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

    /// Gives each section distinct test values so incorrect wind-data indexing is visible.
    fn wind(branch: &SkeletonBranch) -> BranchWind {
        let n = branch.sections.len();
        BranchWind {
            bend: (0..n).map(|i| i as f32 / 10.0).collect(),
            weight: (0..n).map(|i| i as f32).collect(),
            phase: 0.5,
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

    /// Creates a tube shape for `branch` using the requested side-count multiplier.
    fn tube(branch: &SkeletonBranch, segment_factor: f32) -> Shape {
        let detail = LodDetail {
            segment_factor,
            ..LodDetail::default()
        };
        Shape::new(&detail, branch)
    }

    /// Creates a two-strip ribbon shape for `branch`.
    fn ribbons(branch: &SkeletonBranch) -> Shape {
        let detail = LodDetail {
            branch_shape: BranchShape::Ribbons,
            ..LodDetail::default()
        };
        Shape::new(&detail, branch)
    }

    #[test]
    fn stride_keeps_first_and_last_sections() {
        let branch = straight_branch(8, 4);
        let mirrored = bark(1.0, BarkUvMode::Mirrored);
        let mut out = MeshBuffers::default();
        let shape = tube(&branch, 1.0);
        let span = mesh_branch(&mut out, &branch, 0, 3, shape, &mirrored, &wind(&branch));
        assert_eq!(span.sections, vec![0, 3, 6, 7]);
        assert_eq!(span.vertices, 0..4 * 5);
        assert_eq!(span.indices, 0..3 * 4 * 6);
        // The second retained ring uses section 3's wind data for all five vertices,
        // including the duplicate vertex that closes the ring.
        assert_eq!(out.wind[5..10], [[0.3, 3.0, 0.5, 0.0]; 5]);

        // Retain the tip only once when the stride already includes it.
        let short = straight_branch(7, 4);
        let shape = tube(&short, 1.0);
        let span = mesh_branch(&mut out, &short, 1, 3, shape, &mirrored, &wind(&short));
        assert_eq!(span.sections, vec![0, 3, 6]);
        assert_eq!(span.vertices, 20..35);
        assert_eq!(out.tangents.len(), out.positions.len());
        assert_eq!(out.wind.len(), out.positions.len());
    }

    #[test]
    fn segments_never_drop_below_three() {
        let mut out = MeshBuffers::default();
        let mirrored = bark(1.0, BarkUvMode::Mirrored);
        let branch = straight_branch(2, 5);
        let shape = tube(&branch, 0.1);
        let span = mesh_branch(&mut out, &branch, 0, 1, shape, &mirrored, &wind(&branch));
        assert_eq!(span.segments, 3);
    }

    /// Checks ribbon vertex layout, surface directions, texture coordinates and wind data.
    #[test]
    fn ribbons_are_two_crossed_strips() {
        let branch = straight_branch(8, 6);
        let mut out = MeshBuffers::default();
        let span = mesh_branch(
            &mut out,
            &branch,
            0,
            3,
            ribbons(&branch),
            &bark(2.6, BarkUvMode::Mirrored),
            &wind(&branch),
        );

        // Four sections each have four vertices; each gap has two triangles per strip.
        assert_eq!(span.shape, BranchShape::Ribbons);
        assert_eq!((span.segments, span.ring_vertices()), (2, 4));
        assert_eq!(span.sections, vec![0, 3, 6, 7]);
        assert_eq!(span.vertices, 0..16);
        assert_eq!(span.indices, 0..3 * 2 * 6);
        assert_eq!(out.tangents.len(), 16);

        // Both strips span the branch diameter and cross through its centre.
        // All four vertices at a section receive the same wind data.
        let p = |i: u32| Vec3::from_array(out.positions[i as usize]);
        let ring = 4 * 2;
        assert!((p(ring) - Vec3::new(-1.0, 6.0, 0.0)).length() < 1e-6);
        assert!((p(ring + 1) - Vec3::new(1.0, 6.0, 0.0)).length() < 1e-6);
        assert!((p(ring + 2) - Vec3::new(0.0, 6.0, -1.0)).length() < 1e-6);
        assert!((p(ring + 3) - Vec3::new(0.0, 6.0, 1.0)).length() < 1e-6);
        assert_eq!(out.wind[8..12], [[0.6, 6.0, 0.5, 0.0]; 4]);

        // Each strip uses 1.5 texture repeats: half the tube's three repeats.
        assert_eq!(out.uvs[0][0], 0.0);
        assert_eq!(out.uvs[1][0], 1.5);

        // Triangle winding agrees with the normals. Tangents are perpendicular
        // to normals, and bitangents point toward the branch tip.
        for tri in out.indices.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]];
            let facing = (p(b) - p(a)).cross(p(c) - p(a)).normalize();
            let normal = Vec3::from_array(out.normals[a as usize]);
            assert!(facing.dot(normal) > 0.999, "{facing} vs {normal}");
        }
        for (t, n) in out.tangents.iter().zip(&out.normals) {
            let (tangent, n) = (Vec3::from_slice(t), Vec3::from_array(*n));
            assert!(tangent.dot(n).abs() < 1e-6);
            assert!((n.cross(tangent) * t[3]).abs_diff_eq(Vec3::Y, 1e-6));
        }
    }

    /// Checks texture coordinates along a branch when some mesh rings are skipped.
    #[test]
    fn uv_modes_set_v_per_ring() {
        let branch = straight_branch(8, 4);
        let ring_v = |mode| {
            let mut out = MeshBuffers::default();
            mesh_branch(
                &mut out,
                &branch,
                0,
                3,
                tube(&branch, 1.0),
                &bark(1.0, mode),
                &wind(&branch),
            );
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
        let branch = straight_branch(2, 4);
        mesh_branch(
            &mut out,
            &branch,
            0,
            1,
            tube(&branch, 1.0),
            &bark(2.6, BarkUvMode::Mirrored),
            &wind(&branch),
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
        let span = mesh_leaf(
            &mut out,
            &leaf,
            7,
            0.5,
            Billboard::Double,
            false,
            [0.2, 1.5, 0.25],
        );
        assert_eq!(span.vertices, 0..8);
        // Both leaf squares inherit branch movement at the attachment.
        // Their tip corners flutter; their base corners do not.
        assert_eq!(out.wind[0], [0.2, 1.5, 0.25, 1.0]);
        assert_eq!(out.wind[1], [0.2, 1.5, 0.25, 0.0]);
        let flutter: Vec<f32> = out.wind.iter().map(|w| w[3]).collect();
        assert_eq!(flutter, [1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);
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

    /// Checks the directions used for leaf normal maps with single and crossed squares.
    /// Tangents must be unit length, perpendicular to the normals, and point across
    /// the first square. Bitangents must not point away from the tip; with unrounded
    /// normals, they point directly toward it.
    #[test]
    fn leaf_tangents_form_a_frame_toward_u_and_the_tip() {
        let orientation = Quat::from_euler(glam::EulerRot::XYZ, 0.3, -0.7, 1.1);
        let leaf = SkeletonLeaf {
            parent: crate::Attachment {
                branch: 0,
                section: 0,
                t: 0.0,
            },
            origin: Vec3::new(1.0, 2.0, 3.0),
            orientation,
            size: 1.5,
        };
        let across = orientation * Vec3::X;
        let tip = orientation * Vec3::Y;
        for billboard in [Billboard::Single, Billboard::Double] {
            for rounded in [false, true] {
                // Exercise rounded lighting at leaf sizes below, at, and above 2.
                // At 2, one corner needs the fallback normal and another points
                // toward the tip. Larger sizes can tilt normals past the tip.
                for scale in [1.0, 2.0 / 1.5, 2.0] {
                    let mut out = MeshBuffers::default();
                    mesh_leaf(&mut out, &leaf, 0, scale, billboard, rounded, [0.0; 3]);
                    assert_eq!(out.tangents.len(), out.positions.len());
                    for (t, n) in out.tangents.iter().zip(&out.normals) {
                        let (tangent, n) = (Vec3::from_slice(t), Vec3::from_array(*n));
                        let case = format!("{billboard:?} rounded={rounded} scale={scale}");
                        assert!((tangent.length() - 1.0).abs() < 1e-5, "{case}: {tangent}");
                        assert!(tangent.dot(n).abs() < 1e-5, "{case}: {tangent} vs {n}");
                        assert!(t[3] == 1.0 || t[3] == -1.0, "{case}: {t:?}");
                        assert!(tangent.dot(across) > 0.0, "{case}: {tangent}");
                        let bitangent = n.cross(tangent) * t[3];
                        if rounded {
                            assert!(bitangent.dot(tip) >= -1e-6, "{case}: {bitangent}");
                        } else {
                            assert!(bitangent.abs_diff_eq(tip, 1e-5), "{case}: {bitangent}");
                        }
                    }
                }
            }
        }
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
