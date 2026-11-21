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
    /// Keep every Nth leaf on each branch, starting with the leaf nearest its tip
    /// and counting toward the base. For example, 2 keeps the last leaf, third-last
    /// leaf, and so on. Both 0 and 1 keep all leaves.
    pub leaf_stride: u32,
    /// Scale the stored width and height of each retained leaf card (a textured square).
    /// The card stays centred on the original leaf, so enlargement extends toward
    /// the stem as well as the tip. Use values above 1 to fill gaps between fewer leaves.
    pub leaf_scale: f32,
    /// Choose one textured square per leaf or two crossed squares.
    /// `None` uses [`LeafOptions::billboard`](crate::LeafOptions::billboard).
    /// This selects the number of squares; [`leaf_billboard`](Self::leaf_billboard)
    /// controls camera-facing rotation. With rotation enabled, the second square
    /// of [`Billboard::Double`] is stored in [`BillboardCull::leaves`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billboard: Option<Billboard>,
    /// Enable camera-facing leaf cards in renderers that support billboards.
    /// `None` (the default) leaves cards at fixed orientations.
    ///
    /// With `Some`, [`TreeMesh::leaves`] contains [`MeshBuffers::billboard`] data
    /// and no tangents. Each card keeps one lighting normal for all its corners:
    /// the leaf's face direction, or with
    /// [`LeafOptions::rounded_normals`](crate::LeafOptions::rounded_normals), the
    /// average of the rounded corner normals. Light the turned card with this
    /// normal, so it shades like the fixed leaf it replaces under any light, and
    /// show the card's back when the normal faces away from the camera.
    /// Renderers that ignore billboard data draw the stored cards at their
    /// fixed orientations and [`leaf_scale`](Self::leaf_scale) size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_billboard: Option<BillboardMode>,
    /// Scale the camera-facing card relative to the original leaf size.
    /// Used only with [`leaf_billboard`](Self::leaf_billboard), independently of
    /// [`leaf_scale`](Self::leaf_scale). Both sizes keep the original leaf centre.
    /// A card facing the camera can be smaller while still filling gaps in the canopy.
    pub leaf_billboard_scale: f32,
    /// Keep every Nth group of fruit and every Nth group of flowers, starting
    /// with group 0 in each skeleton list. Both 0 and 1 keep all groups.
    pub cluster_stride: u32,
    /// Keep every Nth fruit or flower within each retained cluster, starting with
    /// item 0, along with the stalks supporting it. Both 0 and 1 keep all items.
    /// Flat items and [`billboard_fruit`](Self::billboard_fruit) ignore this stride.
    pub item_stride: u32,
    /// Scale each retained fruit or flower, leaving supporting cluster stalks
    /// unchanged. Values above 1 help fill gaps left by skipped items.
    /// Has no effect on [`billboard_fruit`](Self::billboard_fruit).
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
    /// Replace fruit bodies with flat hexagons that supported renderers turn
    /// toward the camera. Stalks and individual stems are omitted.
    ///
    /// Each fruit has one hexagon in [`TreeMesh::fruit`], with
    /// [`MeshBuffers::billboard`] data and no normals, and a perpendicular fallback
    /// hexagon in [`BillboardCull::fruit`]. Renderers that turn the first skip the second.
    ///
    /// All fruit clusters and items keep their original sizes.
    /// [`cluster_stride`](Self::cluster_stride), [`item_stride`](Self::item_stride),
    /// [`item_scale`](Self::item_scale), [`item_detail`](Self::item_detail) and
    /// [`flat_item_size`](Self::flat_item_size) then affect flowers only.
    /// [`min_item_size`](Self::min_item_size) still filters fruit.
    pub billboard_fruit: bool,
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
            leaf_billboard: None,
            leaf_billboard_scale: 1.0,
            cluster_stride: 1,
            item_stride: 1,
            item_scale: 1.0,
            item_detail: ItemDetail::Full,
            flat_item_size: 0.0,
            min_item_size: 0.0,
            billboard_fruit: false,
        }
    }
}

/// How a renderer should rotate a flat shape using [`MeshBuffers::billboard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BillboardMode {
    /// Rotate around a fixed direction, such as a branch or leaf stem.
    /// The shape keeps its length and orientation along that axis.
    Axis,
    /// Face the camera, using the tree's +Y direction to keep the shape upright.
    Camera,
}

/// Position data a renderer uses to turn one vertex of a flat shape toward the camera.
/// All values are in tree coordinates. Stored mesh positions remain available for
/// renderers that do not support billboards and may describe a different-sized shape.
///
/// Use a unit `to_camera` direction from `pivot` to the camera. For an orthographic
/// view, use the direction opposite the camera's viewing direction. Reconstruct with:
///
/// ```text
/// position = pivot + offset[0] * right + offset[1] * up
/// ```
///
/// - For a nonzero `axis`, use `right = normalize(cross(axis, to_camera))` and `up = axis`.
/// - For a zero `axis`, use `right = normalize(cross(Y, to_camera))` and
///   `up = cross(to_camera, right)`, where `Y` is the tree's +Y direction.
///
/// If the cross product is zero, choose a perpendicular sideways direction instead
/// of normalizing zero. The resulting triangle fronts should face the camera.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BillboardVertex {
    /// Point this vertex rotates around, in tree coordinates. Ribbon vertices at
    /// different branch sections can have different pivots.
    pub pivot: [f32; 3],
    /// Unit-length rotation axis, or `[0.0; 3]` for full camera-facing rotation.
    pub axis: [f32; 3],
    /// Horizontal and vertical offsets from `pivot`, in tree units, along the
    /// reconstructed `right` and `up` directions described above.
    pub offset: [f32; 2],
}

