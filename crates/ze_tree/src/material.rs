//! Settings for the appearance of bark, leaves, and trellis guide wires.
//! Apply these settings in your renderer, and use [`BARK_KINDS`] and [`LEAF_KINDS`]
//! to choose textures from the repository's `assets/textures/` folder.

use crate::{BarkOptions, LeafOptions, TrellisOptions};

/// Available bark texture names. Use these values for [`BarkOptions::kind`];
/// they cover all built-in presets.
pub const BARK_KINDS: [&str; 3] = ["Bark001", "Bark002", "Bark003"];

/// Available leaf texture names. Use these values for [`LeafOptions::kind`];
/// they cover all built-in presets.
pub const LEAF_KINDS: [&str; 4] = ["ash", "aspen", "oak", "pine"];

/// Bark shading settings for your renderer, matching ez-tree's appearance.
/// Creating these settings does not load textures or configure a renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarkMaterial {
    /// Tint in linear RGB (0–1 per channel), the colour space used for lighting.
    /// Multiplies the colour texture when textures are enabled.
    pub base_color: [f32; 3],
    /// Whether to use one lighting normal per triangle for a faceted appearance.
    pub flat_shading: bool,
    /// Whether the renderer should use bark textures. If false, use
    /// [`base_color`](Self::base_color) as the surface colour.
    pub textured: bool,
    /// Scale to apply to mesh texture coordinates `[u, v]` before sampling.
    /// Configure bark textures to repeat in both directions.
    pub uv_scale: [f32; 2],
    /// Metallic factor: 0 for nonmetal surfaces, 1 for metal. Defaults to 0.
    pub metallic: f32,
    /// Roughness factor: 0 is smooth and shiny, 1 is matte. Multiplies the
    /// roughness texture when used; defaults to 1.
    pub roughness: f32,
}

impl BarkMaterial {
    /// Creates bark shading settings from `options`, including a linear-colour tint.
    ///
    /// The returned UV scale is `[1, 1 / options.texture_scale.y]`. Use a nonzero
    /// `texture_scale.y`; zero produces an infinite scale. The mesh already includes
    /// the scale around the branch, so do not apply `texture_scale.x` again.
    pub fn new(options: &BarkOptions) -> Self {
        Self {
            base_color: srgb_hex_to_linear(options.tint),
            flat_shading: options.flat_shading,
            textured: options.textured,
            uv_scale: [1.0, 1.0 / options.texture_scale.y],
            metallic: 0.0,
            roughness: 1.0,
        }
    }
}

/// Leaf shading settings for your renderer, matching ez-tree's appearance.
///
/// Leaf meshes are flat rectangles. Use the texture's alpha to discard pixels
/// outside the leaf shape (alpha testing), rather than blending transparent pixels.
/// Creating these settings does not load textures or configure a renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeafMaterial {
    /// Tint to multiply with the leaf texture, in linear RGB (0–1 per channel).
    pub base_color: [f32; 3],
    /// Discard pixels whose texture alpha is below this threshold, normally 0–1.
    pub alpha_cutoff: f32,
    /// Whether to draw both sides of each leaf. The constructor sets this to true.
    pub double_sided: bool,
    /// Whether leaf lighting should give the canopy a rounded appearance.
    /// When true, use the mesh normal unchanged on back faces too. When false,
    /// flip back-face normals as usual for a double-sided material.
    pub rounded_normals: bool,
    /// Metallic factor: 0 for nonmetal surfaces, 1 for metal. Defaults to 0.
    pub metallic: f32,
    /// Roughness factor: 0 is smooth and shiny, 1 is matte. Defaults to 1.
    pub roughness: f32,
}

impl LeafMaterial {
    /// Creates leaf shading settings from `options`, including a linear-colour tint.
    /// The result draws both sides and uses a nonmetallic, fully rough surface.
    pub fn new(options: &LeafOptions) -> Self {
        Self {
            base_color: srgb_hex_to_linear(options.tint),
            alpha_cutoff: options.alpha_test,
            double_sided: true,
            rounded_normals: options.rounded_normals,
            metallic: 0.0,
            roughness: 1.0,
        }
    }
}

/// Appearance settings for drawing the guide wires from [`TrellisOptions::wires`].
/// Apply these settings in your renderer; no texture is needed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrellisMaterial {
    /// Wire colour as red, green, and blue values from 0 to 1, in the linear
    /// colour space used for lighting.
    pub base_color: [f32; 3],
    /// How metallic the surface looks: 0 for nonmetal, 1 for metal.
    /// [`Self::new`] sets this to 0.
    pub metallic: f32,
    /// How rough the surface looks: 0 is smooth and shiny, 1 is matte.
    /// [`Self::new`] sets this to 0.8.
    pub roughness: f32,
}

impl TrellisMaterial {
    /// Prepares the trellis colour for lighting and sets a matte, nonmetal appearance.
    /// The colour is converted from the options' sRGB value to linear RGB.
    pub fn new(options: &TrellisOptions) -> Self {
        Self {
            base_color: srgb_hex_to_linear(options.color),
            metallic: 0.0,
            roughness: 0.8,
        }
    }
}

/// Converts the lower three bytes of `hex` from an sRGB `0xRRGGBB` colour
/// to linear RGB values in 0–1, suitable for lighting calculations.
fn srgb_hex_to_linear(hex: u32) -> [f32; 3] {
    let channel = |shift: u32| {
        let c = ((hex >> shift) & 0xff) as f32 / 255.0;
        if c < 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [channel(16), channel(8), channel(0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_conversion_matches_three() {
        // Match Three.js: an sRGB channel of 128 becomes about 0.21586 in linear RGB.
        let [r, g, b] = srgb_hex_to_linear(0x80_00_ff);
        assert!((r - 0.215_860_5).abs() < 1e-6, "{r}");
        assert_eq!(g, 0.0);
        assert!((b - 1.0).abs() < 1e-6, "{b}");
        // Very dark sRGB values use the linear part of the conversion curve.
        let [r, _, _] = srgb_hex_to_linear(0x0a_00_00);
        assert!((r - 10.0 / 255.0 / 12.92).abs() < 1e-9, "{r}");
    }
}
