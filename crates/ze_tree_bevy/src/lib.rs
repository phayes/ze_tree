//! # ze_tree_bevy
//!
//! Display procedural trees in Bevy with background generation, textured materials,
//! wind animation, and distance-based mesh detail. Built on `ze_tree`, this crate
//! targets Bevy 0.20.1.
//!
//! ## Display a tree
//!
//! Add `ZeTreePlugin` after Bevy's default plugins, then spawn a `ZeTree` component.
//! This complete example includes a camera and light and runs without texture files:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use ze_tree::TreePreset;
//! use ze_tree_bevy::{ZeTree, ZeTreePlugin};
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((
//!             DefaultPlugins,
//!             ZeTreePlugin {
//!                 texture_root: None,
//!                 ..default()
//!             },
//!         ))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn(ZeTree::preset(TreePreset::OakSmall));
//!     commands.spawn((
//!         Camera3d::default(),
//!         Transform::from_xyz(35.0, 25.0, 55.0)
//!             .looking_at(Vec3::new(0.0, 10.0, 0.0), Vec3::Y),
//!     ));
//!     commands.spawn((
//!         DirectionalLight {
//!             illuminance: 12_000.0,
//!             shadow_maps_enabled: true,
//!             ..default()
//!         },
//!         Transform::from_xyz(30.0, 50.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
//!     ));
//! }
//! ```
//!
//! The tree appears when background generation finishes. Bark, leaves, and any
//! visible trellis support wires are spawned as child entities. Trees with identical
//! options share meshes and materials.
//!
//! With `texture_root: None`, trees use material colours and leaves are solid
//! rectangles. Add textures to give leaves their outline and bark its surface detail.
//!
//! ## Add textures
//!
//! Bark textures are not stored in the repository. Download them first:
//!
//! ```sh
//! tools/textures/download.sh
//! ```
//!
//! This fetches the 512 and 1k sizes into `assets/textures/bark/`; add
//! `--sizes 512,1k,2k` to include 2k. Then copy the repository's `assets/textures`
//! directory to `assets/ze_tree/textures` in your app and use
//! `ZeTreePlugin::default()`. The default texture root is `ze_tree/textures`,
//! relative to Bevy's asset directory. Set `texture_root` to another path if you
//! store textures elsewhere. Missing textures cause asset-loading errors.
//!
//! Every texture comes in two formats. The default, BC7, requires GPU support for
//! that compression format. For other GPUs, set `texture_compression` to
//! `TextureCompression::Uastc` and enable Bevy's `basis-universal` feature.
//!
//! Bark normal maps add lighting detail without extra geometry. Keep the default
//! `BarkUvMode::Continuous` to use them. With `BarkUvMode::Mirrored`, Bevy omits
//! the bark normal map to avoid reversed lighting detail on every other branch
//! section.
//!
//! ## Customize and position trees
//!
//! Edit a preset's options and wrap them with `ZeTree::new`. Add a `Transform` to
//! position each tree. Register this function as a startup system in your app:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use ze_tree::TreePreset;
//! use ze_tree_bevy::ZeTree;
//!
//! fn plant_trees(mut commands: Commands) {
//!     for i in 0..5 {
//!         let mut options = TreePreset::OakSmall.options();
//!         options.seed = i;
//!         options.branch.length[0] = 12.0; // Trunk length.
//!
//!         commands.spawn((
//!             ZeTree::new(options),
//!             Transform::from_xyz(i as f32 * 20.0, 0.0, 0.0),
//!         ));
//!     }
//! }
//! ```
//!
//! The same options and seed produce the same shape. Changing a tree's `ZeTree`
//! component rebuilds its generated parts; removing the component removes those
//! children. Moving the parent entity moves the whole tree.
//!
//! ## Load settings from JSON
//!
//! Save full or partial ez-tree settings to `assets/trees/oak.tree.json`. Missing
//! fields use defaults. For example:
//!
//! ```json
//! {
//!   "seed": 42,
//!   "branch": { "length": { "0": 12.0 } }
//! }
//! ```
//!
//! Load the file through Bevy's asset server in a startup system:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use ze_tree_bevy::ZeTree;
//!
//! fn load_tree(mut commands: Commands, asset_server: Res<AssetServer>) {
//!     commands.spawn((
//!         ZeTree::Asset(asset_server.load("trees/oak.tree.json")),
//!         Transform::from_xyz(20.0, 0.0, 0.0),
//!     ));
//! }
//! ```
//!
//! The filename must end in `.tree.json`. The tree waits for the asset to load,
//! then generates in the background. It rebuilds when Bevy reports that the asset
//! has changed; detecting live file edits requires Bevy asset watching to be enabled.
//!
//! ## Control wind
//!
//! By default, trunks bend, branches sway, and leaves flutter. Change the global
//! `Wind` resource to update all tree materials. For example, register this startup
//! system to keep trunks and branches still while leaves flutter:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use ze_tree::WindParams;
//! use ze_tree_bevy::Wind;
//!
//! fn set_wind(mut wind: ResMut<Wind>) {
//!     wind.0 = WindParams::LEAVES_ONLY;
//! }
//! ```
//!
//! Wind animation runs on the GPU and is included in shadows and motion vectors.
//! Each tree's position varies the timing so copies do not all move together.
//! The plugin synchronizes every `TreeMaterial` with the global resource, including
//! materials you create yourself.
//!
//! ## Choose mesh detail
//!
//! The plugin generates four mesh detail levels. Bevy fades to simpler meshes at
//! camera distances of 100, 250 and 500 world units. With a texture folder configured,
//! the nearest level uses 1k bark textures and the next two use 512 textures.
//! At the farthest level, branches become two flat strips crossing at right angles,
//! with a single colour approximating the bark texture. Thin branches are omitted.
//! Leaves use fewer, larger flat cards, each showing several leaves in a clump image.
//! Set `ZeTreePlugin::lod_levels` when adding the plugin to choose mesh detail,
//! switching distances, bark appearance and leaf images. For a single full-detail
//! mesh with 2k bark textures and no distance switching:
//!
//! ```rust
//! use ze_tree::{LodLevel, TextureSize};
//! use ze_tree_bevy::{BarkTexture, LeafTexture, TreeLodLevel, ZeTreePlugin};
//!
//! let plugin = ZeTreePlugin {
//!     lod_levels: vec![TreeLodLevel {
//!         level: LodLevel::default(),
//!         bark: BarkTexture::Textured(TextureSize::Size2k),
//!         leaves: LeafTexture::Leaf,
//!     }],
//!     ..Default::default()
//! };
//! ```
//!
//! The 2k bark textures are downloaded only with `--sizes 512,1k,2k`.
//!
//! An empty list produces no bark or leaf meshes. Generated children have a
//! `TreePart` component identifying their detail level and whether they draw bark,
//! leaves, or a trellis wire.
//!
//! ## Try the viewer
//!
//! From the repository, run:
//!
//! ```sh
//! cargo run -p ze_tree_bevy --example viewer
//! ```
//!
//! The viewer uses the repository's textures; run `tools/textures/download.sh`
//! first to fetch the bark textures. Press Left/Right to switch presets,
//! F to toggle a forest, Space to toggle trunk and branch movement, and N to toggle
//! bark normal maps. Hold the left mouse button to look around and use WASD and Q/E
//! to fly. Press J to toggle shape variation and R to generate new variations.
//!
//! ## Build your own entities
//!
//! For direct control, convert a `ze_tree::TreeMesh` with `bark_mesh` and `leaf_mesh`,
//! create materials with `bark_material` and `leaf_material`, and attach the mesh
//! handles and `MeshMaterial3d<TreeMaterial>` to your entities. Keep `ZeTreePlugin`
//! installed to register shaders and synchronize wind settings.

mod material;
mod material_key;
mod mesh;
mod spawn;
mod wind;

use bevy::asset::embedded_asset;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::shader::{Shader, load_shader_library};
use ze_tree::{TextureCompression, WIND_WESL};

pub use material::{
    BarkTextures, LeafTextures, TreeMaterial, WindExtension, WindUniform, average_bark_material,
    bark_material, flower_material, fruit_material, leaf_material, trellis_material,
};
pub use mesh::{ATTRIBUTE_WIND, bark_mesh, cluster_mesh, leaf_mesh};
pub use spawn::{
    BarkTexture, LeafTexture, TreeFinished, TreeGenerating, TreeLodLevel, TreeOptionsAsset,
    TreePart, TreePartKind, ZeTree, default_tree_lod_levels,
};
pub use wind::Wind;

use spawn::{SpawnSettings, TreeCache, TreeOptionsLoader};

/// Enables tree generation, rendering, wind and `.tree.json` loading.
///
/// Add after Bevy's default plugins, then spawn entities with [`ZeTree`].
#[derive(Debug, Clone)]
pub struct ZeTreePlugin {
    /// Folder holding the `bark/` and `leaves/` texture folders, relative to Bevy's
    /// asset directory. Defaults to `ze_tree/textures`; copy the repository's
    /// `assets/textures` folder there after downloading the bark textures.
    /// Use `None` for tint colours only.
    pub texture_root: Option<String>,
    /// Which texture files to load. Defaults to BC7; UASTC needs Bevy's
    /// `basis-universal` feature.
    pub texture_compression: TextureCompression,
    /// Mesh detail, switching distance in world units, and bark and leaf appearance
    /// for each level. Defaults to [`default_tree_lod_levels`]. Levels are sorted
    /// by distance before generation. Supply one level to disable distance
    /// switching, or an empty list to produce no tree meshes.
    pub lod_levels: Vec<TreeLodLevel>,
}

impl Default for ZeTreePlugin {
    fn default() -> Self {
        Self {
            texture_root: Some("ze_tree/textures".to_owned()),
            texture_compression: TextureCompression::Bc7,
            lod_levels: default_tree_lod_levels(),
        }
    }
}

/// Retains the core wind shader so tree shaders can continue importing it.
#[derive(Resource)]
struct WindShader(#[allow(dead_code)] Handle<Shader>);

impl Plugin for ZeTreePlugin {
    fn build(&self, app: &mut App) {
        // Register the shared wind module under the path imported by the tree shaders.
        let wind_shader = app
            .world_mut()
            .resource_mut::<Assets<Shader>>()
            .add(Shader::from_wesl(WIND_WESL, "embedded://ze_tree/wind.wesl"));
        app.insert_resource(WindShader(wind_shader));
        load_shader_library!(app, "shaders/tree_wind.wesl");
        embedded_asset!(app, "shaders/tree.wesl");
        embedded_asset!(app, "shaders/tree_prepass.wesl");

        app.add_plugins(MaterialPlugin::<TreeMaterial>::default())
            .init_asset::<TreeOptionsAsset>()
            .register_asset_loader(TreeOptionsLoader)
            .init_resource::<Wind>()
            .init_resource::<TreeCache>()
            .insert_resource(SpawnSettings {
                texture_root: self.texture_root.clone(),
                texture_compression: self.texture_compression,
                lod_levels: self.lod_levels.clone(),
            })
            .add_systems(
                Update,
                (
                    spawn::reload_modified_options,
                    spawn::reset_changed_trees,
                    spawn::start_generation,
                    spawn::finish_generation,
                    spawn::spawn_parts,
                    spawn::prune_cache,
                    wind::sync_wind,
                )
                    .chain(),
            );
    }
}
