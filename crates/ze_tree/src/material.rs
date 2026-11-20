//! Bark and leaf settings to apply in your renderer, with helpers for locating
//! the repository's textures. KTX2 files package compressed images for GPU use.

use crate::{BarkOptions, LeafOptions};

/// The texture encoding to load from the repository's `assets/textures/` folder.
///
/// Both sets contain the same images in KTX2 files. Each includes smaller image
/// versions (mipmaps) for rendering distant surfaces and uses Zstandard (zstd)
/// compression to reduce file size. The files are not included in the published crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureSet {
    /// Basis Universal UASTC, a compressed format that a compatible loader
    /// converts to a format supported by the target GPU. Use it for portability
    /// or glTF with `KHR_texture_basisu`. Bevy requires its `basis-universal` feature.
    Uastc,
    /// BC7, a compressed format that supported GPUs can use directly. Check
    /// your target hardware and loader: support is common on desktop GPUs, but
    /// limited on mobile devices and in glTF viewers.
    Bc7,
}

impl TextureSet {
    /// Returns `"uastc"` or `"bc7"`, the subfolder under `assets/textures/`.
    pub fn dir(self) -> &'static str {
        match self {
            Self::Uastc => "uastc",
            Self::Bc7 => "bc7",
        }
    }
}

/// The type of bark texture to load: colour, surface detail, or roughness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarkMap {
    /// Surface colour in sRGB, the colour encoding commonly used for images.
    /// Load it as an sRGB texture so the renderer converts colours for lighting.
    Color,
    /// Surface lighting detail without extra geometry. Load as linear data,
    /// using the OpenGL normal-map convention: green points along increasing
    /// texture `v`. This is also the convention used by glTF and Bevy.
    Normal,
    /// How matte the surface looks; larger values mean less shiny. Load as
    /// linear data. Red, green, and blue contain the same value. For a glTF or
    /// Bevy metallic-roughness texture, keep the material metallic factor at
    /// zero so the blue channel does not make the bark metallic.
    Roughness,
}

impl BarkMap {
    /// All three bark texture types, ordered as colour, normal, then roughness.
    pub const ALL: [Self; 3] = [Self::Color, Self::Normal, Self::Roughness];

    fn file_stem(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Normal => "normal",
            Self::Roughness => "roughness",
        }
    }
}

/// Available bark texture names. Use these values for [`BarkOptions::kind`];
/// they cover all built-in presets.
pub const BARK_KINDS: [&str; 3] = ["Bark001", "Bark002", "Bark003"];

/// Available leaf texture names. Use these values for [`LeafOptions::kind`];
/// they cover all built-in presets.
pub const LEAF_KINDS: [&str; 4] = ["ash", "aspen", "oak", "pine"];

/// Returns a bark texture path relative to `assets/textures/`, such as
/// `"bc7/bark/Bark001/normal.ktx2"`.
///
/// Pass the encoding, a kind from [`BARK_KINDS`] (or [`BarkOptions::kind`]), and
/// the texture type. This only builds a path; it does not check or load the file.
pub fn bark_texture_path(set: TextureSet, kind: &str, map: BarkMap) -> String {
    format!("{}/bark/{kind}/{}.ktx2", set.dir(), map.file_stem())
}

/// Returns a leaf texture path relative to `assets/textures/`, such as
/// `"uastc/leaves/oak.ktx2"`.
///
/// Pass the encoding and a kind from [`LEAF_KINDS`] (or [`LeafOptions::kind`]).
/// This only builds a path; it does not check or load the file. Load leaf colours
/// as sRGB. Their alpha stores transparency separately from colour (straight,
/// not premultiplied, alpha).
pub fn leaf_texture_path(set: TextureSet, kind: &str) -> String {
    format!("{}/leaves/{kind}.ktx2", set.dir())
}

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

    #[test]
    fn paths_follow_the_asset_layout() {
        assert_eq!(
            bark_texture_path(TextureSet::Bc7, "Bark001", BarkMap::Normal),
            "bc7/bark/Bark001/normal.ktx2"
        );
        assert_eq!(
            leaf_texture_path(TextureSet::Uastc, "oak"),
            "uastc/leaves/oak.ktx2"
        );
    }
}
