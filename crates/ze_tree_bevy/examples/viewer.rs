//! Explore tree presets, wind animation and distance-based mesh detail.
//!
//! Run `cargo run -p ze_tree_bevy --example viewer` from the repository.
//! The viewer uses the repository's `assets/textures` folder with BC7 textures.
//! Run `tools/textures/download.sh` first to fetch the bark and leaf textures.
//!
//! Controls:
//! - Left / Right: choose the previous / next preset.
//! - F: switch between one tree and a 7 × 7 forest to inspect detail transitions.
//! - Space: turn trunk and branch movement on or off; leaves keep fluttering.
//! - N: switch bark normal maps on or off by changing texture coordinates.
//! - J: turn shape variation on or off so trees of the same preset can look different.
//! - R: generate new tree shapes while variation is on.
//! - L: cycle the leaf texture through every kind in `ze_tree::LEAF_KINDS`, starting
//!   from the preset's own leaves. Changing preset returns to the preset's leaves.
//! - [ / ]: make fruit and flowers less / more mature (unripe to ripe fruit,
//!   bud to faded flower). Changing preset returns to the preset's own maturity.
//! - Hold the left mouse button to look around; use WASD and Q / E to fly.
//!
//! Pass options after `--`, for example:
//! `cargo run -p ze_tree_bevy --example viewer -- --preset oak_medium --forest`.
//! Use `--vary` to start with shape variation enabled,
//! `--maturity <0–1>` to set fruit and flower maturity, `--leaves <kind>` to use
//! another leaf texture from `ze_tree::LEAF_KINDS`, `--bark <kind>` to choose a
//! bark texture from `ze_tree::BARK_KINDS`, `--bark-tint <RRGGBB>` to tint the bark
//! with a six-digit hexadecimal RGB colour. The tint multiplies the texture's
//! colour, so it can darken or recolour it but cannot brighten it; `FFFFFF` keeps
//! the texture's own colour. Both bark options stay in effect when changing presets.
//! Use `--still` for leaf-only wind or `--flat` for mirrored bark coordinates
//! without normal maps (this does not enable flat shading). Set camera positions
//! with `--eye x y z` and `--target x y z`. Use `--screenshot <file.png>` to
//! capture a frame after a loading delay and exit. Use `--lod <0–3>` to lock the
//! viewer to one default detail level at all distances. For example, `--lod 3`
//! lets you inspect the distant tree’s camera-facing shapes and shadows up close.

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use ze_tree::{BARK_KINDS, BarkUvMode, LEAF_KINDS, Rng, TreeOptions, TreePreset, WindParams};
use ze_tree_bevy::{TreePart, Wind, ZeTree, ZeTreePlugin, default_tree_lod_levels};

/// Distance between neighbouring trees in the forest, in world units.
const FOREST_SPACING: f32 = 45.0;
/// Number of rows and columns in the square forest.
const FOREST_SIZE: i32 = 7;
/// Vary shape settings by up to 15% either way, then round branch and leaf counts.
const JIGGLE: f32 = 0.15;
/// Change in fruit and flower maturity for each press of [ or ].
const MATURITY_STEP: f32 = 0.1;

