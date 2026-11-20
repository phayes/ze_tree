//! Settings for tree shape, leaves, bark, and guide wires for branch growth.
//!
//! Use [`TreeOptions::default()`] as a starting point, or load a full or partial
//! ez-tree JSON object with [`TreeOptions::from_json`]. JSON support is always
//! available; no Cargo feature is needed.

use std::fmt;
use std::ops::{Index, IndexMut};

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// Maximum value of [`BranchOptions::levels`]: three levels above the trunk.
pub const MAX_LEVELS: u32 = 3;

/// Settings for a tree's shape, leaves, bark, and optional grid of guide wires.
///
/// Start with [`TreeOptions::default()`] and adjust individual settings. For
/// example, `options.branch.length[0] = 12.0` sets the trunk length to 12 units.
/// These settings do not generate or draw a tree yet.
///
/// Use [`from_json`](Self::from_json) to load ez-tree JSON that may omit settings:
/// missing fields, including nested fields, keep their defaults. Deserializing
/// directly with serde requires every field. Use [`to_json`](Self::to_json) to
/// write all settings as JSON.
///
/// Bark and leaf texture identifiers are included, but texture objects are not.
/// Applications must supply the textures themselves.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeOptions {
    /// Starting value for repeatable random choices. Change it to request a
    /// different tree shape while keeping the other settings.
    #[serde(deserialize_with = "deserialize_js_int32")]
    pub seed: i32,
    /// Branching style: broadleaf or conifer-like.
    #[serde(rename = "type")]
    pub tree_type: TreeType,
    /// Colour, shading, and texture settings for the trunk and branches.
    pub bark: BarkOptions,
    /// Settings for the trunk and successive levels of child branches.
    pub branch: BranchOptions,
    /// Settings for where leaves appear and how they are drawn.
    pub leaves: LeafOptions,
    /// Guide-wire grid that can encourage branches to grow along a plane.
    pub trellis: TrellisOptions,
    /// Fruit placement and appearance settings. No fruit grows by default.
    pub fruit: FruitOptions,
    /// Flower placement and appearance settings. No flowers grow by default.
    pub flowers: FlowerOptions,
}

/// Branching styles used by ez-tree to shape the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeType {
    /// Broadleaf style, with a continuation at the tip of non-final branches.
    #[default]
    Deciduous,
    /// Conifer-like style, with branches tapering to a point and no tip continuation.
    Evergreen,
}

/// Flat-surface arrangement used to draw each leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Billboard {
    /// One rectangular surface per leaf.
    Single,
    /// Two rectangular surfaces at right angles, making leaves visible from more directions.
    Double,
}

/// How bark images repeat along each branch. Texture coordinates `u` and `v`
/// place the image around and along the branch, respectively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarkUvMode {
    /// Matches ez-tree: the image reverses direction between successive rings
    /// of vertices (`v` alternates between 0 and 1). Lower detail stretches the
    /// image along the branch. Avoid this mode with the supplied tangents for
    /// normal maps: their lighting detail is reversed on every other section.
    #[default]
    Mirrored,
    /// Repeats the image in one direction, preserving texture coordinates at
    /// retained sections when detail changes. Before material scaling, `v`
    /// increases by one per original branch section. Choose this mode for bark
    /// normal maps. This option is available in ze_tree only.
    Continuous,
}

/// A setting with a separate value for the trunk and each of three branch levels.
///
/// Index 0 is the trunk; branches at each later level grow from the previous
/// level. For example, `options.branch.length[0]` is the trunk length, and
/// `options.branch.length[1]` is the length setting for branches on the trunk.
///
/// JSON uses an object such as `{"0":20,"1":20,"2":10,"3":1}`, rather than an
/// array. Direct serde deserialization requires all four keys. When loading
/// options with [`TreeOptions::from_json`], omitted levels keep their defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PerLevel<T>(pub [T; 4]);

impl<T> Index<usize> for PerLevel<T> {
    type Output = T;

    /// # Panics
    ///
    /// Panics if `level` is outside `0..=3`.
    fn index(&self, level: usize) -> &T {
        &self.0[level]
    }
}

impl<T> IndexMut<usize> for PerLevel<T> {
    /// # Panics
    ///
    /// Panics if `level` is outside `0..=3`.
    fn index_mut(&mut self, level: usize) -> &mut T {
        &mut self.0[level]
    }
}

/// Colour, shading, and texture settings for the trunk and branches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BarkOptions {
    /// Texture-set identifier, such as `"Bark001"`, for the application to resolve.
    /// Setting this field does not load any textures.
    #[serde(rename = "type")]
    pub kind: String,
    /// Bark tint as `0xRRGGBB`; `0xffffff` leaves texture colours unchanged.
    pub tint: u32,
    /// Use faceted shading instead of a smooth-looking surface.
    pub flat_shading: bool,
    /// Whether bark should use textures supplied by the application.
    pub textured: bool,
    /// Texture scale around the branch (`x`) and along it (`y`). In ez-tree,
    /// larger `x` adds repeats around the branch; larger `y` stretches along it.
    #[serde(with = "serde_vec::vec2")]
    pub texture_scale: Vec2,
    /// How bark images repeat along branches. Defaults to [`BarkUvMode::Mirrored`];
    /// choose [`BarkUvMode::Continuous`] for normal maps. The JavaScript ez-tree
    /// generator ignores this setting and always uses mirrored coordinates.
    pub uv_mode: BarkUvMode,
}

