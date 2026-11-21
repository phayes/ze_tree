//! Check material settings and the manifest's coverage of available bark and leaf textures.
//!
//! The file checks read KTX2 metadata to verify compression, colour space, dimensions,
//! and mipmaps (smaller images used when textures are viewed from a distance).
//! Texture images must be downloaded separately. Run `tools/textures/download.sh`
//! with its default sizes and formats, then run the file checks with
//! `cargo test -p ze_tree --test material -- --ignored`.

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::Value;
use ze_tree::{
    BARK_KINDS, BarkMap, BarkMaterial, LEAF_KINDS, LeafMap, LeafMaterial, TextureCompression,
    TextureSize, TreeOptions, TreePreset, TrellisMaterial, bark_average_color, bark_texture_path,
    leaf_has_normal_map, leaf_texture_path,
};

const KTX2_IDENTIFIER: [u8; 12] = *b"\xabKTX 20\xbb\r\n\x1a\n";
const VK_FORMAT_UNDEFINED: u32 = 0;
const VK_FORMAT_BC7_UNORM_BLOCK: u32 = 145;
const VK_FORMAT_BC7_SRGB_BLOCK: u32 = 146;
const KHR_DF_MODEL_UASTC: u8 = 166;
const KHR_DF_TRANSFER_LINEAR: u8 = 1;
const KHR_DF_TRANSFER_SRGB: u8 = 2;
const SUPERCOMPRESSION_ZSTD: u32 = 2;

/// Texture metadata used to check the image encoding, colour space, size,
/// compression, and number of image sizes (mipmap levels).
struct Ktx2Header {
    vk_format: u32,
    width: u32,
    height: u32,
    level_count: u32,
    supercompression: u32,
    color_model: u8,
    transfer: u8,
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// The repository's `assets/textures/` folder.
fn texture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/textures")
}

/// Reads metadata for a path relative to `assets/textures/`.
/// Panics if the file cannot be read, lacks the KTX2 signature, or is too short
/// for the fields read here. This is a test helper, not a full file validator.
fn ktx2_header(rel: &str) -> Ktx2Header {
    let path = texture_dir().join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    assert_eq!(bytes[..12], KTX2_IDENTIFIER, "{rel} is not KTX2");

    // KTX 2.0 section 3 places the data format descriptor offset at byte 48.
    let dfd = read_u32(&bytes, 48) as usize;
    // Skip the descriptor's 4-byte size and 8-byte block header to reach
    // the colour model. The colour transfer function is two bytes later.
    Ktx2Header {
        vk_format: read_u32(&bytes, 12),
        width: read_u32(&bytes, 20),
        height: read_u32(&bytes, 24),
        level_count: read_u32(&bytes, 40),
        supercompression: read_u32(&bytes, 44),
        color_model: bytes[dfd + 12],
        transfer: bytes[dfd + 14],
    }
}

/// Checks the file against the requested encoding and colour space, and returns
/// its width and height. Set `srgb` for image colours; use linear data for normal
/// and roughness maps. Also requires a full set of smaller images and Zstandard
/// compression.
fn check_texture(compression: TextureCompression, rel: &str, srgb: bool) -> (u32, u32) {
    let header = ktx2_header(rel);
    let transfer = if srgb {
        KHR_DF_TRANSFER_SRGB
    } else {
        KHR_DF_TRANSFER_LINEAR
    };
    assert_eq!(header.transfer, transfer, "{rel} colour space");
    match compression {
        TextureCompression::Uastc => {
            assert_eq!(header.vk_format, VK_FORMAT_UNDEFINED, "{rel}");
            assert_eq!(header.color_model, KHR_DF_MODEL_UASTC, "{rel}");
        }
        TextureCompression::Bc7 => {
            let format = if srgb {
                VK_FORMAT_BC7_SRGB_BLOCK
            } else {
                VK_FORMAT_BC7_UNORM_BLOCK
            };
            assert_eq!(header.vk_format, format, "{rel}");
        }
    }
    // Mipmaps halve the image dimensions at each level down to 1×1, so a full
    // set has one level per halving of the longer side, plus the source image.
    let longest = header.width.max(header.height);
    assert!(longest > 0, "{rel} has no size");
    assert_eq!(header.level_count, longest.ilog2() + 1, "{rel} mip levels");
    assert_eq!(header.supercompression, SUPERCOMPRESSION_ZSTD, "{rel}");
    (header.width, header.height)
}

