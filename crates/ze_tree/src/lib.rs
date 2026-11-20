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
//! Rendering is up to your application. Upload the bark and leaf buffers from
//! [`TreeMesh`] to your renderer, apply the settings from [`BarkMaterial`] and
//! [`LeafMaterial`], and choose detail levels based on camera distance. Use
//! [`bark_texture_path`] and [`leaf_texture_path`] to locate images under the
//! repository's `assets/textures/` folder. These KTX2 files contain compressed
//! textures for GPU use and must be obtained separately from the published crate.
//! To add bark lighting detail with normal maps, set [`BarkOptions::uv_mode`] to
//! [`BarkUvMode::Continuous`] and supply [`MeshBuffers::tangents`] to your renderer.

mod material;
mod mesh;
mod options;
mod presets;
mod rng;
mod skeleton;
mod tree;
mod trellis;

pub use material::{
    BARK_KINDS, BarkMap, BarkMaterial, LEAF_KINDS, LeafMaterial, TextureSet, bark_texture_path,
    leaf_texture_path,
};
pub use mesh::{
    BranchSpan, LeafSpan, LodDetail, LodLevel, MeshBuffers, TreeMesh, default_lod_levels,
};
pub use options::{
    BarkOptions, BarkUvMode, Billboard, BranchOptions, GrowthForce, LeafOptions, MAX_LEVELS, OptionsError,
    PerLevel, TreeOptions, TreeType, TrellisForce, TrellisOptions,
};
pub use presets::TreePreset;
pub use rng::Rng;
pub use skeleton::{Attachment, Section, Skeleton, SkeletonBranch, SkeletonLeaf};
pub use tree::{Tree, TreeError};
pub use trellis::TrellisPull;