/// Settings for the trunk and successive levels of child branches.
///
/// Level 0 is the trunk. Each successive level grows from the preceding one,
/// up to [`levels`](Self::levels). Entries for unused levels have no effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOptions {
    /// Number of branching steps above the trunk: 0 means trunk only.
    /// Must not exceed [`MAX_LEVELS`]; checked by [`TreeOptions::validate`].
    pub levels: u32,
    /// Child-branch angle relative to its parent, in degrees, for levels 1–3.
    pub angle: PerLevel<f32>,
    /// Number of side branches per parent at levels 0–2, excluding any tip continuation.
    pub children: PerLevel<u32>,
    /// Direction and strength of a steady influence on branch growth.
    pub force: GrowthForce,
    /// Amount of random bending; larger values give more irregular branches.
    pub gnarliness: PerLevel<f32>,
    /// Length setting for each level, in the same units as the tree dimensions.
    pub length: PerLevel<f32>,
    /// Trunk base radius at level 0. At levels 1–3, a multiplier of the parent
    /// radius where a side branch starts; 0.7 makes it 70% as thick.
    /// Tip continuations inherit the parent tip radius instead.
    pub radius: PerLevel<f32>,
    /// Number of lengthwise intervals per branch; 12 gives 13 cross-sections.
    /// More intervals allow finer bends. Deciduous tip continuations inherit the
    /// parent's count rather than this level's setting.
    /// [`TreeOptions::validate`] requires at least 1 for levels 0 through `levels`,
    /// even if no branches will grow at a configured level.
    pub sections: PerLevel<u32>,
    /// Number of sides around a branch cross-section; more sides look rounder.
    pub segments: PerLevel<u32>,
    /// Earliest child-branch position along the parent, from 0 (base) to 1 (tip).
    /// Indexed by the child level, 1–3.
    pub start: PerLevel<f32>,
    /// Radius reduction along deciduous branches: 0 keeps the radius constant,
    /// 1 tapers to a point. Evergreen branches always taper to a point.
    pub taper: PerLevel<f32>,
    /// Rotation around the branch axis for each lengthwise division, in radians.
    pub twist: PerLevel<f32>,
}

/// A directional influence on growth, such as an upward pull toward light.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GrowthForce {
    /// Target direction in tree coordinates; `(0, 1, 0)` points upward.
    /// Its length need not be 1.
    #[serde(with = "serde_vec::vec3")]
    pub direction: Vec3,
    /// Bending strength. Positive values turn toward the direction, negative
    /// values turn away, and zero disables the influence. Thinner branches bend more.
    pub strength: f32,
}

/// Settings for where leaves appear and how they are drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeafOptions {
    /// Texture identifier, such as `"oak"`, for the application to resolve.
    /// Setting this field does not load a texture.
    #[serde(rename = "type")]
    pub kind: String,
    /// Flat-surface arrangement used to draw each leaf.
    pub billboard: Billboard,
    /// Leaf angle relative to its supporting branch, in degrees.
    pub angle: f32,
    /// Number of leaves placed along each outermost branch, excluding tip leaves.
    pub count: u32,
    /// Earliest leaf position along a branch, from 0 (base) to 1 (tip).
    pub start: f32,
    /// Base leaf size, in the same units as branch lengths.
    pub size: f32,
    /// Random size variation as a fraction of [`size`](Self::size). For example,
    /// 0.2 varies sizes by up to 20% above or below the base size.
    pub size_variance: f32,
    /// Leaf tint as `0xRRGGBB`; `0xffffff` leaves texture colours unchanged.
    pub tint: u32,
    /// Opacity cutoff from 0 to 1. Texture pixels below this value are hidden,
    /// allowing a rectangular surface to show a leaf-shaped silhouette.
    pub alpha_test: f32,
    /// Shade leaves to suggest a rounded canopy rather than separate flat surfaces.
    pub rounded_normals: bool,
}

/// A flat grid of guide wires that can bend nearby branches toward it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrellisOptions {
    /// Enable the trellis influence on branch growth.
    pub enabled: bool,
    /// Position of the bottom centre of the grid, which lies in the XY plane.
    #[serde(with = "serde_vec::vec3")]
    pub position: Vec3,
    /// Grid width along the X axis, centred on [`position`](Self::position).
    pub width: f32,
    /// Grid height upward along the Y axis from [`position`](Self::position).
    pub height: f32,
    /// Gap between neighbouring guide wires, in tree units.
    pub spacing: f32,
    /// Strength, reach, and distance dependence of the trellis influence.
    pub force: TrellisForce,
    /// Radius to use when drawing the guide wires as cylinders.
    pub cylinder_radius: f32,
    /// Whether the application should draw the grid; independent of its growth influence.
    pub visible: bool,
    /// Guide-wire colour as `0xRRGGBB`.
    pub color: u32,
}

