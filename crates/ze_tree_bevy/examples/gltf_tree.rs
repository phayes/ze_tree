//! View an exported ze_tree glTF scene with wind and camera-facing billboards,
//! the flat shapes used to represent distant tree parts.
//!
//! Run `cargo run -p ze_tree_bevy --example gltf_tree --features gltf` to export
//! the Apple preset at its lowest default detail level and view the result.
//! The file is saved to `target/ze_tree_gltf/apple_far.glb` in the workspace.
//! Append `-- path/to/tree.glb` to view an existing file instead. Add
//! `--screenshot output.png` after the `--` to capture a frame after five
//! seconds and exit; this delay does not guarantee that loading has finished.
//! Hold the left mouse button to look around, and use WASD and Q/E to fly.

use std::path::PathBuf;

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::gltf::{GltfAssetLabel, GltfPlugin};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::world_serialization::WorldAssetRoot;
use ze_tree::export::gltf::{GltfOptions, to_glb};
use ze_tree::{Tree, TreePreset, default_lod_levels};
use ze_tree_bevy::{ZeTreePlugin, add_tree_gltf_attributes};

/// Scene file name relative to the asset folder, plus an optional screenshot path.
#[derive(Resource)]
struct Settings {
    file: String,
    screenshot: Option<String>,
}

fn main() {
    let mut file = None;
    let mut screenshot = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshot" => screenshot = args.next(),
            path => file = Some(PathBuf::from(path)),
        }
    }
    let file = file.unwrap_or_else(export_apple);
    let file = std::path::absolute(file).expect("a usable file path");
    let folder = file.parent().expect("a file in a folder").to_owned();
    let name = file
        .file_name()
        .expect("a file name")
        .to_string_lossy()
        .into_owned();

    App::new()
        .add_plugins((
            DefaultPlugins
                .set(add_tree_gltf_attributes(GltfPlugin::default()))
                .set(AssetPlugin {
                    file_path: folder.to_string_lossy().into_owned(),
                    ..default()
                }),
            // The scene supplies its own meshes and materials; disable generated
            // detail levels and external tree textures.
            ZeTreePlugin {
                texture_root: None,
                lod_levels: Vec::new(),
                ..default()
            },
            FreeCameraPlugin,
        ))
        .insert_resource(Settings {
            file: name,
            screenshot,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, take_screenshot)
        .run();
}

/// Saves an Apple tree at the lowest default detail level and returns its GLB path.
/// Panics if exporting or writing the file fails.
fn export_apple() -> PathBuf {
    let tree = Tree::from_preset(TreePreset::Apple);
    let mesh = tree.mesh_with_detail(&default_lod_levels()[3].detail);
    let bytes = to_glb(
        std::slice::from_ref(&mesh),
        tree.options(),
        &GltfOptions::default(),
    )
    .expect("the tree exports");
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ze_tree_gltf");
    std::fs::create_dir_all(&folder).expect("the export folder can be created");
    let path = folder.join("apple_far.glb");
    std::fs::write(&path, bytes).expect("the export can be saved");
    path
}

fn setup(
    mut commands: Commands,
    settings: Res<Settings>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scene = GltfAssetLabel::Scene(0).from_asset(settings.file.clone());
    commands.spawn(WorldAssetRoot(asset_server.load(scene)));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(20.0, 22.0, 45.0).looking_at(Vec3::new(0.0, 10.0, 0.0), Vec3::Y),
        FreeCamera {
            walk_speed: 20.0,
            run_speed: 80.0,
            ..default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(40.0, 80.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 400.0,
        ..default()
    });
    commands.insert_resource(ClearColor(Color::srgb(0.62, 0.76, 0.9)));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.32, 0.42, 0.22))),
    ));
}

/// If a screenshot path was supplied, requests a capture after five seconds
/// and exits after the captured image is passed to the save handler.
fn take_screenshot(
    mut commands: Commands,
    settings: Res<Settings>,
    time: Res<Time>,
    mut taken: Local<bool>,
) {
    let Some(path) = &settings.screenshot else {
        return;
    };
    if !*taken && time.elapsed_secs() > 5.0 {
        *taken = true;
        let mut save = save_to_disk(path.clone());
        commands.spawn(Screenshot::primary_window()).observe(
            move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                save(captured);
                exit.write(AppExit::Success);
            },
        );
    }
}
