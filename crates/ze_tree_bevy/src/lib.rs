//! Generate and display [`ze_tree`] trees in Bevy.
//!
//! Add [`ZeTreePlugin`] after Bevy's default plugins, then spawn a [`ZeTree`]
//! component. Trees generate in the background and appear as child entities for
//! bark, leaves and optional trellis support wires. The plugin creates several
//! levels of detail (LOD): simpler meshes for trees farther from the camera.
//! Bevy fades between these levels. Identical tree options share meshes and materials.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use ze_tree_bevy::{Wind, ZeTree, ZeTreePlugin};
//! use ze_tree::{TreePreset, WindParams};
//!
//! fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
//!     // Spawn a built-in preset and load another tree from the asset directory.
//!     commands.spawn((ZeTree::preset(TreePreset::OakMedium), Transform::default()));
//!     commands.spawn((
//!         ZeTree::Asset(asset_server.load("trees/my_tree.tree.json")),
//!         Transform::from_xyz(40.0, 0.0, 0.0),
//!     ));
//!     // Keep trunks and branches still while leaves flutter.
//!     commands.insert_resource(Wind(WindParams::LEAVES_ONLY));
//! }
//!
//! App::new()
//!     .add_plugins((DefaultPlugins, ZeTreePlugin::default()))
//!     .add_systems(Startup, setup)
//!     .run();
//! ```
//!
//! # Textures
//!
//! Textures are supplied separately. Copy the repository's `assets/textures/bc7`
//! folder to `assets/ze_tree/textures/bc7` in an app using Bevy's default asset
//! directory. For another location, set [`ZeTreePlugin::texture_root`] to the
//! path relative to your asset directory. Missing files cause asset-loading errors;
//! set `texture_root` to `None` to use tint colours without textures. Untextured
//! leaves appear as solid rectangles because no image supplies their outline.
//!
//! The BC7 textures require GPU support for that compression format. The `uastc`
//! set is an alternative for other GPUs and requires Bevy's `basis-universal` feature.
//! Bark normal maps add surface detail only with
//! [`BarkUvMode::Continuous`](ze_tree::BarkUvMode::Continuous). Built-in presets
//! use mirrored texture coordinates, so their bark normal maps are disabled.
//!
//! # Wind
//!
//! Change the [`Wind`] resource to control movement for all tree materials.
//! By default, trunks bend, branches sway and leaves flutter. Animation runs on
//! the GPU using [`MeshBuffers::wind`](ze_tree::MeshBuffers::wind); shadows and
//! motion vectors include this movement. Each tree's position varies the animation
//! timing so copies do not all move together.
//!
//! # Lower-level use
//!
//! To create your own entities, use [`bark_mesh`] and [`leaf_mesh`] with
//! [`bark_material`] and [`leaf_material`], then attach the meshes and
//! `MeshMaterial3d<TreeMaterial>` to those entities. Keep [`ZeTreePlugin`] installed
//! to register the shaders and synchronize materials with the global wind.

mod material;
mod mesh;
mod spawn;
mod wind;

use bevy::asset::embedded_asset;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::shader::{Shader, load_shader_library};
use ze_tree::{LodLevel, WIND_WESL, default_lod_levels};

pub use material::{
    BarkTextures, TreeMaterial, WindExtension, WindUniform, bark_material, leaf_material,
    load_leaf_texture, trellis_material,
};
pub use mesh::{ATTRIBUTE_WIND, bark_mesh, leaf_mesh};
pub use spawn::{TreeOptionsAsset, TreePart, TreePartKind, ZeTree};
pub use wind::Wind;

use spawn::{SpawnSettings, TreeCache, TreeOptionsLoader};

/// Enables tree generation, rendering, wind and `.tree.json` loading.
///
/// Add after Bevy's default plugins, then spawn entities with [`ZeTree`].
#[derive(Debug, Clone)]
pub struct ZeTreePlugin {
    /// Path to a texture set, relative to Bevy's asset directory.
    /// Defaults to `ze_tree/textures/bc7`; copy the repository's
    /// `assets/textures/bc7` folder there. Use `None` for tint colours only.
    pub texture_root: Option<String>,
    /// Mesh detail settings and camera switching distances in world units.
    /// Levels are sorted by distance before generation. Defaults to
    /// [`default_lod_levels`]. Supply one level to disable distance switching;
    /// an empty list produces no bark or leaf meshes.
    pub lod_levels: Vec<LodLevel>,
}

impl Default for ZeTreePlugin {
    fn default() -> Self {
        Self {
            texture_root: Some("ze_tree/textures/bc7".to_owned()),
            lod_levels: default_lod_levels().to_vec(),
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