/// Settings for the pull toward the nearest trellis wire.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrellisForce {
    /// Base strength of the pull; zero disables it.
    pub strength: f32,
    /// Maximum distance from a guide wire at which the pull applies, in tree units.
    pub max_distance: f32,
    /// Exponent controlling how the pull weakens with distance: 1 is linear;
    /// 2 weakens it more quickly away from the wire.
    pub falloff: f32,
}

/// Controls where fruit grow and how they appear in [`Skeleton::mesh`](crate::Skeleton::mesh).
///
/// Set [`ClusterPlacement::count`] above zero for clusters along branches,
/// or [`ClusterPlacement::tips`] above zero for clusters at eligible tips.
/// Both default to zero, so fruit are disabled until enabled. Each cluster
/// can hold one or several items. Shape, maturity, and cluster stalk colour
/// control the resulting surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FruitOptions {
    /// Where fruit grow, how many clusters appear, and their size and direction.
    pub placement: ClusterPlacement,
    /// Arrangement of fruit within each cluster, including their supporting stalks.
    pub cluster: ClusterShape,
    /// Fruit shape and colours used to build [`TreeMesh::fruit`](crate::TreeMesh::fruit).
    pub shape: FruitShape,
    /// Ripeness from 0 (unripe) to 1 (ripe). When meshed, fruit grow from
    /// 30% to 100% of their size, blend from `unripe_color` to `color`, and
    /// develop a stronger blush (a contrasting patch on the skin).
    /// Does not move fruit or change their supporting cluster stalks.
    pub maturity: f32,
}

/// Controls where flowers grow and how they appear in [`Skeleton::mesh`](crate::Skeleton::mesh).
///
/// Set [`ClusterPlacement::count`] above zero for clusters along branches,
/// or [`ClusterPlacement::tips`] above zero for clusters at eligible tips.
/// Both default to zero, so flowers are disabled until enabled. Each cluster
/// can hold one or several items. Shape, maturity, and cluster stalk colour
/// control the resulting surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowerOptions {
    /// Where flowers grow, how many clusters appear, and their size and direction.
    pub placement: ClusterPlacement,
    /// Arrangement of flowers within each cluster, including their supporting stalks.
    pub cluster: ClusterShape,
    /// Flower shape and colours used to build [`TreeMesh::flowers`](crate::TreeMesh::flowers).
    pub shape: FlowerShape,
    /// Bloom stage from 0 (closed bud with `bud_color` petals), through 0.5
    /// (fully open), to 1 (faded, drooping petals with `fade_color`).
    /// Must be within 0–1. Catkin flowers change size and colour instead
    /// of opening petals. The flower centre keeps `center_color`.
    pub maturity: f32,
}

/// Controls where fruit or flower clusters appear, their size, and their direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterPlacement {
    /// First branch level allowed to carry clusters: 0 includes the trunk,
    /// 1 starts with branches growing from it, and so on. Deeper levels are
    /// also included. A value beyond the tree's branch levels produces no clusters.
    pub min_level: u32,
    /// Chance of an extra cluster at each eligible branch tip, from 0 (never)
    /// to 1 (always). The branch must meet [`min_level`](Self::min_level) and
    /// have no continuation at its tip: only the deepest level on deciduous
    /// trees, or any level on evergreen trees.
    pub tips: f32,
    /// Clusters along each eligible branch, in addition to any tip cluster.
    /// Set both this and [`tips`](Self::tips) to zero to disable clusters.
    pub count: u32,
    /// Start of the region used for side clusters, from 0 (branch base) to
    /// 1 (branch tip). Does not affect the chance of a tip cluster.
    pub start: f32,
    /// Main direction of each cluster before random tilt is applied.
    pub facing: Facing,
    /// Maximum random tilt from [`facing`](Self::facing), in degrees.
    /// Must be finite and non-negative; zero keeps the chosen direction.
    pub facing_jitter: f32,
    /// Base size of each fruit or flower in tree units, before random variation
    /// and cluster taper. Must be finite and non-negative.
    pub size: f32,
    /// Random size variation from 0 (all the same base size) to 1.
    /// For example, 0.2 allows sizes from 80% to 120% of [`size`](Self::size),
    /// before cluster taper is applied.
    pub size_variance: f32,
}

/// Main direction of a fruit or flower cluster, before random tilt.
/// Up and down refer to the tree's Y axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    /// Downward, as for hanging fruit.
    Hang,
    /// Upward, as for an upright flower cluster.
    Up,
    /// Away from the side of the branch at a right angle to it.
    /// At a tip, follows the direction of branch growth.
    Outward,
}