#[derive(Resource)]
struct Viewer {
    preset: usize,
    forest: bool,
    sway: bool,
    continuous: bool,
    vary: bool,
    /// Selects a new set of tree variations each time R is pressed.
    roll: i32,
    /// Fruit and flower maturity from 0 to 1; `None` keeps the preset's own values.
    maturity: Option<f32>,
    /// Index into [`LEAF_KINDS`]; `None` keeps the preset's own leaf texture.
    leaves: Option<usize>,
    /// Bark texture to use for all presets, chosen from [`BARK_KINDS`].
    /// `None` uses each preset's own bark texture.
    bark: Option<&'static str>,
    /// Bark tint to use for all presets, as a hexadecimal RGB colour (`0xRRGGBB`).
    /// Multiplies the texture's colour to darken or recolour it; `0xFFFFFF` keeps
    /// the texture's own colour. `None` uses each preset's own tint.
    bark_tint: Option<u32>,
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
        maturity: None,
        leaves: None,
        bark: None,
        bark_tint: None,
        screenshot: None,
    };
    let mut eye = Vec3::new(0.0, 30.0, 110.0);
    let mut target = Vec3::new(0.0, 28.0, 0.0);
    let mut lod_levels = default_tree_lod_levels();

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
            "--maturity" => {
                let maturity = args[i + 1].parse::<f32>().expect("a number");
                viewer.maturity = Some(maturity.clamp(0.0, 1.0));
                i += 1;
            }
            "--leaves" => {
                let kind = &args[i + 1];
                viewer.leaves = Some(
                    LEAF_KINDS
                        .iter()
                        .position(|k| k == kind)
                        .unwrap_or_else(|| panic!("unknown leaf kind {kind}")),
                );
                i += 1;
            }
            "--bark" => {
                let kind = &args[i + 1];
                viewer.bark = Some(
                    BARK_KINDS
                        .into_iter()
                        .find(|k| k == kind)
                        .unwrap_or_else(|| panic!("unknown bark kind {kind}")),
                );
                i += 1;
            }
            "--bark-tint" => {
                let hex = args[i + 1].trim_start_matches('#');
                assert!(hex.len() == 6, "--bark-tint takes a RRGGBB hex colour");
                let tint = u32::from_str_radix(hex, 16).expect("a RRGGBB hex colour");
                viewer.bark_tint = Some(tint);
                i += 1;
            }
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
            "--lod" => {
                let index: usize = args[i + 1].parse().expect("a detail level number");
                let mut level = *lod_levels.get(index).expect("a default detail level");
                level.level.distance = 0.0;
                lod_levels = vec![level];
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
                texture_root: Some("textures".to_owned()),
                lod_levels,
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
        viewer.maturity = None;
        viewer.leaves = None;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        viewer.preset = (viewer.preset + count - 1) % count;
        viewer.maturity = None;
        viewer.leaves = None;
    }

    // Step to the next leaf kind, starting after the preset's own leaves.
    if keys.just_pressed(KeyCode::KeyL) {
        let current = viewer.leaves.unwrap_or_else(|| {
            let kind = TreePreset::ALL[viewer.preset].options().leaves.kind;
            LEAF_KINDS.iter().position(|&k| k == kind).unwrap_or(0)
        });
        viewer.leaves = Some((current + 1) % LEAF_KINDS.len());
    }

    // Step maturity from the preset's fruit maturity the first time it changes.
    let step = match (
        keys.just_pressed(KeyCode::BracketLeft),
        keys.just_pressed(KeyCode::BracketRight),
    ) {
        (true, false) => -MATURITY_STEP,
        (false, true) => MATURITY_STEP,
        _ => 0.0,
    };
    if step != 0.0 {
        let current = viewer
            .maturity
            .unwrap_or_else(|| TreePreset::ALL[viewer.preset].options().fruit.maturity);
        viewer.maturity = Some((current + step).clamp(0.0, 1.0));
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
    options.bark.uv_mode = if viewer.continuous {
        BarkUvMode::Continuous
    } else {
        BarkUvMode::Mirrored
    };
    if let Some(maturity) = viewer.maturity {
        options.fruit.maturity = maturity;
        options.flowers.maturity = maturity;
    }
    if let Some(leaves) = viewer.leaves {
        options.leaves.kind = LEAF_KINDS[leaves].to_owned();
    }
    if let Some(bark) = viewer.bark {
        options.bark.kind = bark.to_owned();
    }
    if let Some(tint) = viewer.bark_tint {
        options.bark.tint = tint;
    }
    info!(
        "{} (leaves {}, maturity {:.1}, bark normal maps {}, sway {}, variation {})",
        preset.name(),
        options.leaves.kind,
        options.fruit.maturity,
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
