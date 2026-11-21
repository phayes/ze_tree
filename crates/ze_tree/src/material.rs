//! Appearance settings to apply in your renderer for bark, leaves, fruit, flowers,
//! and trellis guide wires. These settings do not configure the renderer themselves.
//! Fruit and flowers use mesh vertex colours. For bark and leaf textures, use
//! [`BARK_KINDS`] and [`LEAF_KINDS`] with [`bark_texture_path`] and
//! [`leaf_texture_path`] to find files in the repository's `assets/textures/` folder.

use crate::{BarkOptions, LeafOptions, TrellisOptions};

/// Available bark texture names. Use these values for [`BarkOptions::kind`];
/// they include every built-in preset's bark.
pub const BARK_KINDS: [&str; 31] = [
    "bark_brown_02",
    "bark_brown_01",
    "bark_willow",
    "bark_willow_02",
    "pine_bark",
    "tree_bark_03",
    "palm_tree_bark",
    "sakura_bark",
    "japanese_sycamore",
    "japanese_hackberry_bark",
    "chinese_cedar_bark",
    "japanese_zelkova_bark",
    "japanese_camphor_bark",
    "knotted_pine_bark",
    "jolcham_oak_bark_01",
    "eucalyptus_bark",
    "trident_maple_bark",
    "palm_bark",
    "japanese_cedar_bark",
    "bark_platanus",
    "bark_bluegum",
    "metasequoia_bark",
    "chinese_hackberry_bark",
    "Bark001",
    "Bark002",
    "Bark003",
    "Bark009",
    "Bark014",
    "Bark016",
    "apple_bark_01",
    "apple_bark_02",
];

/// Bark texture resolution. Each size fits within a square of that many pixels;
/// textures that are taller than they are wide keep their shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureSize {
    /// Fits within 512 × 512 pixels: the least memory, for distant trees.
    Size512,
    /// Fits within 1024 × 1024 pixels.
    Size1k,
    /// Fits within 2048 × 2048 pixels: the most detail, for close-up views.
    /// The repository's download script skips this size unless asked.
    Size2k,
}

impl TextureSize {
    /// Every size, smallest first.
    pub const ALL: [TextureSize; 3] = [Self::Size512, Self::Size1k, Self::Size2k];

    /// The size as written in texture file names: `"512"`, `"1k"` or `"2k"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Size512 => "512",
            Self::Size1k => "1k",
            Self::Size2k => "2k",
        }
    }
}

/// GPU compression format of a texture file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextureCompression {
    /// BC7, read directly by GPUs that support it, including most desktop GPUs.
    #[default]
    Bc7,
    /// Basis Universal UASTC. The loader converts it to a format the GPU supports;
    /// in Bevy this needs the `basis-universal` feature.
    Uastc,
}

impl TextureCompression {
    /// Both formats.
    pub const ALL: [TextureCompression; 2] = [Self::Bc7, Self::Uastc];

    /// The format as written in texture file names: `"bc7"` or `"uastc"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bc7 => "bc7",
            Self::Uastc => "uastc",
        }
    }
}

/// One of the three images that make up a bark texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarkMap {
    /// Surface colour. Load as sRGB.
    Color,
    /// Surface lighting detail, using the OpenGL (+Y) normal map convention.
    /// Load as linear data.
    Normal,
    /// How matte the surface is, with the same value in red, green and blue.
    /// Load as linear data.
    Roughness,
}

impl BarkMap {
    /// Every map.
    pub const ALL: [BarkMap; 3] = [Self::Color, Self::Normal, Self::Roughness];

    /// The map as written in texture file names: `"color"`, `"normal"` or `"roughness"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Normal => "normal",
            Self::Roughness => "roughness",
        }
    }

    /// Whether the image holds sRGB colour rather than linear data.
    pub fn is_srgb(self) -> bool {
        self == Self::Color
    }
}

/// Returns the path of a bark texture file relative to the texture folder, such as
/// `"bark/sakura_bark-1k-color-bc7.ktx2"`.
///
/// Pass a name from [`BARK_KINDS`] as `kind`. The repository's texture folder is
/// `assets/textures/`; its bark files come from `tools/textures/download.sh`.
pub fn bark_texture_path(
    kind: &str,
    size: TextureSize,
    map: BarkMap,
    compression: TextureCompression,
) -> String {
    format!(
        "bark/{kind}-{}-{}-{}.ktx2",
        size.as_str(),
        map.as_str(),
        compression.as_str()
    )
}