/// Arrangement and supporting stalks of one fruit or flower cluster.
///
/// An *item* is one fruit or flower. The *axis* is the main stem supporting
/// a cluster. Lengths and radii use the same units as the tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterShape {
    /// Pattern used to arrange fruit or flowers in the cluster.
    pub kind: ClusterKind,
    /// Base number of fruit or flowers per cluster; must be at least 1.
    /// [`ClusterKind::Single`] always produces exactly one.
    pub items: u32,
    /// Random variation in the item count, from 0 to 1. For example, 0.2
    /// varies the count by up to 20%, rounded to a whole number with a minimum
    /// of one. Ignored by [`ClusterKind::Single`].
    pub items_variance: f32,
    /// Angle in degrees at which stalks fan out from the cluster direction
    /// for an umbel, or from their supporting stem for a raceme or panicle.
    /// For a catkin, sets the angle of the items themselves. Ignored for a
    /// single item. Must be finite and non-negative.
    pub spread: f32,
    /// Length of the main supporting stem for racemes, panicles, and catkins,
    /// in tree units. Excludes a catkin's bare stalk. Must be finite and non-negative.
    pub axis_length: f32,
    /// Downward bending of the main stem and panicle side stems, from 0
    /// (no added downward bend) to 1 (strongest bend). The stem still curves
    /// away from the branch toward the chosen facing direction. Has no effect
    /// on single items or umbels.
    pub droop: f32,
    /// Reduction in item size toward the main stem's tip, from 0 (no reduction)
    /// to 1 (approaching zero at the tip). Also shortens item stalks and panicle
    /// side stems. Has no effect on single items or umbels.
    pub taper: f32,
    /// Number of side stems for a panicle; must be at least 1. Items are
    /// shared among them. Fewer stems are used if there are fewer items.
    pub side_stalks: u32,
    /// Base length of each panicle side stem as a fraction of
    /// [`axis_length`](Self::axis_length), before taper shortens it.
    /// Must be finite and non-negative; values above 1 are allowed.
    pub side_length: f32,
    /// Length of each item's stalk in tree units, before taper. A single item
    /// or umbel grows directly from the branch; raceme items grow from the
    /// main stem, and panicle items from side stems. For catkins, this is the
    /// bare stem before the items begin. Must be finite and non-negative;
    /// zero attaches items directly to their supporting branch or stem.
    pub stalk_length: f32,
    /// Thickness of all cluster stalks and stems, measured as a radius in
    /// tree units. Must be finite and non-negative.
    pub stalk_radius: f32,
    /// Colour of the supporting stalks and stems, written as `0xRRGGBB`.
    pub stalk_color: u32,
}

/// Ways to arrange fruit or flowers and their supporting stalks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClusterKind {
    /// One fruit or flower, with an optional stalk, such as an apple.
    Single,
    /// An umbrella-like group: stalks fan out from one point, each carrying
    /// one fruit or flower, as in a small group of cherries.
    Umbel,
    /// One main stem with individually stalked fruit or flowers along it,
    /// as in a spray of laburnum flowers.
    Raceme,
    /// A branching cluster: side stems grow from a main stem and each carry
    /// fruit or flowers, as in a bunch of grapes.
    Panicle,
    /// Fruit or flowers attached directly along a main stem, without
    /// individual stalks, forming a narrow cluster like a willow catkin.
    Catkin,
}

/// Shape and colours of fruit built by [`Skeleton::mesh`](crate::Skeleton::mesh).
///
/// Lengths and radii are relative to each fruit’s size after placement
/// variation, cluster taper, and maturity are applied. Colours are sRGB
/// values written as `0xRRGGBB`. Fruit in catkin clusters have no individual
/// stems, so their stem settings have no effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FruitShape {
    /// Height relative to width: 1 is round, below 1 is flattened, and above 1
    /// is elongated. Must be finite and greater than zero.
    pub elongation: f32,
    /// Depth of the hollow around the stem, like the dip in an apple,
    /// from 0 (none) to 1 (deepest).
    pub top_dimple: f32,
    /// Narrowing at the end opposite the stem, from 0 (none) to 1 (strongest).
    pub bottom_taper: f32,
    /// Number of raised ribs around the fruit. Zero describes a smooth fruit.
    pub lobes: u32,
    /// Strength of the ribbed shape, from 0 (smooth) to less than 1.
    pub lobe_depth: f32,
    /// Number of points around the fruit; at least 3. Higher values allow
    /// a smoother outline when building a mesh.
    pub segments: u32,
    /// Number of rings of points from stem to bottom; at least 2.
    /// Higher values allow more detail along the fruit's height.
    pub rings: u32,
    /// Length of the fruit's own short stem, relative to fruit size.
    /// Must be finite and non-negative.
    pub stem_length: f32,
    /// Radius of the fruit's own stem, relative to fruit size.
    /// Must be finite and non-negative.
    pub stem_radius: f32,
    /// Bend in the fruit's stem, from 0 (straight) to 1 (strongest).
    pub stem_curve: f32,
    /// Whether to show a calyx: the small remains of the flower at the
    /// end opposite the fruit's stem.
    pub calyx: bool,
    /// Ripe fruit colour, written as `0xRRGGBB`.
    pub color: u32,
    /// Unripe fruit colour, written as `0xRRGGBB`.
    pub unripe_color: u32,
    /// Colour of the blush, a contrasting patch on the fruit's skin.
    pub blush_color: u32,
    /// Strength of the blush colour, from 0 (absent) to 1 (strongest).
    pub blush_amount: f32,
}

