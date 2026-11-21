//! Tree identifiers for recognizing and reusing generated trees.
//!
//! [`TreeOptions::tree_id`] hashes every setting into a 64-bit value using XXH3.
//! Identical settings have the same identifier across processes and platforms
//! running the same `ze_tree` version. Use it as a cache key, keeping in mind
//! that different settings can occasionally share a hash.
//! Floating-point settings use their exact stored bits, so `0.0` and `-0.0`
//! produce different identifiers.

use std::hash::{Hash, Hasher};

use glam::{Vec2, Vec3};
use twox_hash::XxHash3_64;

use crate::{
    BarkOptions, BarkUvMode, Billboard, BranchOptions, ClusterKind, ClusterPlacement, ClusterShape,
    Facing, FlowerOptions, FlowerShape, FruitOptions, FruitShape, GrowthForce, LeafOptions,
    PerLevel, Tree, TreeOptions, TreeType, TrellisForce, TrellisOptions,
};

impl TreeOptions {
    /// Returns a 64-bit XXH3 hash of all settings used to generate this tree.
    ///
    /// Use the identifier to look up cached trees or shared meshes. Identical
    /// settings produce the same identifier across processes and platforms
    /// using the same `ze_tree` version. Identifiers may change between versions.
    /// Different settings can share an identifier because a 64-bit hash has a
    /// limited number of possible values; a matching identifier is not proof
    /// that the settings are identical.
    ///
    /// Every setting contributes, including colors and texture names. Settings
    /// that do not affect the generated geometry still contribute to the hash.
    /// Floating-point values use their exact stored bits: `0.0` and `-0.0`
    /// produce different identifiers even though they compare equal. A NaN
    /// (a "not a number" value) hashes consistently when its bits are unchanged.
    ///
    /// Cloning settings preserves the identifier. Changing the random seed in
    /// this example gives the tree a different identifier:
    ///
    /// ```
    /// use ze_tree::TreePreset;
    ///
    /// let a = TreePreset::OakSmall.options();
    /// let mut b = a.clone();
    /// assert_eq!(a.tree_id(), b.tree_id());
    ///
    /// b.seed += 1;
    /// assert_ne!(a.tree_id(), b.tree_id());
    /// ```
    pub fn tree_id(&self) -> u64 {
        let mut hasher = StableHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl Tree {
    /// Returns this tree's identifier, calculated from its generation settings.
    ///
    /// This is the same value as `tree.options().tree_id()`. See
    /// [`TreeOptions::tree_id`] for cache usage, version stability, and limits.
    pub fn tree_id(&self) -> u64 {
        self.options().tree_id()
    }
}

/// Hashes the tree's generation settings, without reading its skeleton or meshes.
/// Use [`Tree::tree_id`] when you need an identifier that is stable across processes
/// and platforms; this implementation uses whichever hasher you supply.
impl Hash for Tree {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.options().hash(state);
    }
}

/// Computes XXH3-64 hashes with a fixed seed and consistent integer encoding.
/// Integers are written least significant byte first (little-endian), and
/// `usize` and `isize` use 64 bits, keeping tree IDs consistent across platforms.
struct StableHasher(XxHash3_64);

impl StableHasher {
    fn new() -> Self {
        Self(XxHash3_64::with_seed(0))
    }
}

impl Hasher for StableHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes);
    }

    fn write_u8(&mut self, i: u8) {
        self.0.write(&[i]);
    }

    fn write_u16(&mut self, i: u16) {
        self.0.write(&i.to_le_bytes());
    }

    fn write_u32(&mut self, i: u32) {
        self.0.write(&i.to_le_bytes());
    }

    fn write_u64(&mut self, i: u64) {
        self.0.write(&i.to_le_bytes());
    }

    fn write_u128(&mut self, i: u128) {
        self.0.write(&i.to_le_bytes());
    }

    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }

    fn write_i8(&mut self, i: i8) {
        self.write_u8(i as u8);
    }

    fn write_i16(&mut self, i: i16) {
        self.write_u16(i as u16);
    }

    fn write_i32(&mut self, i: i32) {
        self.write_u32(i as u32);
    }

    fn write_i64(&mut self, i: i64) {
        self.write_u64(i as u64);
    }

    fn write_i128(&mut self, i: i128) {
        self.write_u128(i as u128);
    }

    fn write_isize(&mut self, i: isize) {
        self.write_u64(i as i64 as u64);
    }

    fn finish(&self) -> u64 {
        self.0.finish()
    }
}

