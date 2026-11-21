//! Identifies reusable tree materials, which control surface colour, texture
//! and shine. Trees with matching material settings can share these assets
//! even when their shapes differ.

use ze_tree::{
    BarkMaterial, BarkOptions, BarkUvMode, LeafMaterial, LeafOptions, TextureSize, TrellisMaterial,
    TrellisOptions,
};

use crate::spawn::{BarkTexture, LeafTexture};

/// Converts numbers to their stored bits so cache keys can compare and hash them.
fn bits<const N: usize>(values: [f32; N]) -> [u32; N] {
    values.map(f32::to_bits)
}

/// Selects how bark gets its surface colour and detail.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum BarkLook {
    /// Uses only the chosen bark colour, without a texture image.
    Tint,
    /// Uses texture images at the chosen resolution. An optional normal map
    /// adds the appearance of surface bumps without changing the shape.
    Textured {
        kind: String,
        size: TextureSize,
        normal_map: bool,
    },
    /// Combines the chosen tint with the bark texture's average colour.
    Average { kind: String },
}

/// Groups matching bark surface settings so their material can be reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct BarkKey {
    look: BarkLook,
    color: [u32; 3],
    metallic: u32,
    roughness: u32,
    two_sided: bool,
}

impl BarkKey {
    /// Identifies the requested bark appearance. With no texture selection,
    /// only the tint is used. `two_sided` keeps bark visible from either side.
    pub(crate) fn new(bark: &BarkOptions, look: Option<BarkTexture>, two_sided: bool) -> Self {
        let settings = BarkMaterial::new(bark);
        // When bark textures are disabled, only the tint affects its colour.
        let look = match look {
            Some(BarkTexture::Textured(size)) if settings.textured => BarkLook::Textured {
                kind: bark.kind.clone(),
                size,
                normal_map: bark.uv_mode == BarkUvMode::Continuous,
            },
            Some(BarkTexture::AverageColor) if settings.textured => BarkLook::Average {
                kind: bark.kind.clone(),
            },
            _ => BarkLook::Tint,
        };
        Self {
            look,
            color: bits(settings.base_color),
            metallic: settings.metallic.to_bits(),
            roughness: settings.roughness.to_bits(),
            two_sided,
        }
    }
}

/// Groups matching leaf surface settings so their material can be reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LeafKey {
    /// Selects the leaf texture image; `None` means no texture is used.
    textures: Option<(String, LeafTexture)>,
    color: [u32; 3],
    alpha_cutoff: u32,
    double_sided: bool,
    rounded_normals: bool,
    metallic: u32,
    roughness: u32,
    /// Whether camera-facing leaves need a material without a normal map.
    billboard: bool,
}

impl LeafKey {
    /// Returns a reusable material key for these leaf settings and optional texture.
    /// Set `billboard` when the mesh turns toward the camera and must omit normal maps.
    pub(crate) fn new(leaves: &LeafOptions, image: Option<LeafTexture>, billboard: bool) -> Self {
        let settings = LeafMaterial::new(leaves);
        Self {
            textures: image.map(|image| (leaves.kind.clone(), image)),
            color: bits(settings.base_color),
            alpha_cutoff: settings.alpha_cutoff.to_bits(),
            double_sided: settings.double_sided,
            rounded_normals: settings.rounded_normals,
            metallic: settings.metallic.to_bits(),
            roughness: settings.roughness.to_bits(),
            billboard,
        }
    }
}

/// Groups matching surface settings for the wires of a tree support frame (trellis).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TrellisKey([u32; 5]);

impl TrellisKey {
    /// Identifies the wire colour and how the surface reflects light.
    pub(crate) fn new(trellis: &TrellisOptions) -> Self {
        let settings = TrellisMaterial::new(trellis);
        let [r, g, b] = settings.base_color;
        Self(bits([r, g, b, settings.metallic, settings.roughness]))
    }
}

#[cfg(test)]
mod tests {
    use ze_tree::TreeOptions;

    use super::*;

    #[test]
    fn bark_keys_follow_what_the_material_shows() {
        let bark = TreeOptions::default().bark;
        let textured = Some(BarkTexture::Textured(TextureSize::Size1k));
        let key = BarkKey::new(&bark, textured, false);

        // Matching settings reuse a key; surface bumps, tint and visibility
        // from both sides require distinct keys.
        assert_eq!(key, BarkKey::new(&bark.clone(), textured, false));
        let mut mirrored = bark.clone();
        mirrored.uv_mode = BarkUvMode::Mirrored;
        assert_ne!(key, BarkKey::new(&mirrored, textured, false));
        let mut tinted = bark.clone();
        tinted.tint ^= 0x10;
        assert_ne!(key, BarkKey::new(&tinted, textured, false));
        assert_ne!(key, BarkKey::new(&bark, textured, true));

        // With textures disabled, changing the texture selection or bark kind
        // does not change the material key.
        let mut plain = bark.clone();
        plain.textured = false;
        let tint_only = BarkKey::new(&plain, None, false);
        assert_eq!(tint_only, BarkKey::new(&plain, textured, false));
        assert_eq!(
            tint_only,
            BarkKey::new(&plain, Some(BarkTexture::AverageColor), false)
        );
        plain.kind = "other".to_owned();
        assert_eq!(tint_only, BarkKey::new(&plain, textured, false));
    }

    #[test]
    fn leaf_keys_follow_what_the_material_shows() {
        let leaves = TreeOptions::default().leaves;
        let key = LeafKey::new(&leaves, Some(LeafTexture::Leaf), false);
        assert_ne!(key, LeafKey::new(&leaves, Some(LeafTexture::Clump), false));
        assert_ne!(key, LeafKey::new(&leaves, None, false));
        // Camera-facing leaves must not share a material that uses a normal map.
        assert_ne!(key, LeafKey::new(&leaves, Some(LeafTexture::Leaf), true));

        // Changing the number or size of leaves preserves the material key.
        let mut more = leaves.clone();
        more.count += 3;
        more.size *= 2.0;
        assert_eq!(key, LeafKey::new(&more, Some(LeafTexture::Leaf), false));
        let mut sharper = leaves.clone();
        sharper.alpha_test += 0.1;
        assert_ne!(key, LeafKey::new(&sharper, Some(LeafTexture::Leaf), false));
    }
}