/// Shape and colours of flowers built by [`Skeleton::mesh`](crate::Skeleton::mesh).
///
/// Lengths are relative to each flower’s size after placement variation and
/// cluster taper. Maturity controls opening and fading. Colours are sRGB
/// values written as `0xRRGGBB`. Catkin flowers are rounded bumps and use
/// only `petal_color`, `bud_color`, and `fade_color` from these settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowerShape {
    /// Petals in each ring around the flower's centre; must be at least 1.
    pub petal_count: u32,
    /// Number of layers of petals around the centre; must be at least 1.
    pub rings: u32,
    /// Length of a petal relative to flower size; must be finite and non-negative.
    pub petal_length: f32,
    /// Maximum width of a petal relative to flower size; must be finite
    /// and non-negative.
    pub petal_width: f32,
    /// How strongly petals curve into a cup, from 0 (flat) to 1 (most closed).
    pub cup: f32,
    /// Backward bend at the petal tips, from 0 (none) to 1 (strongest).
    pub curl: f32,
    /// Depth of the indentation in each petal tip, from 0 (none) to 1 (deepest).
    pub notch: f32,
    /// Lengthwise divisions of each petal; at least 1. More divisions allow
    /// smoother bends when building a mesh.
    pub petal_segments: u32,
    /// Width of the flower's centre relative to flower size; must be finite
    /// and non-negative.
    pub center_size: f32,
    /// Colour of the central part surrounded by the petals.
    pub center_color: u32,
    /// Main colour of the petals.
    pub petal_color: u32,
    /// Colour at the base of each petal, near the flower's centre.
    pub base_color: u32,
    /// Colour of an unopened flower bud.
    pub bud_color: u32,
    /// Colour of petals as the flower fades.
    pub fade_color: u32,
}

impl Default for BarkOptions {
    fn default() -> Self {
        Self {
            kind: "Bark001".to_owned(),
            tint: 0xffffff,
            flat_shading: false,
            textured: true,
            texture_scale: Vec2::ONE,
            uv_mode: BarkUvMode::Mirrored,
        }
    }
}

impl Default for BranchOptions {
    fn default() -> Self {
        // Keep unused entries at zero: the trunk has no parent angle or start
        // position, and the final level has no child branches.
        Self {
            levels: 3,
            angle: PerLevel([0.0, 70.0, 60.0, 60.0]),
            children: PerLevel([7, 7, 5, 0]),
            force: GrowthForce::default(),
            gnarliness: PerLevel([0.15, 0.2, 0.3, 0.02]),
            length: PerLevel([20.0, 20.0, 10.0, 1.0]),
            radius: PerLevel([1.5, 0.7, 0.7, 0.7]),
            sections: PerLevel([12, 10, 8, 6]),
            segments: PerLevel([8, 6, 4, 3]),
            start: PerLevel([0.0, 0.4, 0.3, 0.3]),
            taper: PerLevel([0.7; 4]),
            twist: PerLevel([0.0; 4]),
        }
    }
}

impl Default for GrowthForce {
    fn default() -> Self {
        Self {
            direction: Vec3::Y,
            strength: 0.01,
        }
    }
}

impl Default for LeafOptions {
    fn default() -> Self {
        Self {
            kind: "oak".to_owned(),
            billboard: Billboard::Double,
            angle: 10.0,
            count: 1,
            start: 0.0,
            size: 2.5,
            size_variance: 0.7,
            tint: 0xffffff,
            alpha_test: 0.5,
            rounded_normals: true,
        }
    }
}

impl Default for TrellisOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            position: Vec3::new(0.0, 0.0, -2.0),
            width: 10.0,
            height: 20.0,
            spacing: 2.0,
            force: TrellisForce::default(),
            cylinder_radius: 0.05,
            visible: true,
            color: 0x8b4513,
        }
    }
}

impl Default for TrellisForce {
    fn default() -> Self {
        Self {
            strength: 0.02,
            max_distance: 3.0,
            falloff: 1.0,
        }
    }
}

impl Default for FruitOptions {
    fn default() -> Self {
        Self {
            placement: ClusterPlacement {
                min_level: 1,
                tips: 0.0,
                count: 0,
                start: 0.3,
                facing: Facing::Hang,
                facing_jitter: 15.0,
                size: 1.2,
                size_variance: 0.15,
            },
            cluster: ClusterShape {
                kind: ClusterKind::Single,
                items: 1,
                items_variance: 0.0,
                spread: 30.0,
                axis_length: 2.0,
                droop: 0.0,
                taper: 0.0,
                side_stalks: 4,
                side_length: 0.4,
                stalk_length: 0.4,
                stalk_radius: 0.04,
                stalk_color: 0x5a6b2e,
            },
            shape: FruitShape {
                elongation: 1.0,
                top_dimple: 0.0,
                bottom_taper: 0.0,
                lobes: 0,
                lobe_depth: 0.0,
                segments: 12,
                rings: 8,
                stem_length: 0.2,
                stem_radius: 0.04,
                stem_curve: 0.3,
                calyx: true,
                color: 0xb8262a,
                unripe_color: 0x7fa83a,
                blush_color: 0xe0552a,
                blush_amount: 0.0,
            },
            maturity: 1.0,
        }
    }
}

