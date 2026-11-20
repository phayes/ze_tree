//! # ze_tree
//!
//! Generate procedural trees in Rust, from small bushes to branching oaks and pines.
//! A port of [ez-tree](https://github.com/dgreenheck/ez-tree), `ze_tree` produces
//! branch and leaf meshes, repeatable shapes from a seed, and simpler meshes for
//! distant trees. Your application supplies the renderer.
//!
//! ## Generate a tree
//!
//! Start with a built-in preset:
//!
//! ```rust
//! use ze_tree::{Tree, TreePreset};
//!
//! let tree = Tree::from_preset(TreePreset::OakSmall);
//! let mesh = tree.mesh();
//!
//! println!("{} triangles", tree.triangle_count());
//! println!("{} bark vertices", mesh.branches.positions.len());
//! println!("{} leaf vertices", mesh.leaves.positions.len());
//! ```
//!
//! `mesh.branches` and `mesh.leaves` contain positions, normals, texture coordinates,
//! and triangle indices. Upload these buffers to your renderer and give each mesh
//! its own material. Presets include ash, aspen, oak, pine, bushes, and a tree trained
//! along a trellis. `TreePreset::ALL` lists them all.
//!
//! ## Customize the shape
//!
//! Edit a preset's options, then generate a new tree. The same options and seed
//! produce the same shape; change the seed for another variation.
//!
//! ```rust
//! use ze_tree::{BarkUvMode, Tree, TreePreset};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut options = TreePreset::OakSmall.options();
//!     options.seed = 42;
//!     options.branch.length[0] = 12.0; // Trunk length, in tree units.
//!     options.leaves.size = 1.5;
//!     options.bark.uv_mode = BarkUvMode::Continuous;
//!
//!     let tree = Tree::new(options)?;
//!     println!("{} triangles", tree.triangle_count());
//!     Ok(())
//! }
//! ```
//!
//! Branch settings use index `0` for the trunk and indices `1` through `3` for
//! successive branch levels. You can also start with `TreeOptions::default()`.
//! `Tree::new` checks branch depth and section counts; it does not validate every
//! numeric setting, so use sensible dimensions and counts.
//!
//! ## Load and save JSON
//!
//! Load full ez-tree settings or a partial JSON object. Missing fields, including
//! nested fields, use defaults. JSON support is always enabled.
//!
//! ```rust
//! use ze_tree::Tree;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let tree = Tree::from_json(r#"{
//!         "seed": 42,
//!         "branch": { "length": { "0": 12.0 } }
//!     }"#)?;
//!
//!     let json = tree.options().to_json();
//!     let restored = Tree::from_json(&json)?;
//!     assert_eq!(tree, restored);
//!     Ok(())
//! }
//! ```
//!
//! Use `TreeOptions::from_json` if you only need settings. Unlike this helper,
//! deserializing `TreeOptions` directly with serde requires every field.
//!
//! ## Create simpler meshes
//!
//! Generate several levels of detail (LOD) from the same tree. Your renderer chooses
//! which mesh to show based on camera distance.
//!
//! ```rust
//! use ze_tree::{Tree, TreePreset, default_lod_levels};
//!
//! let tree = Tree::from_preset(TreePreset::AshSmall);
//! for (level, mesh) in tree.lod_meshes(&default_lod_levels()) {
//!     println!(
//!         "From distance {}: {} triangles",
//!         level.distance,
//!         mesh.triangle_count(),
//!     );
//! }
//! ```
//!
//! The defaults provide three levels, starting at distances 0, 100, and 250 in tree
//! units. For your own settings, pass a `LodDetail` to `Tree::mesh_with_detail`.
//! If you only need branch shapes and leaf placements, use `Skeleton::generate`
//! without building a mesh.
//!
//! ## Export a model
//!
//! Optional Cargo features enable file export:
//!
//! - `export-obj`: Wavefront OBJ geometry and MTL materials.
//! - `export-gltf`: glTF (`.gltf` and `.bin`) or a single binary `.glb` file.
//!
//! With `export-gltf` enabled, save a tree as GLB:
//!
//! ```no_run
//! # #[cfg(feature = "export-gltf")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use ze_tree::{Tree, TreePreset};
//! use ze_tree::export::gltf::{GltfOptions, to_glb};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let tree = Tree::from_preset(TreePreset::PineSmall);
//!     let bytes = to_glb(
//!         std::slice::from_ref(tree.mesh()),
//!         tree.options(),
//!         &GltfOptions::default(),
//!     )?;
//!     std::fs::write("pine.glb", bytes)?;
//!     Ok(())
//! }
//! # main()
//! # }
//! # #[cfg(not(feature = "export-gltf"))]
//! # fn main() {}
//! ```
//!
//! Exporters return file contents for you to save. By default, exports use material
//! colours without images; leaves appear as rectangles without a texture supplying
//! their outline. Use `GltfOptions::textures` to reference or embed PNG/JPEG images.
//! An optional rig prepares a model for bending animation, but does not add animation
//! clips. Exported LOD meshes require the receiving tool to choose which to display.
//!
//! To export every preset as OBJ and GLB from this repository:
//!
//! ```sh
//! cargo run -p ze_tree --example export --features export-obj,export-gltf
//! ```
//!
//! Files are written to `target/ze_tree_export` by default. The example accepts an
//! output directory and an optional source texture directory as positional arguments.
//!
//! ## Materials and animation
//!
//! [`BarkMaterial`] and [`LeafMaterial`] describe appearance settings for a renderer.
//! Texture images are not bundled with the crate. In the repository, run
//! `tools/textures/download.sh` to save ready-made KTX2 textures in `assets/textures/`.
//! The script downloads all leaf textures and the 512 and 1k bark sizes by default;
//! add `--sizes 512,1k,2k` to include 2k bark. Each image comes in two formats:
//!
//! - BC7 for GPUs that support it, including most desktop GPUs.
//! - UASTC for loaders that convert Basis Universal textures to a supported GPU format.
//!
//! Set `bark.kind` and `leaves.kind` to names from [`BARK_KINDS`] and [`LEAF_KINDS`].
//! [`bark_texture_path`] and [`leaf_texture_path`] give paths inside the texture folder,
//! such as `bark/sakura_bark-1k-color-bc7.ktx2` or `leaves/white_oak-normal-bc7.ktx2`.
//! Bark has three sizes; leaves have one size per kind. [`leaf_has_normal_map`] tells
//! you whether a leaf kind includes a normal map, which adds lighting detail without
//! extra geometry. `assets/textures/assets_manifest.json` lists the downloadable files.
//!
//! Load colour images as sRGB, and normal and roughness maps as linear data without
//! colour conversion. Normal maps use the OpenGL (+Y) convention. Leaf colour images
//! include straight alpha: transparency is stored separately from the RGB colours.
//! Keep the metallic factor at zero when using a roughness image as a combined
//! metallic-roughness texture.
//!
//! Normal maps need the directions in [`MeshBuffers::tangents`] to orient lighting
//! detail on the surface. Supply the leaf mesh's tangents for leaf normal maps.
//! For bark normal maps, also select [`BarkUvMode::Continuous`].
//!
//! For GPU wind, pass `MeshBuffers::wind` and `WindParams` to the `WIND_WESL` shader
//! module and add its `wind_offset` to vertex positions. For joint-based animation,
//! create joints with `Tree::rig` and vertex weights with `Rig::skin`.
//! `BranchGrouping::parts` and `TreeMesh::part` let you extract separate tree parts.
//! `TrellisOptions::wires` and `TrellisMaterial` describe the optional support wires.
//!
//! ## Bevy integration
//!
//! The workspace also includes `ze_tree_bevy`. Add its `ZeTreePlugin` after Bevy's
//! default plugins and spawn a `ZeTree` component to generate and display trees.
//! It handles background generation, materials, wind, and mesh detail switching.
//! See that crate's documentation for setup and texture paths.

