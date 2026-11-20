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
        }
    }
}

impl std::error::Error for OptionsError {}

impl TreeOptions {
    /// Checks the supported branch depth and required section counts.
    ///
    /// Checks every level from the trunk through `branch.levels`, even when child
    /// counts would leave a level empty. Section counts for higher levels are ignored.
    /// Success does not guarantee usable geometry: dimensions, floating-point values,
    /// and other settings are not checked.
    ///
    /// # Errors
    ///
    /// Returns [`OptionsError::TooManyLevels`] if `branch.levels` exceeds [`MAX_LEVELS`].
    /// Otherwise, returns [`OptionsError::ZeroSections`] for the first zero entry in
    /// `branch.sections` from level 0 through `branch.levels`, inclusive.
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
        Ok(())
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
fn deserialize_js_int32<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
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

        pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec2, D::Error> {
            let super::Xy { x, y } = super::Xy::deserialize(deserializer)?;
            Ok(Vec2::new(x, y))
        }
    }

    pub(super) mod vec3 {
        use glam::Vec3;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub(crate) fn serialize<S: Serializer>(v: &Vec3, serializer: S) -> Result<S::Ok, S::Error> {
            super::Xyz { x: v.x, y: v.y, z: v.z }.serialize(serializer)
        }

        pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec3, D::Error> {
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