/// Texture image types for leaves. Every built-in leaf kind provides a colour
/// image and an image showing several leaves together. Some also provide a normal
/// map for lighting detail; check availability with [`leaf_has_normal_map`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeafMap {
    /// Leaf colour and transparency, which cuts out the leaf or branch outline.
    /// Load as sRGB. Alpha is straight: RGB colours are not multiplied by opacity.
    Color,
    /// Surface directions that add lighting detail without adding geometry.
    /// Uses the OpenGL (+Y) convention; load as linear data, without sRGB conversion.
    /// Available only when [`leaf_has_normal_map`] returns true.
    Normal,
    /// A 256 × 256 colour image showing several leaves on one flat surface
    /// (a leaf card). Use it with fewer, larger cards for distant trees.
    /// Leaves spread from the bottom centre, where the card attaches to a branch.
    /// Load as sRGB with straight alpha, as for [`Self::Color`]. No normal map is provided.
    Clump,
}

impl LeafMap {
    /// All three image types. This is not an availability list: some leaf kinds
    /// do not provide [`Self::Normal`].
    pub const ALL: [LeafMap; 3] = [Self::Color, Self::Normal, Self::Clump];

    /// Returns the filename label for this image type: `"color"`, `"normal"` or `"clump"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Normal => "normal",
            Self::Clump => "clump",
        }
    }

    /// Whether to load this map as sRGB colour. Returns false for normal maps,
    /// whose values describe directions and must be read as linear data.
    pub fn is_srgb(self) -> bool {
        self != Self::Normal
    }
}

/// Builds a path relative to `assets/textures/`, such as `"leaves/oak-color-bc7.ktx2"`.
///
/// Use a name from [`LEAF_KINDS`] for `kind`. Leaf textures have one size per kind,
/// so no size argument is needed. Download the files with `tools/textures/download.sh`.
///
/// This function only builds a path; it does not check that the file exists. Request
/// [`LeafMap::Normal`] only when [`leaf_has_normal_map`] returns true for `kind`.
pub fn leaf_texture_path(kind: &str, map: LeafMap, compression: TextureCompression) -> String {
    format!(
        "leaves/{kind}-{}-{}.ktx2",
        map.as_str(),
        compression.as_str()
    )
}

/// Whether a built-in leaf kind provides a normal map for surface lighting detail.
/// For example, `"white_oak"` does and `"oak"` does not. Unknown names return false.
/// This checks the available texture set, not whether the file has been downloaded.
pub fn leaf_has_normal_map(kind: &str) -> bool {
    LEAF_KINDS[EZ_TREE_LEAF_KIND_COUNT..].contains(&kind)
}

/// Available texture names for [`LeafOptions::kind`], including those used by all presets.
///
/// The first four (`ash`, `aspen`, `oak`, `pine`) come from ez-tree and have colour
/// images only. The remaining 22 come from Nobiax's CC0 "Free 3D branches textures"
/// packs and depict branch tips or bare twigs; `cedar` and `maple` are the
/// arborvitae and field maple branches recoloured a bright green that suits tinting.
/// Each includes a colour image and a normal map for lighting detail; use
/// [`leaf_has_normal_map`] to check a name.
pub const LEAF_KINDS: [&str; 26] = [
    "ash",
    "aspen",
    "oak",
    "pine",
    "aspen_autumn",
    "european_aspen",
    "bigtooth_aspen",
    "alder_buckthorn",
    "smoke_bush",
    "field_maple_autumn",
    "bamboo",
    "fir",
    "white_pine",
    "arborvitae_autumn",
    "birch_autumn",
    "larch",
    "black_locust",
    "olive",
    "cotoneaster",
    "bare_twig_1",
    "bare_twig_2",
    "white_oak_autumn",
    "white_oak",
    "white_oak_dead",
    "cedar",
    "maple",
];

/// Number of ez-tree entries at the start of [`LEAF_KINDS`]. These have colour
/// images only; the remaining entries also provide normal maps.
const EZ_TREE_LEAF_KIND_COUNT: usize = 4;

/// Returns a built-in bark texture's average colour as linear RGB values from 0 to 1.
/// Pass a name from [`BARK_KINDS`]; other names return `None`.
///
/// For distant bark without texture images, multiply each channel by the matching
/// channel of [`BarkMaterial::base_color`]. This approximates the textured bark
/// colour. Calling this function does not require texture files.
pub fn bark_average_color(kind: &str) -> Option<[f32; 3]> {
    BARK_AVERAGE_COLORS
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|&(_, color)| color)
}