/// How branch surfaces are drawn in [`TreeMesh::branches`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BranchShape {
    /// A round tube, open at both ends.
    #[default]
    Tube,
    /// Two perpendicular flat strips following the branch, each as wide as its
    /// diameter. Use these for distant trees and render both sides.
    ///
    /// [`TreeMesh::branches`] holds the first strip with [`MeshBuffers::billboard`]
    /// data and no normals or tangents. [`BillboardCull::branches`] holds the
    /// second strip. Renderers that rotate the first strip toward the camera
    /// should skip the second; other renderers should draw both.
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
    /// Use coarse fruit bodies without individual stems or flower remains
    /// (calyxes), and a star-shaped surface for each non-catkin flower.
    /// Supporting stalks become flat strips; render both sides. Catkins
    /// (hanging clusters along a stem) still draw their retained items separately.
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

/// Returns four suggested detail levels at distances of 0, 100, 250 and 500
/// tree units. The caller must select and switch meshes as the camera moves.
///
/// At 100, branches use fewer sections and sides, and every second leaf is kept
/// at 1.25 times its size. Fruit and flowers use [`ItemDetail::Simple`].
/// At 250, branches simplify further; every second leaf uses a single card at
/// 1.3 times its size. Fruit and flowers use [`ItemDetail::Minimal`], retaining
/// every second item at 1.25 times its size. Non-catkin items smaller than 0.75
/// configured tree units are drawn flat and keep every item.
///
/// At 500 (LOD3), renderers supporting [`MeshBuffers::billboard`] can turn
/// branch strips, leaf cards and fruit toward the camera:
///
/// - Branches use two-sided [`BranchShape::Ribbons`]. Branches with a base radius
///   below 0.15 are omitted, except for the trunk.
/// - Every fourth leaf is kept, counting back from each branch tip, including on
///   omitted branches. Fixed fallback cards use 1.8 times the original size;
///   camera-facing cards use 1.25 times.
/// - [`LodDetail::billboard_fruit`] keeps every fruit as crossed hexagons without stalks.
/// - Fruit and flowers with configured sizes below 0.5 are omitted. Flowers
///   otherwise use the 250 settings, but keep every second cluster and draw
///   non-catkin flowers smaller than 1.5 flat.
///
/// Renderers that rotate billboards should skip [`TreeMesh::billboard_cull`].
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
                leaf_billboard: Some(BillboardMode::Camera),
                leaf_billboard_scale: 1.25,
                cluster_stride: 2,
                item_stride: 2,
                item_scale: 1.25,
                item_detail: ItemDetail::Minimal,
                min_item_size: 0.5,
                flat_item_size: 1.5,
                billboard_fruit: true,
                ..LodDetail::default()
            },
        },
    ]
}

/// Vertex data and triangle indices for one mesh.
///
/// Meshes from [`Skeleton::mesh`] have one texture-coordinate pair per position.
/// Normals describe lighting directions. Billboard bark and fruit, and the bark
/// and fruit [`BillboardCull`] buffers, omit normals, so the renderer must supply
/// lighting directions; billboard leaves keep one normal per card. Billboard
/// meshes have no tangents.
/// When editing buffers, keep populated vertex attributes in position order and
/// ensure every triangle index refers to a valid position.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshBuffers {
    /// Vertex coordinates in the same coordinate system as the skeleton.
    pub positions: Vec<[f32; 3]>,
    /// Unit lighting directions in position order. Billboard bark and fruit, and
    /// their [`BillboardCull`] buffers, leave this empty; the renderer supplies
    /// lighting. Billboard leaves have one normal per card, repeated at each corner.
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
    /// Camera-facing position data, with one entry per position when present.
    /// Populated for ribbon bark, [`LodDetail::leaf_billboard`] leaves and
    /// [`LodDetail::billboard_fruit`] fruit. Such meshes omit
    /// [`tangents`](Self::tangents), and bark and fruit also omit
    /// [`normals`](Self::normals).
    /// Renderers without billboard support can use the stored positions.
    pub billboard: Vec<BillboardVertex>,
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
        self.push_unlit(position, uv, wind);
        self.normals.push(normal.to_array());
    }

    /// Adds a vertex with texture coordinates and wind controls, but no normal.
    /// Use for billboard shapes and their crossed fallback shapes.
    pub(crate) fn push_unlit(&mut self, position: Vec3, uv: [f32; 2], wind: [f32; 4]) {
        self.positions.push(position.to_array());
        self.uvs.push(uv);
        self.wind.push(wind);
    }

    /// Adds rotation data for the vertex just appended. Pass a unit `axis` for
    /// rotation around that axis, or zero for full camera-facing rotation.
    pub(crate) fn push_billboard(&mut self, pivot: Vec3, axis: Vec3, offset: [f32; 2]) {
        self.billboard.push(BillboardVertex {
            pivot: pivot.to_array(),
            axis: axis.to_array(),
            offset,
        });
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
    /// Crossed fallback shapes for bark, leaves and fruit. Draw these alongside
    /// the main buffers only when the renderer does not rotate billboards toward
    /// the camera.
    pub billboard_cull: BillboardCull,
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
    /// Returns the total vertex count, including duplicates and the fallback
    /// shapes in [`billboard_cull`](Self::billboard_cull).
    pub fn vertex_count(&self) -> usize {
        self.branches.vertex_count()
            + self.leaves.vertex_count()
            + self.fruit.vertex_count()
            + self.flowers.vertex_count()
            + self.billboard_cull.vertex_count()
    }

    /// Returns the total triangle count, including crossed fallback shapes.
    /// Subtract [`BillboardCull::triangle_count`] when drawing camera-facing
    /// billboards without those shapes. Incomplete trailing triangles in manually
    /// edited buffers are not counted.
    pub fn triangle_count(&self) -> usize {
        self.branches.triangle_count()
            + self.leaves.triangle_count()
            + self.fruit.triangle_count()
            + self.flowers.triangle_count()
            + self.billboard_cull.triangle_count()
    }
}

