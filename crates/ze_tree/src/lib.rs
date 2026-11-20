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
//! A skeleton is shape data, not a drawable mesh. Mesh generation and rendering are
//! not yet implemented; see `port_plan.md` at the repository root for planned work.

mod material;
mod mesh;
mod options;
mod presets;
mod rng;
mod skeleton;
mod tree;
mod trellis;

pub use options::{
    BarkOptions, Billboard, BranchOptions, GrowthForce, LeafOptions, MAX_LEVELS, OptionsError,
    PerLevel, TreeOptions, TreeType, TrellisForce, TrellisOptions,
};
pub use presets::TreePreset;
pub use rng::Rng;
pub use skeleton::{Attachment, Section, Skeleton, SkeletonBranch, SkeletonLeaf};
pub use trellis::TrellisPull;
