//! Explore tree presets, wind animation and distance-based mesh detail.
//!
//! Run `cargo run -p ze_tree_bevy --example viewer` from the repository.
//! The viewer uses the repository's `assets/textures/bc7` texture set.
//!
//! Controls:
//! - Left / Right: choose the previous / next preset.
//! - F: switch between one tree and a 7 × 7 forest to inspect detail transitions.
//! - Space: turn trunk and branch movement on or off; leaves keep fluttering.
//! - N: switch bark normal maps on or off by changing texture coordinates.
//! - J: turn shape variation on or off so trees of the same preset can look different.
//! - R: generate new tree shapes while variation is on.
//! - Hold the left mouse button to look around; use WASD and Q / E to fly.
//!
//! Pass options after `--`, for example:
//! `cargo run -p ze_tree_bevy --example viewer -- --preset oak_medium --forest`.
//! Use `--vary` to start with shape variation enabled,
//! `--still` for leaf-only wind or `--flat` for mirrored bark coordinates
//! without normal maps (this does not enable flat shading). Set camera positions
//! with `--eye x y z` and `--target x y z`. Use `--screenshot <file.png>` to
//! capture a frame after a loading delay and exit.

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use ze_tree::{BarkUvMode, Rng, TreeOptions, TreePreset, WindParams};
use ze_tree_bevy::{TreePart, Wind, ZeTree, ZeTreePlugin};

/// Distance between neighbouring trees in the forest, in world units.
const FOREST_SPACING: f32 = 45.0;
/// Number of rows and columns in the square forest.
const FOREST_SIZE: i32 = 7;
/// Vary shape settings by up to 15% either way, then round branch and leaf counts.
const JIGGLE: f32 = 0.15;

#[derive(Resource)]
struct Viewer {
    preset: usize,
    forest: bool,
    sway: bool,
    continuous: bool,
    vary: bool,
    /// Selects a new set of tree variations each time R is pressed.
    roll: i32,
    screenshot: Option<String>,
}

#[derive(Component)]
struct Shown;

fn main() {
    let mut viewer = Viewer {
        preset: TreePreset::ALL
            .iter()
            .position(|&p| p == TreePreset::OakMedium)
            .unwrap(),
        forest: false,
        sway: true,
        continuous: true,
        vary: false,
        roll: 0,
        screenshot: None,
    };
    let mut eye = Vec3::new(0.0, 30.0, 110.0);
    let mut target = Vec3::new(0.0, 28.0, 0.0);

    // Apply command-line choices before creating the scene.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let vec3 = |args: &[String], i: usize| {
        let n = |k: usize| args[i + k].parse::<f32>().expect("a number");
        Vec3::new(n(1), n(2), n(3))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--preset" => {
                let name = args[i + 1].replace('_', " ");
                viewer.preset = TreePreset::ALL
                    .iter()
                    .position(|p| p.name().eq_ignore_ascii_case(&name))
                    .unwrap_or_else(|| panic!("unknown preset {name}"));
                i += 1;
            }
            "--forest" => viewer.forest = true,
            "--vary" => viewer.vary = true,
            "--still" => viewer.sway = false,
            "--flat" => viewer.continuous = false,
            "--eye" => {
                eye = vec3(&args, i);
                i += 3;
            }
            "--target" => {
                target = vec3(&args, i);
                i += 3;
            }
            "--screenshot" => {
                viewer.screenshot = Some(args[i + 1].clone());
                i += 1;
            }
            other => panic!("unknown option {other}"),
        }
        i += 1;
    }

    App::new()
        .add_plugins((
            DefaultPlugins.set(AssetPlugin {
                file_path: "../../assets".to_owned(),
                ..default()
            }),
            ZeTreePlugin {
                texture_root: Some("textures/bc7".to_owned()),
                ..default()
            },
            FreeCameraPlugin,
        ))
        .insert_resource(viewer)
        .insert_resource(Camera3dSetup { eye, target })
        .add_systems(Startup, setup)
        .add_systems(Update, (controls, show_trees, screenshot))
        .run();
}

