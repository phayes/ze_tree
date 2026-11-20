//! Tree settings and repeatable random numbers for a Rust port of
//! [ez-tree](https://github.com/dgreenheck/ez-tree), which generates tree shapes
//! from settings such as branch length and leaf count.
//!
//! Start with [`TreeOptions::default()`] and adjust the settings, or use a
//! ready-made preset such as `TreePreset::OakSmall.options()`.
//! [`TreeOptions::from_json`] loads full or partial ez-tree JSON, keeping defaults
//! for omitted fields. [`TreeOptions::to_json`] saves all settings as JSON.
//! JSON and serde support are always available; no Cargo feature is needed.
//! Deserializing [`TreeOptions`] directly with serde requires every field.
//!
//! Pass settings to [`Skeleton::generate`] to get branch shapes and leaf placements.
//! Each branch is a sequence of circular slices describing its position and thickness;
//! each leaf has a position, orientation, and size. The same settings, including the
//! seed, produce the same skeleton. Generation returns an error if
//! [`TreeOptions::validate`] rejects the branch depth or section counts.
//!
//! A skeleton describes the tree's shape. To get triangles for rendering, call
//! [`Skeleton::mesh`] with the tree options and [`LodDetail::default()`] for full detail.
//! A level of detail is a simpler or more detailed version of the same tree mesh:
//! simpler versions cost less to draw at a distance. [`default_lod_levels`] provides
//! ez-tree's three standard settings. You can mesh one skeleton at several detail
//! levels without changing its shape data or making new random choices.
//!
//! [`Tree`] generates both the tree's layout and its full-detail mesh in one call,
//! using custom settings, a preset, or JSON. It gives you access to the settings
//! and generated shape, and can create simpler meshes for distant trees.
//!
//! To render a tree, upload the bark and leaf buffers from [`TreeMesh`] to your
//! renderer and apply the appearance settings from [`BarkMaterial`] and
//! [`LeafMaterial`]. Choose simpler meshes as the tree moves farther from the camera.
//!
//! The repository's `assets/textures/` folder contains KTX2 texture files:
//! compressed images with smaller versions (mipmaps) for rendering at a distance.
//! These files are not included in the published crate. To use them:
//!
//! - Choose `uastc/` for textures that a Basis Universal loader can convert for
//!   any GPU. In Bevy, enable the `basis-universal` feature to load them.
//!   Choose `bc7/` for textures that most desktop GPUs can use directly.
//! - Inside either folder, bark textures are at
//!   `bark/<kind>/{color,normal,roughness}.ktx2`, and leaf textures are at
//!   `leaves/<kind>.ktx2`. Use [`BarkOptions::kind`] from [`BARK_KINDS`] and
//!   [`LeafOptions::kind`] from [`LEAF_KINDS`] to select the textures.
//! - Load bark colour and leaf textures as sRGB so their colours display correctly.
//!   Load normal maps (surface bump directions) and roughness maps (how matte the
//!   surface looks) as linear data so their values are used without colour conversion.
//! - Normal maps use the OpenGL/glTF convention: the green channel points along
//!   increasing `v`, the second texture coordinate.
//! - Roughness is stored in all three colour channels. If your renderer uses this
//!   as a combined metallic-roughness texture, keep the metallic factor at 0 so
//!   the bark does not look metallic.
//! - Leaf alpha controls transparency and is straight, not premultiplied: the
//!   stored colours have not been multiplied by alpha.
//!
//! To light bark with normal maps, set [`BarkOptions::uv_mode`] to
//! [`BarkUvMode::Continuous`] and supply [`MeshBuffers::tangents`] to your renderer.
//!
//! Animate bark and leaves with the [`WIND_WESL`] shader module. Pass
//! [`MeshBuffers::wind`] as per-vertex data and [`WindParams`] as the shared wind
//! settings, then add `wind_offset` to each vertex's position in your shader.
//! To display a trellis (the guide wires that shape growth), use
//! [`TrellisOptions::wires`] for wire endpoints and [`TrellisMaterial`] for appearance.
//!
//! To draw or export trunk, branches, and leaves separately, list nonempty parts
//! with [`BranchGrouping::parts`] and copy each into its own mesh with [`TreeMesh::part`].
//!
//! To prepare a tree for bending animation, use [`Tree::rig`] to create movable
//! joints along its branches. [`Rig::skin`] supplies weights describing how strongly
//! each joint moves each vertex. Apply joint movement in your renderer or 3D tool.
//!
//! To save model files, enable `export-obj` for Wavefront OBJ mesh and material text,
//! or `export-gltf` for glTF scene data, and use the `export` module. These functions
//! return file contents for you to save. glTF can include the rig and simpler mesh
//! variants for distant viewing; the receiving tool controls animation and detail switching.

#[cfg(any(feature = "export-obj", feature = "export-gltf"))]
pub mod export;
mod jiggle;
mod material;
mod mesh;
mod options;
mod part;
mod presets;
mod rig;
mod rng;
mod skeleton;
mod tree;
mod trellis;
mod wind;

pub use material::{BARK_KINDS, BarkMaterial, LEAF_KINDS, LeafMaterial, TrellisMaterial};
pub use mesh::{
    BranchSpan, LeafSpan, LodDetail, LodLevel, MeshBuffers, TreeMesh, default_lod_levels,
};
pub use options::{
    BarkOptions, BarkUvMode, Billboard, BranchOptions, GrowthForce, LeafOptions, MAX_LEVELS, OptionsError,
    PerLevel, TreeOptions, TreeType, TrellisForce, TrellisOptions,
};
pub use part::{BranchGrouping, MeshPart, PartMesh};
pub use presets::TreePreset;
pub use rig::{Joint, MAX_RIG_JOINTS, MeshSkin, Rig, RigError, RigOptions, SkinWeights};
pub use rng::Rng;
pub use skeleton::{Attachment, Section, Skeleton, SkeletonBranch, SkeletonLeaf};
pub use tree::{Tree, TreeError};
pub use trellis::{TrellisPull, TrellisWire};
pub use wind::{WIND_WESL, WindParams};