impl Default for FlowerOptions {
    fn default() -> Self {
        Self {
            placement: ClusterPlacement {
                min_level: 1,
                tips: 0.0,
                count: 0,
                start: 0.3,
                facing: Facing::Outward,
                facing_jitter: 20.0,
                size: 1.0,
                size_variance: 0.15,
            },
            cluster: ClusterShape {
                kind: ClusterKind::Single,
                items: 1,
                items_variance: 0.0,
                spread: 40.0,
                axis_length: 2.0,
                droop: 0.0,
                taper: 0.0,
                side_stalks: 4,
                side_length: 0.4,
                stalk_length: 0.3,
                stalk_radius: 0.02,
                stalk_color: 0x5a6b2e,
            },
            shape: FlowerShape {
                petal_count: 5,
                rings: 1,
                petal_length: 0.5,
                petal_width: 0.35,
                cup: 0.2,
                curl: 0.0,
                notch: 0.0,
                petal_segments: 4,
                center_size: 0.15,
                center_color: 0xe8c840,
                petal_color: 0xffffff,
                base_color: 0xffd8e4,
                bud_color: 0xe08098,
                fade_color: 0xcbbca4,
            },
            maturity: 0.5,
        }
    }
}

/// A problem detected by [`TreeOptions::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptionsError {
    /// The requested branch level exceeds the supported maximum.
    TooManyLevels {
        /// Number of levels above the trunk requested by the caller.
        levels: u32,
    },
    /// A level from 0 through `branch.levels` requests zero lengthwise intervals.
    ZeroSections {
        /// Index of the zero entry in `branch.sections`; 0 identifies the trunk.
        level: u32,
    },
    /// A fruit or flower setting failed a range or finiteness check.
    InvalidSetting {
        /// Setting name using JSON field names, for example `"fruit.maturity"`.
        field: String,
        /// Description of the accepted value, for example `"between 0 and 1"`.
        requirement: &'static str,
    },
}

impl fmt::Display for OptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyLevels { levels } => write!(
                f,
                "branch.levels is {levels}, but at most {MAX_LEVELS} levels are supported"
            ),
            Self::ZeroSections { level } => write!(
                f,
                "branch.sections[{level}] is 0, but every level in use needs at least 1 section"
            ),
            Self::InvalidSetting { field, requirement } => {
                write!(f, "{field} must be {requirement}")
            }
        }
    }
}

impl std::error::Error for OptionsError {}

impl TreeOptions {
    /// Checks branch depth, branch section counts, and fruit and flower ranges.
    ///
    /// All branch levels from the trunk through `branch.levels` need nonzero
    /// section counts, even if no branches grow at a level. Higher levels are
    /// ignored. Fruit and flower settings are checked even when disabled or
    /// unused by the chosen cluster kind. Other settings, including branch and
    /// leaf dimensions, are not checked, so success does not guarantee valid geometry.
    ///
    /// # Errors
    ///
    /// Returns [`OptionsError::TooManyLevels`] if branch depth exceeds [`MAX_LEVELS`],
    /// then [`OptionsError::ZeroSections`] for the first configured level with
    /// zero sections. If those checks pass, returns [`OptionsError::InvalidSetting`]
    /// for the first invalid fruit setting, then flower setting. NaN and infinite
    /// values fail the checked floating-point ranges.
    pub fn validate(&self) -> Result<(), OptionsError> {
        if self.branch.levels > MAX_LEVELS {
            return Err(OptionsError::TooManyLevels {
                levels: self.branch.levels,
            });
        }
        for level in 0..=self.branch.levels {
            if self.branch.sections[level as usize] == 0 {
                return Err(OptionsError::ZeroSections { level });
            }
        }
        self.fruit.validate()?;
        self.flowers.validate()
    }
}

impl FruitOptions {
    /// Checks fruit maturity, placement, cluster arrangement, and shape ranges,
    /// including settings that are currently disabled or unused.
    fn validate(&self) -> Result<(), OptionsError> {
        let v = Checker("fruit");
        v.unit(self.maturity, "maturity")?;
        v.cluster(&self.placement, &self.cluster)?;

        // Check fruit proportions and the detail needed for a fruit surface.
        let s = &self.shape;
        v.check(
            s.elongation > 0.0 && s.elongation.is_finite(),
            "shape.elongation",
            "a finite value above 0",
        )?;
        v.unit(s.top_dimple, "shape.topDimple")?;
        v.unit(s.bottom_taper, "shape.bottomTaper")?;
        v.check(
            (0.0..1.0).contains(&s.lobe_depth),
            "shape.lobeDepth",
            "at least 0 and below 1",
        )?;
        v.check(s.segments >= 3, "shape.segments", "at least 3")?;
        v.check(s.rings >= 2, "shape.rings", "at least 2")?;
        v.non_negative(s.stem_length, "shape.stemLength")?;
        v.non_negative(s.stem_radius, "shape.stemRadius")?;
        v.unit(s.stem_curve, "shape.stemCurve")?;
        v.unit(s.blush_amount, "shape.blushAmount")
    }
}