#[derive(Resource)]
struct Camera3dSetup {
    eye: Vec3,
    target: Vec3,
}

fn setup(
    mut commands: Commands,
    camera: Res<Camera3dSetup>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera.eye).looking_at(camera.target, Vec3::Y),
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
        Mesh3d(meshes.add(Plane3d::default().mesh().size(800.0, 800.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.32, 0.42, 0.22))),
    ));
}

fn controls(keys: Res<ButtonInput<KeyCode>>, mut viewer: ResMut<Viewer>) {
    let count = TreePreset::ALL.len();
    if keys.just_pressed(KeyCode::ArrowRight) {
        viewer.preset = (viewer.preset + 1) % count;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        viewer.preset = (viewer.preset + count - 1) % count;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        viewer.forest = !viewer.forest;
    }
    if keys.just_pressed(KeyCode::Space) {
        viewer.sway = !viewer.sway;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        viewer.continuous = !viewer.continuous;
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        viewer.vary = !viewer.vary;
    }
    if keys.just_pressed(KeyCode::KeyR) && viewer.vary {
        viewer.roll += 1;
    }
}

/// Rebuilds the displayed trees and applies wind settings when viewer controls change.
fn show_trees(
    mut commands: Commands,
    viewer: Res<Viewer>,
    shown: Query<Entity, With<Shown>>,
    mut wind: ResMut<Wind>,
) {
    if !viewer.is_changed() {
        return;
    }
    wind.0 = if viewer.sway {
        WindParams::default()
    } else {
        WindParams::LEAVES_ONLY
    };
    for entity in &shown {
        commands.entity(entity).despawn();
    }

    let preset = TreePreset::ALL[viewer.preset];
    let mut options: TreeOptions = preset.options();
    if viewer.continuous {
        options.bark.uv_mode = BarkUvMode::Continuous;
    }
    info!(
        "{} (bark normal maps {}, sway {}, variation {})",
        preset.name(),
        if viewer.continuous { "on" } else { "off" },
        if viewer.sway { "on" } else { "off" },
        if viewer.vary { "on" } else { "off" },
    );
    let tree = ZeTree::new(options.clone());
    let half = if viewer.forest { FOREST_SIZE / 2 } else { 0 };
    for x in -half..=half {
        for z in -half..=half {
            let position = Vec3::new(x as f32, 0.0, -z as f32) * FOREST_SPACING;
            // Vary each copy's rotation to make the repeated tree shape less obvious.
            let turn = Quat::from_rotation_y((x * 7 + z * 13) as f32);
            // Give each forest position a repeatable variation; pressing R selects a new set.
            let tree = if viewer.vary {
                let cell = (x + half) * FOREST_SIZE + (z + half);
                let mut rng = Rng::new(viewer.roll.wrapping_mul(1000).wrapping_add(cell));
                let mut varied = options.clone();
                varied.jiggle(JIGGLE, &mut rng);
                ZeTree::new(varied)
            } else {
                tree.clone()
            };
            commands.spawn((
                tree,
                Transform::from_translation(position).with_rotation(turn),
                Shown,
            ));
        }
    }
}

/// With `--screenshot`, requests a capture five seconds after the first tree part
/// appears and exits one second later. The delay allows time for textures and
/// shaders to load, but does not check that loading or saving has completed.
fn screenshot(
    mut commands: Commands,
    viewer: Res<Viewer>,
    time: Res<Time>,
    parts: Query<(), With<TreePart>>,
    mut appeared: Local<Option<f32>>,
    mut taken: Local<bool>,
) {
    let Some(path) = &viewer.screenshot else {
        return;
    };
    let now = time.elapsed_secs();
    if appeared.is_none() && !parts.is_empty() {
        *appeared = Some(now);
    }
    let Some(appeared) = *appeared else {
        return;
    };
    if !*taken && now > appeared + 5.0 {
        *taken = true;
        // Exit only after the image has been written.
        let mut save = save_to_disk(path.clone());
        commands.spawn(Screenshot::primary_window()).observe(
            move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                save(captured);
                exit.write(AppExit::Success);
            },
        );
    }
}