// These helpers encode common setting types consistently for hashing.

fn write_f32<H: Hasher>(state: &mut H, value: f32) {
    state.write_u32(value.to_bits());
}

fn write_bool<H: Hasher>(state: &mut H, value: bool) {
    state.write_u8(u8::from(value));
}

fn write_str<H: Hasher>(state: &mut H, value: &str) {
    state.write_u64(value.len() as u64);
    state.write(value.as_bytes());
}

fn write_vec2<H: Hasher>(state: &mut H, value: Vec2) {
    write_f32(state, value.x);
    write_f32(state, value.y);
}

fn write_vec3<H: Hasher>(state: &mut H, value: Vec3) {
    write_f32(state, value.x);
    write_f32(state, value.y);
    write_f32(state, value.z);
}

fn write_levels_f32<H: Hasher>(state: &mut H, levels: &PerLevel<f32>) {
    for &value in &levels.0 {
        write_f32(state, value);
    }
}

fn write_levels_u32<H: Hasher>(state: &mut H, levels: &PerLevel<u32>) {
    for &value in &levels.0 {
        state.write_u32(value);
    }
}

// Each enum variant has an explicit byte value. Reordering variants in their
// declarations therefore leaves tree identifiers unchanged.

fn write_tree_type<H: Hasher>(state: &mut H, value: TreeType) {
    state.write_u8(match value {
        TreeType::Deciduous => 0,
        TreeType::Evergreen => 1,
    });
}

fn write_billboard<H: Hasher>(state: &mut H, value: Billboard) {
    state.write_u8(match value {
        Billboard::Single => 0,
        Billboard::Double => 1,
    });
}

fn write_uv_mode<H: Hasher>(state: &mut H, value: BarkUvMode) {
    state.write_u8(match value {
        BarkUvMode::Mirrored => 0,
        BarkUvMode::Continuous => 1,
    });
}

fn write_facing<H: Hasher>(state: &mut H, value: Facing) {
    state.write_u8(match value {
        Facing::Hang => 0,
        Facing::Up => 1,
        Facing::Outward => 2,
    });
}

fn write_cluster_kind<H: Hasher>(state: &mut H, value: ClusterKind) {
    state.write_u8(match value {
        ClusterKind::Single => 0,
        ClusterKind::Umbel => 1,
        ClusterKind::Raceme => 2,
        ClusterKind::Panicle => 3,
        ClusterKind::Catkin => 4,
    });
}

// Listing every field without `..` makes the compiler flag newly added settings.
// When adding a field, include it in both the field list and the hash writes.

/// Hashes all tree settings, including the seed and appearance settings.
/// Floats use their exact stored bits, so `0.0` and `-0.0` hash differently.
/// Use [`TreeOptions::tree_id`] for an identifier that is stable across processes
/// and platforms; this implementation uses whichever hasher you supply.
impl Hash for TreeOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            seed,
            tree_type,
            bark,
            branch,
            leaves,
            trellis,
            fruit,
            flowers,
        } = self;
        state.write_i32(*seed);
        write_tree_type(state, *tree_type);
        bark.hash(state);
        branch.hash(state);
        leaves.hash(state);
        trellis.hash(state);
        fruit.hash(state);
        flowers.hash(state);
    }
}