/// Fallback shapes perpendicular to the main billboards in a [`TreeMesh`].
///
/// Draw these with the corresponding main buffers when billboard rotation is
/// unavailable: crossed shapes help keep distant geometry visible from more angles.
/// Renderers that rotate the main shapes toward the camera should skip these buffers.
///
/// These buffers contain positions, texture coordinates, wind data and, for fruit,
/// colours. Leaf cards also have normals, matching their main cards. None have
/// tangents or billboard data. A buffer is empty when its part has no crossed pairs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BillboardCull {
    /// Fallback bark strips for [`BranchShape::Ribbons`]. Locate each branch with
    /// [`BranchSpan::cull_vertices`] and [`BranchSpan::cull_indices`].
    pub branches: MeshBuffers,
    /// Fallback leaf cards for [`Billboard::Double`] with
    /// [`LodDetail::leaf_billboard`] enabled. Locate each card with
    /// [`LeafSpan::cull_vertices`] and [`LeafSpan::cull_indices`].
    pub leaves: MeshBuffers,
    /// Fallback fruit hexagons for [`LodDetail::billboard_fruit`]. Locate each
    /// cluster with [`ClusterSpan::cull_vertices`] and [`ClusterSpan::cull_indices`].
    pub fruit: MeshBuffers,
}

impl BillboardCull {
    /// Returns the total fallback vertex count, including duplicate vertices.
    pub fn vertex_count(&self) -> usize {
        self.branches.vertex_count() + self.leaves.vertex_count() + self.fruit.vertex_count()
    }

    /// Returns the total fallback triangle count, ignoring incomplete trailing
    /// triangles in manually edited buffers.
    pub fn triangle_count(&self) -> usize {
        self.branches.triangle_count() + self.leaves.triangle_count() + self.fruit.triangle_count()
    }
}

/// Identifies a skeleton branch and its ranges in the main and fallback bark buffers.
///
/// Vertices are grouped by retained section, from base to tip. Each group has
/// [`ring_vertices`](Self::ring_vertices) vertices. Tube groups form circular rings,
/// with a repeated position at the texture seam. Ribbon groups contain two strip
/// edges; matching groups in [`BillboardCull::branches`] form the fallback strip.
/// Adjacent groups use `segments * 2` triangles per buffer.
///
/// Branches omitted by [`LodDetail::min_branch_radius`] have empty ranges and
/// no retained sections.
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
    /// Vertex range for the fallback bark strip in [`BillboardCull::branches`],
    /// including the start and excluding the end. Empty for tubes or omitted branches.
    pub cull_vertices: Range<u32>,
    /// Index-buffer entry range for the fallback strip in [`BillboardCull::branches`],
    /// including the start and excluding the end. Empty for tubes or omitted branches.
    pub cull_indices: Range<u32>,
    /// Whether this branch uses a tube or crossed strips.
    pub shape: BranchShape,
    /// Number of sides around a tube, excluding the duplicated seam vertex.
    /// Ribbon meshes use 1: one strip in each of the main and fallback buffers.
    pub segments: u32,
    /// Retained section indices within the source branch, in ring order from base to tip.
    pub sections: Vec<usize>,
}