/// Precomputed average bark colours in linear RGB. Regenerate with
/// `tools/textures/bark_colors.sh` after changing the source textures.
const BARK_AVERAGE_COLORS: [(&str, [f32; 3]); BARK_KINDS.len()] = [
    ("bark_brown_02", [0.1298, 0.1069, 0.0647]),
    ("bark_brown_01", [0.1888, 0.1261, 0.0468]),
    ("bark_willow", [0.0565, 0.0381, 0.0251]),
    ("bark_willow_02", [0.2482, 0.1959, 0.1530]),
    ("pine_bark", [0.1391, 0.0895, 0.0545]),
    ("tree_bark_03", [0.2879, 0.2562, 0.1566]),
    ("palm_tree_bark", [0.3499, 0.2964, 0.2392]),
    ("sakura_bark", [0.2061, 0.1455, 0.1021]),
    ("japanese_sycamore", [0.3412, 0.3004, 0.2370]),
    ("japanese_hackberry_bark", [0.2288, 0.1727, 0.0901]),
    ("chinese_cedar_bark", [0.0893, 0.0530, 0.0309]),
    ("japanese_zelkova_bark", [0.3340, 0.2393, 0.1625]),
    ("japanese_camphor_bark", [0.3761, 0.2513, 0.1682]),
    ("knotted_pine_bark", [0.0747, 0.0595, 0.0335]),
    ("jolcham_oak_bark_01", [0.1136, 0.0884, 0.0525]),
    ("eucalyptus_bark", [0.0916, 0.0613, 0.0316]),
    ("trident_maple_bark", [0.0782, 0.0478, 0.0251]),
    ("palm_bark", [0.1000, 0.0769, 0.0603]),
    ("japanese_cedar_bark", [0.0660, 0.0366, 0.0183]),
    ("bark_platanus", [0.0994, 0.0687, 0.0490]),
    ("bark_bluegum", [0.1078, 0.0785, 0.0551]),
    ("metasequoia_bark", [0.1018, 0.0519, 0.0278]),
    ("chinese_hackberry_bark", [0.0911, 0.0649, 0.0352]),
    ("Bark001", [0.2313, 0.2235, 0.2060]),
    ("Bark002", [0.2202, 0.1846, 0.1242]),
    ("Bark003", [0.2632, 0.1933, 0.1097]),
    ("Bark009", [0.6170, 0.6044, 0.4250]),
    ("Bark014", [0.2022, 0.1417, 0.0881]),
    ("Bark016", [0.1691, 0.1348, 0.0549]),
    ("apple_bark_01", [0.1581, 0.1455, 0.1358]),
    ("apple_bark_02", [0.3574, 0.2886, 0.2331]),
];

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

/// Appearance settings for fruit and their supporting stalks.
///
/// Use [`MeshBuffers::colors`](crate::MeshBuffers::colors) as the surface
/// colour; these per-vertex values are already in linear RGBA for lighting.
/// No texture is required. Apply these settings in your renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FruitMaterial {
    /// Metallic appearance from 0 (nonmetal) to 1 (metal). Defaults to 0.
    pub metallic: f32,
    /// Surface roughness from 0 (smooth and shiny) to 1 (matte).
    /// Defaults to 0.45.
    pub roughness: f32,
    /// Whether to render both faces of each triangle. Defaults to true, because
    /// low-detail fruit stalks and flat fruit are single surfaces seen from both sides.
    pub double_sided: bool,
}

impl Default for FruitMaterial {
    fn default() -> Self {
        Self {
            metallic: 0.0,
            roughness: 0.45,
            double_sided: true,
        }
    }
}

/// Appearance settings for flowers and their supporting stalks.
///
/// Use [`MeshBuffers::colors`](crate::MeshBuffers::colors) as the surface
/// colour; these per-vertex values are already in linear RGBA for lighting.
/// No texture is required. Apply these settings in your renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowerMaterial {
    /// Metallic appearance from 0 (nonmetal) to 1 (metal). Defaults to 0.
    pub metallic: f32,
    /// Surface roughness from 0 (smooth and shiny) to 1 (matte).
    /// Defaults to 0.8.
    pub roughness: f32,
    /// Whether to render both faces of each triangle. Defaults to true.
    pub double_sided: bool,
}

impl Default for FlowerMaterial {
    fn default() -> Self {
        Self {
            metallic: 0.0,
            roughness: 0.8,
            double_sided: true,
        }
    }
}

/// Converts a display colour written as `0xRRGGBB` to linear red, green, and
/// blue values in 0–1 for lighting calculations. Ignores bits above the lowest
/// 24; the input does not supply an alpha value.
pub(crate) fn srgb_hex_to_linear(hex: u32) -> [f32; 3] {
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
