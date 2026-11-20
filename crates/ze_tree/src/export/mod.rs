//! Export trees as 3D model data for a renderer or modelling tool.
//!
//! Enable the Cargo feature for the desired format:
//!
//! - `export-obj`: the `obj` module produces Wavefront OBJ mesh text and an MTL
//!   file describing colours and textures. It exports one mesh without animation data.
//! - `export-gltf`: the `gltf` module produces glTF 2.0, a scene format supporting
//!   materials and animation rigs. Choose a JSON `.gltf` file plus binary `.bin`
//!   data, or a single binary `.glb` file.
//!
//! Both exporters return data for the caller to save. They separate trunk, branches,
//! leaves, fruit and flowers; [`BranchGrouping`] controls
//! branch grouping. Bark texture repetition is included in the exported texture
//! coordinates. Fruit and flowers are coloured per vertex and need no images.
//!
//! Texture images are not bundled with the crate. Supply a callback that maps each
//! [`TextureRef`] to an image path or, for glTF, image bytes to embed.
//! [`TextureRef::image_path`] names the JPEG and PNG images published for export
//! at `https://storage.googleapis.com/ze_textures/tree/images/`, which the repository's
//! `assets/textures/images/images_manifest.json` lists with their checksums.

#[cfg(feature = "export-gltf")]
pub mod gltf;
#[cfg(feature = "export-obj")]
pub mod obj;

use crate::{BarkMaterial, BranchGrouping, MeshPart, TextureSize, TreeMesh, TreeOptions};

/// A request for a bark or leaf texture image.
///
/// The exporter passes this to the caller's texture callback. Use both `kind`
/// and `map` to choose the image, such as the normal map for `"Bark001"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureRef<'a> {
    /// Texture identifier from [`BarkOptions::kind`](crate::BarkOptions::kind)
    /// or, for leaves, [`LeafOptions::kind`](crate::LeafOptions::kind).
    pub kind: &'a str,
    /// The image's role, such as colour or surface roughness.
    pub map: TextureMap,
}

impl TextureRef<'_> {
    /// Builds an export image path relative to `assets/textures/images/`, such as
    /// `"bark/sakura_bark-1k-color.jpg"` or `"leaves/white_oak-normal.png"`.
    ///
    /// Bark images are JPEGs in the requested [`TextureSize`]. Leaf images are PNGs
    /// in one size per kind, so `size` is ignored for leaves. Use a name from
    /// [`BARK_KINDS`](crate::BARK_KINDS) or [`LEAF_KINDS`](crate::LEAF_KINDS).
    /// This builds a path without checking for a file; leaf normal maps are available
    /// only when [`leaf_has_normal_map`](crate::leaf_has_normal_map) returns true.
    pub fn image_path(&self, size: TextureSize) -> String {
        let map = match self.map {
            TextureMap::BarkColor => "color",
            TextureMap::BarkNormal => "normal",
            TextureMap::BarkRoughness => "roughness",
            TextureMap::LeafColor => return format!("leaves/{}-color.png", self.kind),
            TextureMap::LeafNormal => return format!("leaves/{}-normal.png", self.kind),
        };
        format!("bark/{}-{}-{map}.jpg", self.kind, size.as_str())
    }
}

/// The surface property supplied by a texture image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureMap {
    /// Bark colour image, encoded in sRGB colour space.
    BarkColor,
    /// Bark surface directions for lighting small bumps without extra geometry.
    /// Supply an OpenGL-style normal map as linear data, not sRGB colour.
    BarkNormal,
    /// How matte the bark looks, stored as linear data. glTF uses the green
    /// channel for roughness; a greyscale image supplies the same value in all
    /// colour channels and also works with OBJ importers expecting greyscale.
    BarkRoughness,
    /// Leaf colour in sRGB, with transparency in the alpha channel to cut out
    /// the leaf or branch outline. Supply straight (not premultiplied) alpha.
    LeafColor,
    /// Surface directions that add lighting detail to leaves without extra geometry.
    /// Supply an OpenGL (+Y) normal map as linear data, without sRGB conversion.
    /// Exporters request this only for leaf kinds supported by
    /// [`leaf_has_normal_map`](crate::leaf_has_normal_map).
    LeafNormal,
}

/// The parts both exporters write: `grouping`'s bark and leaf parts, then
/// fruit and flowers when the mesh has them.
fn export_parts(grouping: BranchGrouping, mesh: &TreeMesh) -> Vec<MeshPart> {
    let mut parts = grouping.parts(mesh);
    for (part, buffers) in [
        (MeshPart::Fruit, &mesh.fruit),
        (MeshPart::Flowers, &mesh.flowers),
    ] {
        if !buffers.indices.is_empty() {
            parts.push(part);
        }
    }
    parts
}

/// Applies the bark material's texture repetition to the supplied coordinates.
fn scaled_bark_uvs(uvs: &[[f32; 2]], options: &TreeOptions) -> Vec<[f32; 2]> {
    let [su, sv] = BarkMaterial::new(&options.bark).uv_scale;
    uvs.iter().map(|&[u, v]| [u * su, v * sv]).collect()
}
