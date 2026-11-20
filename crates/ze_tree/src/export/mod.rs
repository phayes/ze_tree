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
//! and leaves; [`BranchGrouping`](crate::BranchGrouping) controls branch grouping.
//! Bark texture repetition is included in the exported texture coordinates.
//!
//! Texture images are not bundled with the crate. Supply a callback that maps each
//! [`TextureRef`] to an image path or, for glTF, image bytes to embed.

#[cfg(feature = "export-gltf")]
pub mod gltf;
#[cfg(feature = "export-obj")]
pub mod obj;

use crate::{BarkMaterial, TreeOptions};

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
    /// Leaf colour in sRGB, with an alpha channel marking transparent areas
    /// outside the leaf shape.
    Leaf,
}

/// Applies the bark material's texture repetition to the supplied coordinates.
fn scaled_bark_uvs(uvs: &[[f32; 2]], options: &TreeOptions) -> Vec<[f32; 2]> {
    let [su, sv] = BarkMaterial::new(&options.bark).uv_scale;
    uvs.iter().map(|&[u, v]| [u * su, v * sv]).collect()
}
