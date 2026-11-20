//! Check that `TextureRef::image_path` names exactly the export images listed in
//! `assets/textures/images/images_manifest.json`, for every bark and leaf kind.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use ze_tree::export::{TextureMap, TextureRef};
use ze_tree::{BARK_KINDS, LEAF_KINDS, TextureSize, leaf_has_normal_map};

const BARK_MAPS: [TextureMap; 3] = [
    TextureMap::BarkColor,
    TextureMap::BarkNormal,
    TextureMap::BarkRoughness,
];

/// Every file path in the export image manifest.
fn manifest_paths() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/textures/images/images_manifest.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let manifest: Value = serde_json::from_str(&text).expect("manifest is valid JSON");
    manifest["assets"]
        .as_array()
        .expect("manifest assets")
        .iter()
        .flat_map(|asset| asset["files"].as_array().expect("asset files"))
        .map(|file| file["path"].as_str().expect("file path").to_owned())
        .collect()
}

#[test]
fn image_paths_are_the_published_images() {
    let mut expected = BTreeSet::new();
    for kind in BARK_KINDS {
        for size in TextureSize::ALL {
            for map in BARK_MAPS {
                expected.insert(TextureRef { kind, map }.image_path(size));
            }
        }
    }
    for kind in LEAF_KINDS {
        let mut maps = vec![TextureMap::LeafColor];
        if leaf_has_normal_map(kind) {
            maps.push(TextureMap::LeafNormal);
        }
        for map in maps {
            for size in TextureSize::ALL {
                expected.insert(TextureRef { kind, map }.image_path(size));
            }
        }
    }
    assert_eq!(manifest_paths(), expected);
}

#[test]
fn image_paths_name_bark_by_size_and_leaves_once() {
    let bark = TextureRef {
        kind: "sakura_bark",
        map: TextureMap::BarkNormal,
    };
    assert_eq!(
        bark.image_path(TextureSize::Size1k),
        "bark/sakura_bark-1k-normal.jpg"
    );
    let leaf = TextureRef {
        kind: "oak",
        map: TextureMap::LeafColor,
    };
    assert_eq!(leaf.image_path(TextureSize::Size512), "leaves/oak-color.png");
    assert_eq!(leaf.image_path(TextureSize::Size2k), "leaves/oak-color.png");
    let normal = TextureRef {
        kind: "white_oak",
        map: TextureMap::LeafNormal,
    };
    assert_eq!(
        normal.image_path(TextureSize::Size1k),
        "leaves/white_oak-normal.png"
    );
}