impl FlowerOptions {
    /// Checks flower maturity, placement, cluster arrangement, and shape ranges,
    /// including settings that are currently disabled or unused.
    fn validate(&self) -> Result<(), OptionsError> {
        let v = Checker("flowers");
        v.unit(self.maturity, "maturity")?;
        v.cluster(&self.placement, &self.cluster)?;

        // Check petal counts, dimensions, and shape controls.
        let s = &self.shape;
        v.check(s.petal_count >= 1, "shape.petalCount", "at least 1")?;
        v.check(s.rings >= 1, "shape.rings", "at least 1")?;
        v.non_negative(s.petal_length, "shape.petalLength")?;
        v.non_negative(s.petal_width, "shape.petalWidth")?;
        v.unit(s.cup, "shape.cup")?;
        v.unit(s.curl, "shape.curl")?;
        v.unit(s.notch, "shape.notch")?;
        v.check(s.petal_segments >= 1, "shape.petalSegments", "at least 1")?;
        v.non_negative(s.center_size, "shape.centerSize")
    }
}

/// Checks values and prefixes error field names with their settings group,
/// such as `fruit` or `flowers`.
struct Checker(&'static str);

impl Checker {
    /// Succeeds when `ok` is true; otherwise reports the full setting path and
    /// the requirement that was not met.
    fn check(&self, ok: bool, field: &str, requirement: &'static str) -> Result<(), OptionsError> {
        if ok {
            Ok(())
        } else {
            Err(OptionsError::InvalidSetting {
                field: format!("{}.{field}", self.0),
                requirement,
            })
        }
    }

    /// Accepts values from 0 through 1, including both endpoints.
    /// Rejects NaN and infinities.
    fn unit(&self, value: f32, field: &str) -> Result<(), OptionsError> {
        self.check((0.0..=1.0).contains(&value), field, "between 0 and 1")
    }

    /// Accepts zero and positive finite values; rejects negatives, NaN, and infinities.
    fn non_negative(&self, value: f32, field: &str) -> Result<(), OptionsError> {
        self.check(
            (0.0..f32::INFINITY).contains(&value),
            field,
            "a finite value of 0 or more",
        )
    }

    /// Checks the shared placement and cluster ranges, including fields unused
    /// by the selected cluster kind.
    fn cluster(
        &self,
        placement: &ClusterPlacement,
        cluster: &ClusterShape,
    ) -> Result<(), OptionsError> {
        // Check where clusters can grow, their size, and their random tilt.
        self.unit(placement.tips, "placement.tips")?;
        self.unit(placement.start, "placement.start")?;
        self.non_negative(placement.facing_jitter, "placement.facingJitter")?;
        self.non_negative(placement.size, "placement.size")?;
        self.unit(placement.size_variance, "placement.sizeVariance")?;

        // Check item counts and the dimensions and bends of supporting stalks.
        self.check(cluster.items >= 1, "cluster.items", "at least 1")?;
        self.unit(cluster.items_variance, "cluster.itemsVariance")?;
        self.non_negative(cluster.spread, "cluster.spread")?;
        self.non_negative(cluster.axis_length, "cluster.axisLength")?;
        self.unit(cluster.droop, "cluster.droop")?;
        self.unit(cluster.taper, "cluster.taper")?;
        self.check(cluster.side_stalks >= 1, "cluster.sideStalks", "at least 1")?;
        self.non_negative(cluster.side_length, "cluster.sideLength")?;
        self.non_negative(cluster.stalk_length, "cluster.stalkLength")?;
        self.non_negative(cluster.stalk_radius, "cluster.stalkRadius")
    }
}

// ---------------------------------------------------------------------------
// JSON support

impl TreeOptions {
    /// Reads a full or partial ez-tree options object from JSON.
    ///
    /// Missing fields keep their defaults, including nested fields. For example,
    /// `{"branch":{"length":{"0":12}}}` changes only the trunk length.
    /// Unknown fields are ignored. Numeric seeds are read as `f64`, truncated
    /// toward zero, and wrapped into the `i32` range, as in JavaScript.
    ///
    /// A non-object root returns the defaults. An object supplied for a scalar
    /// field is ignored, so `{"seed":{}}` also keeps the default seed.
    ///
    /// The result is not validated; call [`validate`](Self::validate) to check it.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON, numbers outside the supported JSON
    /// range, or incompatible values remaining after defaults are merged, such
    /// as `{"seed":"12"}` or `{"type":"palm"}`.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        // Merge supplied settings into defaults so omitted nested fields also
        // have values before deserializing the complete options object.
        let source: serde_json::Value = serde_json::from_str(json)?;
        let mut merged = serde_json::to_value(Self::default())?;
        merge_json(&mut merged, source);
        serde_json::from_value(merged)
    }

    /// Writes all settings as indented JSON using ez-tree field names.
    ///
    /// Does not validate the settings. Non-finite floats become `null`, which
    /// cannot be read back as numeric settings. Texture objects are not included.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("tree options always serialize to JSON")
    }
}

/// Overlays known source keys onto the default object, merging nested objects.
/// Unknown keys and object-to-scalar replacements are ignored. Non-object values
/// replace their slots directly and are type-checked during deserialization.
fn merge_json(target: &mut serde_json::Value, source: serde_json::Value) {
    use serde_json::Value;

    let (Value::Object(target), Value::Object(source)) = (target, source) else {
        return;
    };
    for (key, value) in source {
        let Some(slot) = target.get_mut(&key) else {
            continue;
        };
        if value.is_object() {
            merge_json(slot, value);
        } else {
            *slot = value;
        }
    }
}