impl BranchSpan {
    /// Returns the vertices per section in one buffer: `segments + 1` for tubes
    /// or 2 for generated ribbons. An omitted branch reports the same layout
    /// even though its ranges are empty.
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
    /// Start-inclusive, end-exclusive range in the main leaf vertex buffers.
    /// A card uses 4 vertices. Crossed cards use 8, or 4 when
    /// [`LodDetail::leaf_billboard`] puts the second card in [`BillboardCull::leaves`].
    /// Camera-facing cards have no tangent entries.
    pub vertices: Range<u32>,
    /// Entry range in the main leaf index buffer, including the start and excluding
    /// the end. Each card uses 6 entries (two triangles). These are buffer offsets,
    /// not vertex indices; a separate fallback card is covered by `cull_indices`.
    pub indices: Range<u32>,
    /// Vertex range of the fallback card in [`BillboardCull::leaves`], including
    /// the start and excluding the end. Empty unless [`Billboard::Double`] and
    /// [`LodDetail::leaf_billboard`] are both enabled.
    pub cull_vertices: Range<u32>,
    /// Entry range in the [`BillboardCull::leaves`] index buffer, including the
    /// start and excluding the end. Empty whenever
    /// [`cull_vertices`](Self::cull_vertices) is empty.
    pub cull_indices: Range<u32>,
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
    /// Start-inclusive, end-exclusive vertex range in [`BillboardCull::fruit`].
    /// Empty for flowers and for fruit without [`LodDetail::billboard_fruit`].
    pub cull_vertices: Range<u32>,
    /// Start-inclusive, end-exclusive entry range in the [`BillboardCull::fruit`]
    /// index buffer. Empty whenever [`cull_vertices`](Self::cull_vertices) is empty.
    pub cull_indices: Range<u32>,
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
        let cull = &mut mesh.billboard_cull;
        for (index, branch) in self.branches.iter().enumerate() {
            let shape = Shape::new(detail, branch);
            let span = if branch.level > 0 && branch.base_radius < detail.min_branch_radius {
                culled_branch(&mesh.branches, &cull.branches, branch, index, shape)
            } else {
                mesh_branch(
                    [&mut mesh.branches, &mut cull.branches],
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
        let style = LeafStyle {
            scale: detail.leaf_scale,
            billboard,
            rounded_normals: options.leaves.rounded_normals,
            turn: detail.leaf_billboard,
            turned_scale: detail.leaf_billboard_scale,
        };
        let kept = kept_leaves(&self.leaves, leaf_stride);
        for (index, leaf) in self.leaves.iter().enumerate().filter(|(i, _)| kept[*i]) {
            let span = mesh_leaf(
                [&mut mesh.leaves, &mut cull.leaves],
                leaf,
                index,
                &style,
                wind_at(&winds, &leaf.parent),
            );
            mesh.leaf_spans.push(span);
        }
        mesh.fruit_spans = mesh_fruit(
            [&mut mesh.fruit, &mut cull.fruit],
            &self.fruit,
            &options.fruit,
            &winds,
            detail,
        );
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
                segments: 1,
            },
        }
    }
}

/// Returns an empty span for an omitted branch, keeping its source index and
/// shape information. Both buffers remain unchanged.
fn culled_branch(
    out: &MeshBuffers,
    cull: &MeshBuffers,
    branch: &SkeletonBranch,
    index: usize,
    shape: Shape,
) -> BranchSpan {
    let empty = |buffers: &MeshBuffers| {
        let vertex = buffers.next_index();
        let entry = index_range(buffers.indices.len(), buffers.indices.len());
        (vertex..vertex, entry)
    };
    let ((vertices, indices), (cull_vertices, cull_indices)) = (empty(out), empty(cull));
    BranchSpan {
        branch: index,
        level: branch.level,
        vertices,
        indices,
        cull_vertices,
        cull_indices,
        shape: shape.kind,
        segments: shape.segments,
        sections: Vec::new(),
    }
}

/// Adds a branch to `out` and returns its ranges and retained section indices.
/// Ribbons also add a perpendicular fallback strip to `cull`. Pass a branch with
/// at least one section, a nonzero `section_stride`, and wind data for its sections.
fn mesh_branch(
    [out, cull]: [&mut MeshBuffers; 2],
    branch: &SkeletonBranch,
    index: usize,
    section_stride: usize,
    shape: Shape,
    bark: &BarkOptions,
    wind: &BranchWind,
) -> BranchSpan {
    let segments = shape.segments;

    // Repeat the bark image more often around wider branches. Whole repeats
    // keep the texture continuous across a tube seam.
    let wraps_x = (branch.base_radius * bark.texture_scale.x).round().max(1.0);

    // Include the branch tip even if it falls between stride steps.
    let last = branch.sections.len() - 1;
    let mut kept: Vec<usize> = (0..=last).step_by(section_stride).collect();
    if !last.is_multiple_of(section_stride) {
        kept.push(last);
    }

    let (first_vertex, first_index) = (out.next_index(), out.indices.len());
    let (first_cull_vertex, first_cull_index) = (cull.next_index(), cull.indices.len());
    for (k, &s) in kept.iter().enumerate() {
        let section = &branch.sections[s];
        let v = match bark.uv_mode {
            // Alternate texture direction between retained sections.
            BarkUvMode::Mirrored => (k % 2) as f32,
            // Preserve texture placement along the branch when sections are skipped.
            BarkUvMode::Continuous => s as f32,
        };
        // Move all vertices at this section together, without leaf flutter.
        let ring_wind = [wind.bend[s], wind.weight[s], wind.phase, 0.0];
        match shape.kind {
            BranchShape::Tube => push_tube_ring(out, section, segments, wraps_x, v, ring_wind),
            BranchShape::Ribbons => {
                push_ribbon_ring([out, cull], section, wraps_x, v, ring_wind);
            }
        }
    }

    // Join neighbouring sections with two triangles per tube side or per strip.
    // The main and fallback ribbon strips have separate index buffers.
    let span = BranchSpan {
        branch: index,
        level: branch.level,
        vertices: first_vertex..first_vertex,
        indices: 0..0,
        cull_vertices: first_cull_vertex..first_cull_vertex,
        cull_indices: 0..0,
        shape: shape.kind,
        segments,
        sections: kept,
    };
    let ring = span.ring_vertices();
    let rings = span.sections.len() as u32;
    connect_rings(out, first_vertex, rings, ring, segments);
    if shape.kind == BranchShape::Ribbons {
        connect_rings(cull, first_cull_vertex, rings, ring, segments);
    }

    BranchSpan {
        vertices: first_vertex..out.next_index(),
        indices: index_range(first_index, out.indices.len()),
        cull_vertices: first_cull_vertex..cull.next_index(),
        cull_indices: index_range(first_cull_index, cull.indices.len()),
        ..span
    }
}