/// Hashes all bark settings, including the texture name, color, and texture layout.
/// Floating-point settings use their exact stored bits.
impl Hash for BarkOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            kind,
            tint,
            flat_shading,
            textured,
            texture_scale,
            uv_mode,
        } = self;
        write_str(state, kind);
        state.write_u32(*tint);
        write_bool(state, *flat_shading);
        write_bool(state, *textured);
        write_vec2(state, *texture_scale);
        write_uv_mode(state, *uv_mode);
    }
}

/// Hashes all branch settings, including values for every branching level.
/// Floating-point settings use their exact stored bits.
impl Hash for BranchOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            levels,
            stems,
            stem_angle,
            angle,
            children,
            force,
            flatten,
            gnarliness,
            length,
            radius,
            sections,
            segments,
            start,
            taper,
            twist,
        } = self;
        state.write_u32(*levels);
        state.write_u32(*stems);
        write_f32(state, *stem_angle);
        write_levels_f32(state, angle);
        write_levels_u32(state, children);
        force.hash(state);
        write_levels_f32(state, flatten);
        write_levels_f32(state, gnarliness);
        write_levels_f32(state, length);
        write_levels_f32(state, radius);
        write_levels_u32(state, sections);
        write_levels_u32(state, segments);
        write_levels_f32(state, start);
        write_levels_f32(state, taper);
        write_levels_f32(state, twist);
    }
}

/// Hashes the direction and strength of the force that influences branch growth.
/// Floating-point settings use their exact stored bits.
impl Hash for GrowthForce {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            direction,
            strength,
        } = self;
        write_vec3(state, *direction);
        write_f32(state, *strength);
    }
}

/// Hashes all leaf settings, including placement, size, texture, and appearance.
/// Floating-point settings use their exact stored bits.
impl Hash for LeafOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            kind,
            billboard,
            angle,
            count,
            start,
            size,
            size_variance,
            tint,
            alpha_test,
            rounded_normals,
            flatten,
        } = self;
        write_str(state, kind);
        write_billboard(state, *billboard);
        write_f32(state, *angle);
        state.write_u32(*count);
        write_f32(state, *start);
        write_f32(state, *size);
        write_f32(state, *size_variance);
        state.write_u32(*tint);
        write_f32(state, *alpha_test);
        write_bool(state, *rounded_normals);
        write_f32(state, *flatten);
    }
}

/// Hashes all trellis settings, even when the trellis is disabled or hidden.
/// Floating-point settings use their exact stored bits.
impl Hash for TrellisOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            enabled,
            position,
            width,
            height,
            spacing,
            force,
            cylinder_radius,
            visible,
            color,
        } = self;
        write_bool(state, *enabled);
        write_vec3(state, *position);
        write_f32(state, *width);
        write_f32(state, *height);
        write_f32(state, *spacing);
        force.hash(state);
        write_f32(state, *cylinder_radius);
        write_bool(state, *visible);
        state.write_u32(*color);
    }
}

/// Hashes the strength, range, and falloff of the trellis influence on growth.
/// Floating-point settings use their exact stored bits.
impl Hash for TrellisForce {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            strength,
            max_distance,
            falloff,
        } = self;
        write_f32(state, *strength);
        write_f32(state, *max_distance);
        write_f32(state, *falloff);
    }
}

/// Hashes all fruit settings, including placement, cluster shape, and maturity.
/// Floating-point settings use their exact stored bits.
impl Hash for FruitOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            placement,
            cluster,
            shape,
            maturity,
        } = self;
        placement.hash(state);
        cluster.hash(state);
        shape.hash(state);
        write_f32(state, *maturity);
    }
}

/// Hashes all flower settings, including placement, cluster shape, and maturity.
/// Floating-point settings use their exact stored bits.
impl Hash for FlowerOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            placement,
            cluster,
            shape,
            maturity,
        } = self;
        placement.hash(state);
        cluster.hash(state);
        shape.hash(state);
        write_f32(state, *maturity);
    }
}