/// Reads `assets/textures/assets_manifest.json`.
fn manifest() -> Value {
    let path = texture_dir().join("assets_manifest.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).expect("manifest is valid JSON")
}

/// Returns texture entries in the requested manifest category, such as `"bark"`
/// or `"leaves"`.
fn manifest_assets<'a>(manifest: &'a Value, category: &str) -> Vec<&'a Value> {
    manifest["assets"]
        .as_array()
        .expect("manifest assets")
        .iter()
        .filter(|asset| str_field(asset, "category") == category)
        .collect()
}

/// Collects all file entries for a manifest category, across its texture kinds.
fn manifest_files<'a>(manifest: &'a Value, category: &str) -> Vec<&'a Value> {
    manifest_assets(manifest, category)
        .into_iter()
        .flat_map(|asset| asset["files"].as_array().expect("asset files"))
        .collect()
}

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} in {value}"))
}

fn size_from(name: &str) -> TextureSize {
    *TextureSize::ALL
        .iter()
        .find(|s| s.as_str() == name)
        .unwrap_or_else(|| panic!("unknown size {name}"))
}

fn map_from(name: &str) -> BarkMap {
    *BarkMap::ALL
        .iter()
        .find(|m| m.as_str() == name)
        .unwrap_or_else(|| panic!("unknown map {name}"))
}

fn leaf_map_from(name: &str) -> LeafMap {
    *LeafMap::ALL
        .iter()
        .find(|m| m.as_str() == name)
        .unwrap_or_else(|| panic!("unknown leaf map {name}"))
}

/// Returns the image types available for a built-in leaf kind. Colour and clump
/// images are always expected; normal maps depend on [`leaf_has_normal_map`].
fn leaf_maps(kind: &str) -> Vec<LeafMap> {
    LeafMap::ALL
        .into_iter()
        .filter(|&map| map != LeafMap::Normal || leaf_has_normal_map(kind))
        .collect()
}