/// Joins successive rings with two triangles per side. `first` is the starting
/// vertex, `rings` is the ring count, and `ring` is the vertices per ring. Each
/// side joins neighbouring vertices within a ring to those in the next ring.
fn connect_rings(out: &mut MeshBuffers, first: u32, rings: u32, ring: u32, sides: u32) {
    for i in 0..rings.saturating_sub(1) {
        for j in 0..sides {
            let v1 = first + i * ring + j;
            let v2 = v1 + 1;
            let v3 = v1 + ring;
            let v4 = v2 + ring;
            out.indices.extend_from_slice(&[v1, v3, v2, v2, v3, v4]);
        }
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

/// Adds two edge vertices at this section to each ribbon buffer. The main and
/// fallback strips cross along the section’s local X and Z directions and span
/// the branch diameter. Each uses half of `wraps_x` texture repeats. Only the
/// main strip gets data for camera-facing rotation around the branch.
fn push_ribbon_ring(
    [out, cull]: [&mut MeshBuffers; 2],
    section: &Section,
    wraps_x: f32,
    v: f32,
    ring_wind: [f32; 4],
) {
    let axis = (section.orientation * Vec3::Y).normalize();
    for (k, width) in [Vec3::X, Vec3::Z].into_iter().enumerate() {
        let across = (section.orientation * width).normalize();
        for (side, u) in [(-1.0, 0.0), (1.0, wraps_x / 2.0)] {
            let offset = side * section.radius;
            let position = section.origin + across * offset;
            if k == 0 {
                out.push_unlit(position, [u, v], ring_wind);
                // Reverse the offset so the rotated strip keeps its front toward the
                // camera; viewed from local -Z, its right direction is local -X.
                out.push_billboard(section.origin, axis, [-offset, 0.0]);
            } else {
                cull.push_unlit(position, [u, v], ring_wind);
            }
        }
    }
}

/// Size, card count and lighting settings shared by the mesh’s leaves.
struct LeafStyle {
    /// Size multiplier for fixed fallback cards.
    scale: f32,
    /// One card or a pair crossed around the leaf stem.
    billboard: Billboard,
    /// Whether cards use softened canopy lighting. Rotated cards use the
    /// average of the rounded corner normals for the whole card.
    rounded_normals: bool,
    /// Rotation mode for camera-facing cards; `None` keeps fixed cards.
    turn: Option<BillboardMode>,
    /// Size multiplier for camera-facing cards, independent of the stored size.
    turned_scale: f32,
}

/// Adds one leaf card or a crossed pair and returns its buffer ranges, recording
/// `index` as the source leaf. With rotation enabled in `style`, the first card
/// goes into `out` with billboard data and the second, if present, into `cull`.
/// Rotated cards have one normal per card and no tangents. Fixed cards all go
/// into `out` with per-corner normals and tangents.
/// Pass attachment wind values as `[trunk_bend, branch_weight, branch_phase]`.
fn mesh_leaf(
    [out, cull]: [&mut MeshBuffers; 2],
    leaf: &SkeletonLeaf,
    index: usize,
    style: &LeafStyle,
    wind: [f32; 3],
) -> LeafSpan {
    // Keep the original centre so resizing extends equally toward the stem and tip.
    let half = leaf.size * style.scale / 2.0;
    let centre = leaf.size / 2.0;
    let (first_vertex, first_index) = (out.next_index(), out.indices.len());
    let (first_cull_vertex, first_cull_index) = (cull.next_index(), cull.indices.len());

    // Cross the second card at a right angle around the leaf stem.
    let turns: &[f32] = match style.billboard {
        Billboard::Single => &[0.0],
        Billboard::Double => &[0.0, FRAC_PI_2],
    };
    for (k, &turn) in turns.iter().enumerate() {
        let rotation = leaf.orientation * Quat::from_rotation_y(turn);
        let corners = [
            Vec3::new(-half, centre + half, 0.0),
            Vec3::new(-half, centre - half, 0.0),
            Vec3::new(half, centre - half, 0.0),
            Vec3::new(half, centre + half, 0.0),
        ]
        .map(|corner| rotation * corner + leaf.origin);

        // Fixed crossed cards share the first card’s lighting direction, matching
        // ez-tree. Rounded normals tilt it toward each corner for softer canopy lighting.
        let face = leaf.orientation * Vec3::Z;
        let across = leaf.orientation * Vec3::X;
        let tip = leaf.orientation * Vec3::Y;
        let uvs = [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        // Apply flutter at the tip corners only; all corners still follow branch motion.
        let flutter = [1.0, 0.0, 0.0, 1.0];

        // Billboard cards keep one lighting direction for the whole card: the
        // leaf's face, or with rounded normals, the average of its rounded
        // corner normals. Only the main card needs rotation data.
        if let Some(mode) = style.turn {
            let normal = if style.rounded_normals {
                (face + tip * (leaf.size / 2.0)).normalize_or(face)
            } else {
                face
            };
            let target = if k == 0 { &mut *out } else { &mut *cull };
            let i = target.next_index();
            for (c, ((corner, uv), flutter)) in
                corners.into_iter().zip(uvs).zip(flutter).enumerate()
            {
                target.push(corner, normal, uv, [wind[0], wind[1], wind[2], flutter]);
                if k == 0 {
                    let (pivot, axis, offset) = leaf_billboard(leaf, mode, style.turned_scale, c);
                    target.push_billboard(pivot, axis, offset);
                }
            }
            target
                .indices
                .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
            continue;
        }

        let i = out.next_index();
        for ((corner, uv), flutter) in corners.into_iter().zip(uvs).zip(flutter) {
            // Use the face direction when rounding would produce a zero-length normal.
            let normal = if style.rounded_normals {
                (face + corner - leaf.origin).normalize_or(face)
            } else {
                face
            };
            out.push(corner, normal, uv, [wind[0], wind[1], wind[2], flutter]);

            // Align the normal-map tangent with the first card’s width. Choose the
            // bitangent sign so its direction does not point away from the leaf tip.
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
        cull_vertices: first_cull_vertex..cull.next_index(),
        cull_indices: index_range(first_cull_index, cull.indices.len()),
    }
}

/// Returns rotation data for a leaf corner at `scale` times the original size.
/// Corner indices are 0–3: top left, bottom left, bottom right, top right.
/// Camera mode pivots at the centre; axis mode pivots at the stem base and
/// rotates around the stem. Both preserve the original leaf centre.
fn leaf_billboard(
    leaf: &SkeletonLeaf,
    mode: BillboardMode,
    scale: f32,
    corner: usize,
) -> (Vec3, Vec3, [f32; 2]) {
    let half = leaf.size * scale / 2.0;
    let across = if corner < 2 { -half } else { half };
    let along = if corner == 1 || corner == 2 {
        -half
    } else {
        half
    };
    let tip = (leaf.orientation * Vec3::Y).normalize();
    match mode {
        BillboardMode::Camera => (
            leaf.origin + tip * (leaf.size / 2.0),
            Vec3::ZERO,
            [across, along],
        ),
        BillboardMode::Axis => (leaf.origin, tip, [across, leaf.size / 2.0 + along]),
    }
}

/// Marks retained leaves in input order. Within each branch, keep every
/// `stride`th leaf starting nearest the tip. Strides 0 and 1 keep all leaves.
fn kept_leaves(leaves: &[SkeletonLeaf], stride: usize) -> Vec<bool> {
    let mut kept = vec![stride <= 1; leaves.len()];
    if stride <= 1 {
        return kept;
    }

    // Sort each branch’s leaves from tip to base before applying the stride.
    let mut order: Vec<usize> = (0..leaves.len()).collect();
    let along = |i: usize| leaves[i].parent.section as f32 + leaves[i].parent.t;
    order.sort_by(|&a, &b| {
        let (pa, pb) = (&leaves[a].parent, &leaves[b].parent);
        pa.branch
            .cmp(&pb.branch)
            .then(along(b).total_cmp(&along(a)))
            .then(a.cmp(&b))
    });
    for group in order.chunk_by(|&a, &b| leaves[a].parent.branch == leaves[b].parent.branch) {
        for &i in group.iter().step_by(stride) {
            kept[i] = true;
        }
    }
    kept
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
        let mut cull = MeshBuffers::default();
        let buffers = [&mut out, &mut cull];
        let span = mesh_branch(buffers, &branch, 0, 3, shape, &mirrored, &wind(&branch));
        assert_eq!(span.sections, vec![0, 3, 6, 7]);
        assert_eq!(span.vertices, 0..4 * 5);
        assert_eq!(span.indices, 0..3 * 4 * 6);
        // All five vertices of the second retained ring, including its seam
        // duplicate, must use section 3’s wind values.
        assert_eq!(out.wind[5..10], [[0.3, 3.0, 0.5, 0.0]; 5]);

        // A tip already selected by the stride must not be duplicated.
        let short = straight_branch(7, 4);
        let shape = tube(&short, 1.0);
        let buffers = [&mut out, &mut cull];
        let span = mesh_branch(buffers, &short, 1, 3, shape, &mirrored, &wind(&short));
        assert_eq!(span.sections, vec![0, 3, 6]);
        assert_eq!(span.vertices, 20..35);
        // Tube branches must not add fallback ribbon geometry.
        assert!(span.cull_vertices.is_empty() && cull.positions.is_empty());
        assert_eq!(out.tangents.len(), out.positions.len());
        assert_eq!(out.wind.len(), out.positions.len());
    }

    #[test]
    fn segments_never_drop_below_three() {
        let mut out = MeshBuffers::default();
        let mirrored = bark(1.0, BarkUvMode::Mirrored);
        let branch = straight_branch(2, 5);
        let shape = tube(&branch, 0.1);
        let buffers = [&mut out, &mut MeshBuffers::default()];
        let span = mesh_branch(buffers, &branch, 0, 1, shape, &mirrored, &wind(&branch));
        assert_eq!(span.segments, 3);
    }

    /// Creates fixed leaf cards with the requested size, card count and lighting.
    fn fixed_leaves(scale: f32, billboard: Billboard, rounded_normals: bool) -> LeafStyle {
        LeafStyle {
            scale,
            billboard,
            rounded_normals,
            turn: None,
            turned_scale: 1.0,
        }
    }

    /// Reconstructs a vertex for a unit camera direction that is not parallel to
    /// its rotation axis (or +Y for camera mode).
    fn turned(b: &BillboardVertex, to_camera: Vec3) -> Vec3 {
        let (pivot, axis) = (Vec3::from_array(b.pivot), Vec3::from_array(b.axis));
        let (right, up) = if axis == Vec3::ZERO {
            let right = Vec3::Y.cross(to_camera).normalize();
            (right, to_camera.cross(right))
        } else {
            (axis.cross(to_camera).normalize(), axis)
        };
        pivot + right * b.offset[0] + up * b.offset[1]
    }

    /// Checks that reconstructed triangle fronts point toward the camera.
    fn assert_turned_front_faces(buffers: &MeshBuffers, to_camera: Vec3) {
        let p = |i: u32| turned(&buffers.billboard[i as usize], to_camera);
        for tri in buffers.indices.chunks(3) {
            let [a, b, c] = [p(tri[0]), p(tri[1]), p(tri[2])];
            let facing = (b - a).cross(c - a);
            assert!(facing.dot(to_camera) > 0.0, "{facing} vs {to_camera}");
        }
    }

    /// Samples views around the tree, avoiding directions parallel to +Y.
    fn camera_directions() -> Vec<Vec3> {
        (0..12)
            .map(|k| {
                let angle = TAU * k as f32 / 12.0;
                Vec3::new(angle.cos(), 0.4 * (k as f32 - 6.0) / 6.0, angle.sin()).normalize()
            })
            .collect()
    }

    /// Checks that ribbons have separate main and fallback strips with matching
    /// texture and wind data, and that the main strip can face each sampled view.
    #[test]
    fn ribbons_are_two_crossed_strips() {
        let branch = straight_branch(8, 6);
        let (mut out, mut cull) = (MeshBuffers::default(), MeshBuffers::default());
        let span = mesh_branch(
            [&mut out, &mut cull],
            &branch,
            0,
            3,
            ribbons(&branch),
            &bark(2.6, BarkUvMode::Mirrored),
            &wind(&branch),
        );

        // Each retained section contributes two edges per strip; each gap
        // between sections contributes two triangles per strip.
        assert_eq!(span.shape, BranchShape::Ribbons);
        assert_eq!((span.segments, span.ring_vertices()), (1, 2));
        assert_eq!(span.sections, vec![0, 3, 6, 7]);
        assert_eq!(
            (span.vertices.clone(), span.cull_vertices.clone()),
            (0..8, 0..8)
        );
        assert_eq!(
            (span.indices.clone(), span.cull_indices.clone()),
            (0..18, 0..18)
        );
        assert!(out.normals.is_empty() && out.tangents.is_empty());
        assert!(cull.normals.is_empty() && cull.tangents.is_empty());
        assert_eq!(out.billboard.len(), 8);
        assert!(cull.billboard.is_empty());

        // Both strips cross the branch centre and span its diameter. At a given
        // section, their vertices must move together in the wind.
        let p = |b: &MeshBuffers, i: usize| Vec3::from_array(b.positions[i]);
        assert!(p(&out, 4).abs_diff_eq(Vec3::new(-1.0, 6.0, 0.0), 1e-6));
        assert!(p(&out, 5).abs_diff_eq(Vec3::new(1.0, 6.0, 0.0), 1e-6));
        assert!(p(&cull, 4).abs_diff_eq(Vec3::new(0.0, 6.0, -1.0), 1e-6));
        assert!(p(&cull, 5).abs_diff_eq(Vec3::new(0.0, 6.0, 1.0), 1e-6));
        assert_eq!(out.wind[4..6], [[0.6, 6.0, 0.5, 0.0]; 2]);
        assert_eq!(cull.wind[4..6], [[0.6, 6.0, 0.5, 0.0]; 2]);

        // Each strip covers half the tube’s texture width: 1.5 repeats here.
        assert_eq!((out.uvs[0][0], out.uvs[1][0]), (0.0, 1.5));
        assert_eq!((cull.uvs[0][0], cull.uvs[1][0]), (0.0, 1.5));

        // The main strip pivots along the branch centreline. Looking from its
        // stored front (-Z) must reproduce its stored vertex positions.
        for (i, b) in out.billboard.iter().enumerate() {
            let section = span.sections[i / 2] as f32;
            assert_eq!(b.pivot, [0.0, section, 0.0]);
            assert_eq!(b.axis, [0.0, 1.0, 0.0]);
            assert!(
                turned(b, Vec3::NEG_Z).abs_diff_eq(p(&out, i), 1e-6),
                "vertex {i}"
            );
        }
        for to_camera in camera_directions() {
            assert_turned_front_faces(&out, to_camera);
        }
    }

    /// Checks that camera-facing cards without rounded normals carry the leaf's
    /// face direction at every corner.
    #[test]
    fn billboard_leaves_without_rounding_use_the_face_normal() {
        let orientation = Quat::from_euler(glam::EulerRot::XYZ, 0.3, -0.7, 1.1);
        let leaf = SkeletonLeaf {
            parent: crate::Attachment {
                branch: 0,
                section: 0,
                t: 0.0,
            },
            origin: Vec3::ZERO,
            orientation,
            size: 2.0,
        };
        let style = LeafStyle {
            scale: 1.8,
            billboard: Billboard::Single,
            rounded_normals: false,
            turn: Some(BillboardMode::Camera),
            turned_scale: 1.25,
        };
        let mut out = MeshBuffers::default();
        mesh_leaf(
            [&mut out, &mut MeshBuffers::default()],
            &leaf,
            0,
            &style,
            [0.0; 3],
        );
        let face = orientation * Vec3::Z;
        assert_eq!(out.normals.len(), 4);
        for n in &out.normals {
            assert!(Vec3::from_array(*n).abs_diff_eq(face, 1e-6), "{n:?}");
        }
    }

    /// Checks both rotation modes and card counts: stored and camera-facing cards
    /// use their own size multipliers while sharing the original leaf centre.
    #[test]
    fn billboard_leaves_turn_around_the_leaf_centre() {
        let orientation = Quat::from_euler(glam::EulerRot::XYZ, 0.3, -0.7, 1.1);
        let leaf = SkeletonLeaf {
            parent: crate::Attachment {
                branch: 0,
                section: 0,
                t: 0.0,
            },
            origin: Vec3::new(1.0, 2.0, 3.0),
            orientation,
            size: 2.0,
        };
        let centre = leaf.origin + orientation * Vec3::Y;
        for mode in [BillboardMode::Camera, BillboardMode::Axis] {
            for billboard in [Billboard::Single, Billboard::Double] {
                let style = LeafStyle {
                    scale: 1.8,
                    billboard,
                    rounded_normals: true,
                    turn: Some(mode),
                    turned_scale: 1.25,
                };
                let (mut out, mut cull) = (MeshBuffers::default(), MeshBuffers::default());
                let span = mesh_leaf([&mut out, &mut cull], &leaf, 0, &style, [0.0; 3]);
                let case = format!("{mode:?} {billboard:?}");

                // Rotation data belongs to the main card; any crossed card is separate.
                assert_eq!(span.vertices, 0..4, "{case}");
                assert!(out.tangents.is_empty(), "{case}");
                assert_eq!(out.billboard.len(), 4, "{case}");
                let crossed = billboard == Billboard::Double;
                assert_eq!(span.cull_vertices, 0..if crossed { 4 } else { 0 }, "{case}");
                assert!(
                    cull.billboard.is_empty() && cull.tangents.is_empty(),
                    "{case}"
                );

                // Every corner of both cards carries the average rounded normal:
                // the face tilted toward the tip by half the leaf size.
                let face = orientation * Vec3::Z;
                let expected = (face + orientation * Vec3::Y * 1.0).normalize();
                assert_eq!(cull.normals.len(), cull.positions.len(), "{case}");
                for n in out.normals.iter().chain(&cull.normals) {
                    let n = Vec3::from_array(*n);
                    assert!(n.abs_diff_eq(expected, 1e-5), "{case}: {n}");
                }
                assert_eq!(out.normals.len(), 4, "{case}");

                // Fallback cards use the 1.8 size multiplier without shifting the centre.
                let stored: Vec<Vec3> = out.positions.iter().map(|&p| p.into()).collect();
                let mean = stored.iter().sum::<Vec3>() / 4.0;
                assert!(mean.abs_diff_eq(centre, 1e-5), "{case}: {mean}");
                assert!((stored[1].distance(stored[2]) - 3.6).abs() < 1e-5, "{case}");
                if crossed {
                    let second = (0..4).map(|i| Vec3::from(cull.positions[i])).sum::<Vec3>();
                    assert!((second / 4.0).abs_diff_eq(centre, 1e-5), "{case}");
                }

                // Camera-facing cards use the independent 1.25 multiplier at the same centre.
                for to_camera in camera_directions() {
                    let corners: Vec<Vec3> =
                        out.billboard.iter().map(|b| turned(b, to_camera)).collect();
                    let mean = corners.iter().sum::<Vec3>() / 4.0;
                    assert!(mean.abs_diff_eq(centre, 1e-5), "{case}: {mean}");
                    let side = corners[1].distance(corners[2]);
                    assert!((side - 2.5).abs() < 1e-5, "{case}: {side}");
                    assert_turned_front_faces(&out, to_camera);
                }
            }
        }
    }

    /// Checks texture coordinates along a branch when some mesh rings are skipped.
    #[test]
    fn uv_modes_set_v_per_ring() {
        let branch = straight_branch(8, 4);
        let ring_v = |mode| {
            let mut out = MeshBuffers::default();
            mesh_branch(
                [&mut out, &mut MeshBuffers::default()],
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
            [&mut out, &mut MeshBuffers::default()],
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
            [&mut out, &mut MeshBuffers::default()],
            &leaf,
            7,
            &fixed_leaves(0.5, Billboard::Double, false),
            [0.2, 1.5, 0.25],
        );
        assert_eq!(span.vertices, 0..8);
        assert!(span.cull_vertices.is_empty() && out.billboard.is_empty());
        // Both cards follow their branch attachment. Only the two tip corners
        // of each card receive flutter.
        assert_eq!(out.wind[0], [0.2, 1.5, 0.25, 1.0]);
        assert_eq!(out.wind[1], [0.2, 1.5, 0.25, 0.0]);
        let flutter: Vec<f32> = out.wind.iter().map(|w| w[3]).collect();
        assert_eq!(flutter, [1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);
        assert_eq!(span.indices, 0..12);
        // Halving the card keeps its original centre at y = 3; its top edge
        // therefore lies at y = 3.5.
        assert_eq!(out.positions[0], [0.5, 3.5, 3.0]);
        // The crossed card lies at a right angle around the +Y stem direction.
        let turned = Vec3::from_array(out.positions[4]);
        assert!(
            turned.abs_diff_eq(Vec3::new(1.0, 3.5, 3.5), 1e-6),
            "{turned}"
        );
        // Both fixed cards use the same face normal, as in ez-tree.
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
                    let style = fixed_leaves(scale, billboard, rounded);
                    let buffers = [&mut out, &mut MeshBuffers::default()];
                    mesh_leaf(buffers, &leaf, 0, &style, [0.0; 3]);
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