/// Reads a numeric seed as `f64`, then applies JavaScript's signed 32-bit conversion.
fn deserialize_js_int32<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<i32, D::Error> {
    f64::deserialize(deserializer).map(js_to_int32)
}

/// Truncates toward zero and wraps modulo 2^32 into the signed `i32` range.
/// NaN and infinities become zero, matching JavaScript's `ToInt32` operation.
fn js_to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    x.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
}

mod per_level_serde {
    use std::fmt;
    use std::marker::PhantomData;

    use serde::de::{Error, IgnoredAny, MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::PerLevel;

    const KEYS: [&str; 4] = ["0", "1", "2", "3"];

    impl<T: Serialize> Serialize for PerLevel<T> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(KEYS.len()))?;
            for (key, value) in KEYS.iter().zip(&self.0) {
                map.serialize_entry(key, value)?;
            }
            map.end()
        }
    }

    /// Reads per-level values from an object with keys `"0"`, `"1"`, `"2"`, and `"3"`.
    ///
    /// All four keys are required; extra keys are ignored. To load tree options
    /// with only some levels specified, use [`super::TreeOptions::from_json`],
    /// which fills omitted levels from the defaults before deserializing.
    impl<'de, T: Deserialize<'de>> Deserialize<'de> for PerLevel<T> {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_map(PerLevelVisitor(PhantomData))
        }
    }

    struct PerLevelVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for PerLevelVisitor<T> {
        type Value = PerLevel<T>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an object with keys \"0\", \"1\", \"2\" and \"3\"")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            // Read recognised level keys and discard values for all other keys.
            let mut slots: [Option<T>; 4] = [None, None, None, None];
            while let Some(key) = map.next_key::<String>()? {
                match KEYS.iter().position(|k| *k == key) {
                    Some(level) => slots[level] = Some(map.next_value()?),
                    None => {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
            }

            // Direct PerLevel deserialization has no defaults to fill missing slots.
            match slots {
                [Some(a), Some(b), Some(c), Some(d)] => Ok(PerLevel([a, b, c, d])),
                _ => Err(A::Error::custom(
                    "per-level value is missing one of the keys \"0\" to \"3\"",
                )),
            }
        }
    }
}

/// Reads and writes vectors as ez-tree objects: `{"x":1,"y":2}` for 2D
/// and `{"x":1,"y":2,"z":3}` for 3D, rather than arrays.
mod serde_vec {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Xy {
        x: f32,
        y: f32,
    }

    #[derive(Serialize, Deserialize)]
    struct Xyz {
        x: f32,
        y: f32,
        z: f32,
    }

    pub(super) mod vec2 {
        use glam::Vec2;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub(crate) fn serialize<S: Serializer>(v: &Vec2, serializer: S) -> Result<S::Ok, S::Error> {
            super::Xy { x: v.x, y: v.y }.serialize(serializer)
        }

        pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Vec2, D::Error> {
            let super::Xy { x, y } = super::Xy::deserialize(deserializer)?;
            Ok(Vec2::new(x, y))
        }
    }

    pub(super) mod vec3 {
        use glam::Vec3;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub(crate) fn serialize<S: Serializer>(v: &Vec3, serializer: S) -> Result<S::Ok, S::Error> {
            super::Xyz {
                x: v.x,
                y: v.y,
                z: v.z,
            }
            .serialize(serializer)
        }

        pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Vec3, D::Error> {
            let super::Xyz { x, y, z } = super::Xyz::deserialize(deserializer)?;
            Ok(Vec3::new(x, y, z))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_to_int32_matches_javascript() {
        assert_eq!(js_to_int32(0.0), 0);
        assert_eq!(js_to_int32(-0.0), 0);
        assert_eq!(js_to_int32(2.7), 2);
        assert_eq!(js_to_int32(-2.7), -2);
        assert_eq!(js_to_int32(2_147_483_647.0), i32::MAX);
        assert_eq!(js_to_int32(2_147_483_648.0), i32::MIN);
        assert_eq!(js_to_int32(-2_147_483_649.0), i32::MAX);
        assert_eq!(js_to_int32(3_000_000_000.0), -1_294_967_296);
        assert_eq!(js_to_int32(4_294_967_301.0), 5);
        assert_eq!(js_to_int32(1e10), 1_410_065_408);
        assert_eq!(js_to_int32(f64::NAN), 0);
        assert_eq!(js_to_int32(f64::INFINITY), 0);
    }

    #[test]
    fn merge_follows_ez_tree_copy() {
        use serde_json::json;

        let mut target = json!({"a": 1, "b": {"x": 1, "y": 2}, "c": 3});
        merge_json(
            &mut target,
            json!({"a": 5, "b": {"y": 9, "unknown": 1}, "c": {"x": 1}, "d": 4}),
        );
        assert_eq!(target, json!({"a": 5, "b": {"x": 1, "y": 9}, "c": 3}));
    }
}