/// Hashes all settings for where fruit or flower groups appear and how they face.
/// Floating-point settings use their exact stored bits.
impl Hash for ClusterPlacement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            min_level,
            tips,
            count,
            density,
            start,
            facing,
            facing_jitter,
            size,
            size_variance,
        } = self;
        state.write_u32(*min_level);
        write_f32(state, *tips);
        state.write_u32(*count);
        write_f32(state, *density);
        write_f32(state, *start);
        write_facing(state, *facing);
        write_f32(state, *facing_jitter);
        write_f32(state, *size);
        write_f32(state, *size_variance);
    }
}

/// Hashes all settings for the arrangement and stalks of a fruit or flower group.
/// Floating-point settings use their exact stored bits.
impl Hash for ClusterShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            kind,
            items,
            items_variance,
            spread,
            axis_length,
            droop,
            taper,
            side_stalks,
            side_length,
            stalk_length,
            stalk_radius,
            stalk_color,
        } = self;
        write_cluster_kind(state, *kind);
        state.write_u32(*items);
        write_f32(state, *items_variance);
        write_f32(state, *spread);
        write_f32(state, *axis_length);
        write_f32(state, *droop);
        write_f32(state, *taper);
        state.write_u32(*side_stalks);
        write_f32(state, *side_length);
        write_f32(state, *stalk_length);
        write_f32(state, *stalk_radius);
        state.write_u32(*stalk_color);
    }
}

/// Hashes all fruit shape and color settings, including stem details.
/// Floating-point settings use their exact stored bits.
impl Hash for FruitShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            elongation,
            top_dimple,
            bottom_taper,
            lobes,
            lobe_depth,
            segments,
            rings,
            stem_length,
            stem_radius,
            stem_curve,
            calyx,
            color,
            unripe_color,
            blush_color,
            blush_amount,
        } = self;
        write_f32(state, *elongation);
        write_f32(state, *top_dimple);
        write_f32(state, *bottom_taper);
        state.write_u32(*lobes);
        write_f32(state, *lobe_depth);
        state.write_u32(*segments);
        state.write_u32(*rings);
        write_f32(state, *stem_length);
        write_f32(state, *stem_radius);
        write_f32(state, *stem_curve);
        write_bool(state, *calyx);
        state.write_u32(*color);
        state.write_u32(*unripe_color);
        state.write_u32(*blush_color);
        write_f32(state, *blush_amount);
    }
}

/// Hashes all flower shape and color settings, including petal and center details.
/// Floating-point settings use their exact stored bits.
impl Hash for FlowerShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            petal_count,
            rings,
            petal_length,
            petal_width,
            cup,
            curl,
            notch,
            petal_segments,
            center_size,
            center_color,
            petal_color,
            base_color,
            bud_color,
            fade_color,
        } = self;
        state.write_u32(*petal_count);
        state.write_u32(*rings);
        write_f32(state, *petal_length);
        write_f32(state, *petal_width);
        write_f32(state, *cup);
        write_f32(state, *curl);
        write_f32(state, *notch);
        state.write_u32(*petal_segments);
        write_f32(state, *center_size);
        state.write_u32(*center_color);
        state.write_u32(*petal_color);
        state.write_u32(*base_color);
        state.write_u32(*bud_color);
        state.write_u32(*fade_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks that the default settings keep their known identifier.
    /// A failure can indicate changed defaults or changed hashing behavior;
    /// review the cause before updating the expected value.
    #[test]
    fn default_tree_id_is_pinned() {
        assert_eq!(TreeOptions::default().tree_id(), 7_038_842_139_415_929_464);
    }

    /// Checks that a `u32` uses little-endian bytes and a `usize` uses 64 bits.
    /// These encodings help keep identifiers independent of the platform.
    #[test]
    fn stable_hasher_writes_little_endian() {
        let mut a = StableHasher::new();
        a.write_u32(0x0102_0304);
        a.write_usize(5);
        let mut b = StableHasher::new();
        b.write(&[4, 3, 2, 1]);
        b.write(&5u64.to_le_bytes());
        assert_eq!(a.finish(), b.finish());
    }
}