fn compression_from(name: &str) -> TextureCompression {
    *TextureCompression::ALL
        .iter()
        .find(|c| c.as_str() == name)
        .unwrap_or_else(|| panic!("unknown format {name}"))
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
fn texture_kinds_match_the_manifest() {
    let manifest = manifest();
    let ids = |category| -> Vec<&str> {
        manifest_assets(&manifest, category)
            .into_iter()
            .map(|asset| str_field(asset, "id"))
            .collect()
    };
    assert_eq!(ids("bark"), BARK_KINDS);
    assert_eq!(ids("leaves"), LEAF_KINDS);
    let total = manifest["assets"]
        .as_array()
        .expect("manifest assets")
        .len();
    assert_eq!(
        total,
        BARK_KINDS.len() + LEAF_KINDS.len(),
        "unknown categories"
    );
}

#[test]
fn only_the_branch_tip_leaves_have_normal_maps() {
    assert!(!leaf_has_normal_map("oak"));
    assert!(leaf_has_normal_map("white_oak"));
    assert!(!leaf_has_normal_map("not_a_leaf"));
    let with_normals = LEAF_KINDS
        .iter()
        .filter(|kind| leaf_has_normal_map(kind))
        .count();
    assert_eq!(with_normals, 20);
}

#[test]
fn leaf_paths_name_kind_map_and_format() {
    assert_eq!(
        leaf_texture_path("oak", LeafMap::Color, TextureCompression::Bc7),
        "leaves/oak-color-bc7.ktx2"
    );
    assert_eq!(
        leaf_texture_path("white_oak", LeafMap::Normal, TextureCompression::Uastc),
        "leaves/white_oak-normal-uastc.ktx2"
    );
    assert_eq!(
        leaf_texture_path("oak", LeafMap::Clump, TextureCompression::Bc7),
        "leaves/oak-clump-bc7.ktx2"
    );
    assert!(LeafMap::Clump.is_srgb() && !LeafMap::Normal.is_srgb());
}

#[test]
fn every_bark_kind_has_an_average_color() {
    for kind in BARK_KINDS {
        let color = bark_average_color(kind).unwrap_or_else(|| panic!("{kind}"));
        assert!(
            color.iter().all(|c| (0.0..=1.0).contains(c)),
            "{kind}: {color:?}"
        );
    }
    assert_eq!(bark_average_color("not_a_bark"), None);
}

#[test]
fn manifest_lists_every_leaf_file_once() {
    let manifest = manifest();
    let files = manifest_files(&manifest, "leaves");

    // Manifest paths must match the paths callers build for the same kind, map and format.
    for file in &files {
        let path = str_field(file, "path");
        let kind = path
            .strip_prefix("leaves/")
            .and_then(|rest| rest.split('-').next())
            .unwrap_or_else(|| panic!("unexpected path {path}"));
        let built = leaf_texture_path(
            kind,
            leaf_map_from(str_field(file, "map")),
            compression_from(str_field(file, "format")),
        );
        assert_eq!(path, built);
    }

    // Require each supported leaf map in both formats, with no duplicate or extra paths.
    let paths: HashSet<&str> = files.iter().map(|f| str_field(f, "path")).collect();
    assert_eq!(paths.len(), files.len(), "duplicate manifest paths");
    let mut expected = 0;
    for kind in LEAF_KINDS {
        for map in leaf_maps(kind) {
            for compression in TextureCompression::ALL {
                let path = leaf_texture_path(kind, map, compression);
                assert!(
                    paths.contains(path.as_str()),
                    "{path} missing from manifest"
                );
                expected += 1;
            }
        }
    }
    assert_eq!(files.len(), expected);
}

#[test]
fn manifest_lists_every_bark_file_once() {
    let manifest = manifest();
    let files = manifest_files(&manifest, "bark");

    // Each entry's path must be the one built from its own size, map and format.
    for file in &files {
        let path = str_field(file, "path");
        let kind = path
            .strip_prefix("bark/")
            .and_then(|rest| rest.split('-').next())
            .unwrap_or_else(|| panic!("unexpected path {path}"));
        let built = bark_texture_path(
            kind,
            size_from(str_field(file, "size")),
            map_from(str_field(file, "map")),
            compression_from(str_field(file, "format")),
        );
        assert_eq!(path, built);
    }

    // Every combination appears exactly once.
    let paths: HashSet<&str> = files.iter().map(|f| str_field(f, "path")).collect();
    assert_eq!(paths.len(), files.len(), "duplicate manifest paths");
    let mut expected = 0;
    for kind in BARK_KINDS {
        for size in TextureSize::ALL {
            for map in BarkMap::ALL {
                for compression in TextureCompression::ALL {
                    let path = bark_texture_path(kind, size, map, compression);
                    assert!(
                        paths.contains(path.as_str()),
                        "{path} missing from manifest"
                    );
                    expected += 1;
                }
            }
        }
    }
    assert_eq!(files.len(), expected);
}

/// Checks each leaf file listed in the manifest for the expected compression,
/// colour space, dimensions, and complete mipmaps. Requires downloaded colour
/// and available normal maps in both BC7 and UASTC formats.
#[test]
#[ignore = "needs leaf textures from tools/textures/download.sh"]
fn downloaded_leaf_textures_have_the_documented_formats() {
    let manifest = manifest();
    let files = manifest_files(&manifest, "leaves");
    for file in &files {
        let path = str_field(file, "path");
        let map = leaf_map_from(str_field(file, "map"));
        let (width, height) = check_texture(
            compression_from(str_field(file, "format")),
            path,
            map.is_srgb(),
        );
        assert_eq!(
            (Some(width as u64), Some(height as u64)),
            (file["width"].as_u64(), file["height"].as_u64()),
            "{path} size"
        );
    }
    assert!(files.len() >= LEAF_KINDS.len() * 2, "too few files checked");
}

/// Checks the downloaded bark files against the manifest. The 512 and 1k sizes
/// that `tools/textures/download.sh` fetches by default must be present; 2k files
/// are checked when present.
#[test]
#[ignore = "needs bark textures from tools/textures/download.sh"]
fn downloaded_bark_textures_have_the_documented_formats() {
    let manifest = manifest();
    let mut checked = 0;
    for file in manifest_files(&manifest, "bark") {
        let path = str_field(file, "path");
        let size = size_from(str_field(file, "size"));
        if size == TextureSize::Size2k && !texture_dir().join(path).exists() {
            continue;
        }
        let map = map_from(str_field(file, "map"));
        let (width, height) = check_texture(
            compression_from(str_field(file, "format")),
            path,
            map.is_srgb(),
        );
        assert_eq!(
            (Some(width as u64), Some(height as u64)),
            (file["width"].as_u64(), file["height"].as_u64()),
            "{path} size"
        );
        checked += 1;
    }
    assert!(
        checked >= BARK_KINDS.len() * 2 * 3 * 2,
        "too few files checked"
    );
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

    options.trellis.color = 0x00_ff_00;
    let trellis = TrellisMaterial::new(&options.trellis);
    assert_eq!(trellis.base_color, [0.0, 1.0, 0.0]);
    assert_eq!((trellis.metallic, trellis.roughness), (0.0, 0.8));
}
