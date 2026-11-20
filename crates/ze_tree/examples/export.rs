//! Save every built-in tree preset as OBJ and GLB files to open in a 3D tool.
//!
//! ```text
//! cargo run -p ze_tree --example export --features export-obj,export-gltf -- [OUT_DIR] [TEXTURE_DIR]
//! ```
//!
//! `OUT_DIR` defaults to the workspace's `target/ze_tree_export`. Images are
//! optional. If supplied, `TEXTURE_DIR` must contain these ez-tree texture paths:
//!
//! - `bark/<kind>_1K-JPG/<kind>_1K-JPG_{Color,NormalGL,Roughness}.jpg`
//! - `leaves/<kind>.png`
//!
//! If no texture directory is supplied, the example looks in the workspace's
//! `prior_art/ez-tree/src/app/public/textures`. Missing or unreadable images are
//! omitted, leaving the corresponding material colours or surface settings.
//!
//! Each preset produces:
//!
//! - `<preset>.obj` and `<preset>.mtl`: full-detail geometry and material text.
//!   Available images are copied to `OUT_DIR/textures/` and referenced by relative path.
//! - `<preset>.glb`: three mesh detail levels, a rig of at most 64 joints, and any
//!   available images embedded in the file. No animation clips are included.
//!
//! A viewer may show all three GLB detail levels at once; select one to display,
//! or configure distance-based switching in the receiving tool.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use ze_tree::export::gltf::{EmbeddedImage, GltfOptions, GltfTextures, ImageFormat, to_glb};
use ze_tree::export::obj::{ObjOptions, to_obj};
use ze_tree::export::{TextureMap, TextureRef};
use ze_tree::{RigOptions, Tree, TreePreset, default_lod_levels};

fn main() -> Result<(), Box<dyn Error>> {
    // Use command-line folders when supplied, otherwise the workspace defaults.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let mut args = std::env::args().skip(1);
    let out_dir = args
        .next()
        .map_or_else(|| workspace.join("target/ze_tree_export"), PathBuf::from);
    let texture_dir = args
        .next()
        .map(PathBuf::from)
        .or_else(|| Some(workspace.join("prior_art/ez-tree/src/app/public/textures")))
        .filter(|dir| dir.is_dir());
    fs::create_dir_all(&out_dir)?;
    match &texture_dir {
        Some(dir) => println!("textures from {}", dir.display()),
        None => println!("no texture folder found; exporting colours only"),
    }

    // Resolve the requested texture kind and role to an existing image file.
    let texture_path = |t: TextureRef<'_>| -> Option<PathBuf> {
        let dir = texture_dir.as_ref()?;
        let kind = t.kind;
        let path = match t.map {
            TextureMap::BarkColor => {
                dir.join(format!("bark/{kind}_1K-JPG/{kind}_1K-JPG_Color.jpg"))
            }
            TextureMap::BarkNormal => {
                dir.join(format!("bark/{kind}_1K-JPG/{kind}_1K-JPG_NormalGL.jpg"))
            }
            TextureMap::BarkRoughness => {
                dir.join(format!("bark/{kind}_1K-JPG/{kind}_1K-JPG_Roughness.jpg"))
            }
            TextureMap::Leaf => dir.join(format!("leaves/{kind}.png")),
        };
        path.is_file().then_some(path)
    };
    // Copy images beside the export so relative material paths remain usable
    // when the output folder is moved.
    let obj_path = |t: TextureRef<'_>| {
        let source = texture_path(t)?;
        let name = source.file_name()?.to_str()?.to_owned();
        let copy = out_dir.join("textures").join(&name);
        if !copy.is_file() {
            fs::create_dir_all(copy.parent()?).ok()?;
            fs::copy(&source, &copy).ok()?;
        }
        Some(format!("textures/{name}"))
    };
    let embed = |t: TextureRef<'_>| {
        let path = texture_path(t)?;
        let format = match path.extension()?.to_str()? {
            "png" => ImageFormat::Png,
            _ => ImageFormat::Jpeg,
        };
        Some(EmbeddedImage {
            bytes: fs::read(path).ok()?,
            format,
        })
    };

    for preset in TreePreset::ALL {
        let tree = Tree::from_preset(preset);
        let stem = preset.name().to_lowercase().replace(' ', "_");

        // Save full-detail geometry with relative paths to the copied images.
        let mtl_name = format!("{stem}.mtl");
        let obj = to_obj(
            tree.mesh(),
            tree.options(),
            &ObjOptions {
                material_library: &mtl_name,
                textures: Some(&obj_path),
                ..ObjOptions::default()
            },
        );
        fs::write(out_dir.join(format!("{stem}.obj")), obj.obj)?;
        fs::write(out_dir.join(&mtl_name), obj.mtl)?;

        // Include the default detail variants, shared joints, and available image bytes in GLB.
        let rig = tree.rig(&RigOptions::default())?;
        let meshes: Vec<_> = tree
            .lod_meshes(&default_lod_levels())
            .into_iter()
            .map(|(_, mesh)| mesh)
            .collect();
        let glb = to_glb(
            &meshes,
            tree.options(),
            &GltfOptions {
                rig: Some(&rig),
                textures: GltfTextures::Embed(&embed),
                ..GltfOptions::default()
            },
        )?;
        fs::write(out_dir.join(format!("{stem}.glb")), &glb)?;

        println!(
            "{:14} {:>7} triangles, {:>2} joints, {:>5.1} MB glb",
            preset.name(),
            tree.triangle_count(),
            rig.joint_count(),
            glb.len() as f64 / 1e6
        );
    }
    println!("wrote {}", out_dir.display());
    Ok(())
}