mod cluster;
mod cluster_mesh;
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
mod tree_id;
mod trellis;
mod wind;

pub use cluster::{ClusterItem, ClusterStalk, SkeletonCluster};
pub use material::{
    BARK_KINDS, BarkMap, BarkMaterial, FlowerMaterial, FruitMaterial, LEAF_KINDS, LeafMap,
    LeafMaterial, TextureCompression, TextureSize, TrellisMaterial, bark_texture_path,
    leaf_has_normal_map, leaf_texture_path,
};
pub use mesh::{
    BranchSpan, ClusterSpan, ItemDetail, LeafSpan, LodDetail, LodLevel, MeshBuffers, TreeMesh,
    default_lod_levels,
};
pub use options::{
    BarkOptions, BarkUvMode, Billboard, BranchOptions, ClusterKind, ClusterPlacement, ClusterShape,
    Facing, FlowerOptions, FlowerShape, FruitOptions, FruitShape, GrowthForce, LeafOptions,
    MAX_LEVELS, OptionsError, PerLevel, TreeOptions, TreeType, TrellisForce, TrellisOptions,
};
pub use part::{BranchGrouping, MeshPart, PartMesh};
pub use presets::TreePreset;
pub use rig::{Joint, MAX_RIG_JOINTS, MeshSkin, Rig, RigError, RigOptions, SkinWeights};
pub use rng::Rng;
pub use skeleton::{Attachment, Section, Skeleton, SkeletonBranch, SkeletonLeaf};
pub use tree::{Tree, TreeError};
pub use trellis::{TrellisPull, TrellisWire};
pub use wind::{WIND_WESL, WindParams};
