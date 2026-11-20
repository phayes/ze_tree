//! Verify material settings and the texture files needed by every preset.
//! KTX2 stores compressed texture images; these checks read file metadata to
//! confirm encoding, colour handling, and smaller images for distant rendering.

use std::path::PathBuf;

use ze_tree::{
    BARK_KINDS, BarkMap, BarkMaterial, LEAF_KINDS, LeafMaterial, TextureSet, TreeOptions,
    TreePreset, bark_texture_path, leaf_texture_path,
};

const KTX2_IDENTIFIER: [u8; 12] = *b"\xabKTX 20\xbb\r\n\x1a\n";
const VK_FORMAT_UNDEFINED: u32 = 0;
const VK_FORMAT_BC7_UNORM_BLOCK: u32 = 145;
const VK_FORMAT_BC7_SRGB_BLOCK: u32 = 146;
const KHR_DF_MODEL_UASTC: u8 = 166;
const KHR_DF_TRANSFER_LINEAR: u8 = 1;
const KHR_DF_TRANSFER_SRGB: u8 = 2;
const SUPERCOMPRESSION_ZSTD: u32 = 2;

/// Texture metadata used to check the image encoding, colour space,
/// compression, and number of image sizes (mipmap levels).
struct Ktx2Header {
    vk_format: u32,
    level_count: u32,
    supercompression: u32,
    color_model: u8,
    transfer: u8,
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// Reads metadata for a path relative to `assets/textures/`.
/// Panics if the file cannot be read, lacks the KTX2 signature, or is too short
/// for the fields read here. This is a test helper, not a full file validator.
fn ktx2_header(rel: &str) -> Ktx2Header {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/textures")
        .join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    assert_eq!(bytes[..12], KTX2_IDENTIFIER, "{rel} is not KTX2");

    // KTX 2.0 section 3 places the data format descriptor offset at byte 48.
    let dfd = read_u32(&bytes, 48) as usize;
    // Skip the descriptor's 4-byte size and 8-byte block header to reach
    // the colour model. The colour transfer function is two bytes later.
    Ktx2Header {
        vk_format: read_u32(&bytes, 12),
        level_count: read_u32(&bytes, 40),
        supercompression: read_u32(&bytes, 44),
        color_model: bytes[dfd + 12],
        transfer: bytes[dfd + 14],
    }
}

/// Checks the file against the requested encoding and colour space.
/// Set `srgb` for image colours; use linear data for normal and roughness maps.
/// Also requires a full set of smaller images and Zstandard compression.
fn check_texture(set: TextureSet, rel: &str, srgb: bool) {
    let header = ktx2_header(rel);
    let transfer = if srgb {
        KHR_DF_TRANSFER_SRGB
    } else {
        KHR_DF_TRANSFER_LINEAR
    };
    assert_eq!(header.transfer, transfer, "{rel} colour space");
    match set {
        TextureSet::Uastc => {
            assert_eq!(header.vk_format, VK_FORMAT_UNDEFINED, "{rel}");
            assert_eq!(header.color_model, KHR_DF_MODEL_UASTC, "{rel}");
        }
        TextureSet::Bc7 => {
            let format = if srgb {
                VK_FORMAT_BC7_SRGB_BLOCK
            } else {
                VK_FORMAT_BC7_UNORM_BLOCK
            };
            assert_eq!(header.vk_format, format, "{rel}");
        }
    }
    // Mipmaps halve the image dimensions at each level for distant rendering.
    // A 1024×1024 source needs 11 levels, including the source and final 1×1 image.
    assert_eq!(header.level_count, 11, "{rel} mip levels");
    assert_eq!(header.supercompression, SUPERCOMPRESSION_ZSTD, "{rel}");
}

#[test]
fn every_preset_texture_kind_is_provided() {
    for preset in TreePreset::ALL {
        let options = preset.options();
        assert!(
            BARK_KINDS.contains(&options.bark.kind.as_str()),
            "{}: {}",
            preset.name(),
            options.bark.kind
        );
        assert!(
            LEAF_KINDS.contains(&options.leaves.kind.as_str()),
            "{}: {}",
            preset.name(),
            options.leaves.kind
        );
    }
}

#[test]
fn texture_files_have_the_documented_formats() {
    for set in [TextureSet::Uastc, TextureSet::Bc7] {
        for kind in BARK_KINDS {
            for map in BarkMap::ALL {
                let srgb = map == BarkMap::Color;
                check_texture(set, &bark_texture_path(set, kind, map), srgb);
            }
        }
        for kind in LEAF_KINDS {
            check_texture(set, &leaf_texture_path(set, kind), true);
        }
    }
}

#[test]
fn materials_follow_the_options() {
    let mut options = TreeOptions::default();
    options.bark.tint = 0xff_00_00;
    options.bark.flat_shading = true;
    options.bark.texture_scale.y = 4.0;
    options.leaves.alpha_test = 0.25;
    options.leaves.rounded_normals = false;

    let bark = BarkMaterial::new(&options.bark);
    assert_eq!(bark.base_color, [1.0, 0.0, 0.0]);
    assert!(bark.flat_shading && bark.textured);
    assert_eq!(bark.uv_scale, [1.0, 0.25]);
    assert_eq!((bark.metallic, bark.roughness), (0.0, 1.0));

    let leaf = LeafMaterial::new(&options.leaves);
    assert_eq!(leaf.base_color, [1.0, 1.0, 1.0]);
    assert_eq!(leaf.alpha_cutoff, 0.25);
    assert!(leaf.double_sided && !leaf.rounded_normals);
}
