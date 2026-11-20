//! Export trees for use in other 3D applications.
//!
//! Enable the Cargo feature for the format the receiving application supports:
//!
//! - `export-obj` provides an OBJ model and a companion MTL file describing its
//!   colours and textures. It exports one version of the tree without animation joints.
//! - `export-gltf` supports materials and a rig: joints that can be animated in
//!   another tool. Choose a `.gltf` file with a separate `.bin` data file, or a
//!   single `.glb` file.
//!
//! Both exporters return file contents for the caller to save. Trunk, branches,
//! leaves, fruit and flowers are separate parts; [`BranchGrouping`] controls how
//! branches are grouped. Bark texture coordinates already include how often the
//! bark image repeats, so no extra scaling is needed. Fruit and flower
//! colours are stored on the model itself and do not require images.
//!
//! Bark and leaf images are supplied separately. Provide a callback that returns
//! an image path for each [`TextureRef`], or image contents to embed in glTF.
//! [`TextureRef::image_path`] gives paths relative to
//! `https://storage.googleapis.com/ze_textures/tree/images/`. The repository's
//! `assets/textures/images/images_manifest.json` lists the available files and
//! checksums for verifying their contents.

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

/// Lists the bark and leaf parts using the chosen branch grouping, followed
/// by fruit and flower parts only when they contain geometry.
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
