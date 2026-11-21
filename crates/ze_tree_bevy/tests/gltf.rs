//! Checks billboard material conversion, fallback removal and rotation bounds
//! when Bevy loads exported ze_tree scenes. Also checks that missing wind data
//! or attribute registration leaves the original materials and fallback shapes.
#![cfg(feature = "gltf")]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::gltf::{GltfAssetLabel, GltfPlugin};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::WgpuSettings;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::WorldAssetRoot;
use ze_tree::export::gltf::{GltfOptions, to_glb};
use ze_tree::{Tree, TreePreset, default_lod_levels};
use ze_tree_bevy::{
    ATTRIBUTE_BILLBOARD_AXIS, ATTRIBUTE_BILLBOARD_PIVOT, TreeMaterial, ZeTreePlugin,
    add_tree_gltf_attributes,
};

/// Exports the Apple preset at the lowest default detail level, with wind data
/// controlled by `wind`. Writes `<name>.glb` in the test’s output folder and
/// returns that folder, reusing it if it already exists.
fn export(name: &str, wind: bool) -> PathBuf {
    let tree = Tree::from_preset(TreePreset::Apple);
    let mesh = tree.mesh_with_detail(&default_lod_levels()[3].detail);
    let options = GltfOptions {
        wind,
        ..GltfOptions::default()
    };
    let bytes = to_glb(std::slice::from_ref(&mesh), tree.options(), &options).unwrap();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("gltf_{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.glb")), bytes).unwrap();
    dir
}

/// Builds a test app without a window or GPU backend, loading assets from `dir`.
/// Set `register` to preserve the tree’s custom vertex attributes during loading.
fn app(dir: &Path, register: bool) -> App {
    let gltf = if register {
        add_tree_gltf_attributes(GltfPlugin::default())
    } else {
        GltfPlugin::default()
    };
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(gltf)
            .set(AssetPlugin {
                file_path: dir.to_str().unwrap().to_owned(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                render_creation: WgpuSettings {
                    backends: None,
                    ..default()
                }
                .into(),
                ..default()
            }),
        ZeTreePlugin {
            texture_root: None,
            lod_levels: Vec::new(),
            ..default()
        },
    ));
    app.finish();
    app.cleanup();
    app
}

/// Loads the first scene in `<name>.glb` and waits up to 60 seconds for its
/// `leaves` node, panicking on timeout. Runs five more updates to allow
/// billboard bounds to be applied.
fn spawn_scene(app: &mut App, name: &str) {
    let scene = app
        .world()
        .resource::<AssetServer>()
        .load(GltfAssetLabel::Scene(0).from_asset(format!("{name}.glb")));
    app.world_mut().spawn(WorldAssetRoot(scene));
    let deadline = Instant::now() + Duration::from_secs(60);
    while names(app).iter().all(|n| n != "leaves") {
        assert!(Instant::now() < deadline, "the glTF scene did not spawn");
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    }
    for _ in 0..5 {
        app.update();
    }
}

/// Collects all entity names currently in the app’s world.
fn names(app: &mut App) -> Vec<String> {
    let mut query = app.world_mut().query::<&Name>();
    query
        .iter(app.world())
        .map(|n| n.as_str().to_owned())
        .collect()
}

/// Collects direct children with meshes from every entity named `node`.
fn meshes_under(app: &mut App, node: &str) -> Vec<Entity> {
    let mut query = app.world_mut().query::<(&Name, &Children)>();
    query
        .iter(app.world())
        .filter(|(name, _)| name.as_str() == node)
        .flat_map(|(_, children)| children.iter())
        .filter(|&child| app.world().get::<Mesh3d>(child).is_some())
        .collect()
}

/// Calculates billboard vertex positions for eight camera directions around
/// the mesh, slightly above the horizontal plane. Panics if either billboard
/// attribute is missing or is not a four-component float attribute.
fn turned_positions(mesh: &Mesh) -> Vec<Vec3> {
    let values = |attribute| match mesh.attribute(attribute) {
        Some(VertexAttributeValues::Float32x4(values)) => values.clone(),
        other => panic!("unexpected billboard attribute {other:?}"),
    };
    let (pivots, axes) = (
        values(ATTRIBUTE_BILLBOARD_PIVOT),
        values(ATTRIBUTE_BILLBOARD_AXIS),
    );
    let mut out = Vec::new();
    for k in 0..8 {
        let angle = std::f32::consts::TAU * k as f32 / 8.0;
        let to_camera = Vec3::new(angle.cos(), 0.3, angle.sin()).normalize();
        for (p, a) in pivots.iter().zip(&axes) {
            let (pivot, axis) = (Vec3::new(p[0], p[1], p[2]), Vec3::new(a[0], a[1], a[2]));
            let (right, up) = if axis == Vec3::ZERO {
                let right = Vec3::Y.cross(to_camera).normalize();
                (right, to_camera.cross(right))
            } else {
                (axis.cross(to_camera).normalize(), axis)
            };
            out.push(pivot + right * p[3] + up * a[3]);
        }
    }
    out
}

#[test]
fn billboard_nodes_turn_and_their_partners_are_removed() {
    let dir = export("registered", true);
    let mut app = app(&dir, true);
    spawn_scene(&mut app, "registered");

    let names = names(&mut app);
    assert!(
        names.iter().all(|n| !n.ends_with("_billboard_cull")),
        "{names:?}"
    );
    for node in ["trunk", "branches", "leaves", "fruit"] {
        let meshes = meshes_under(&mut app, node);
        assert!(!meshes.is_empty(), "{node}");
        for entity in meshes {
            let entity = app.world().entity(entity);
            assert!(entity.contains::<MeshMaterial3d<TreeMaterial>>(), "{node}");
            assert!(
                !entity.contains::<MeshMaterial3d<StandardMaterial>>(),
                "{node}"
            );
            assert!(entity.contains::<NoAutoAabb>(), "{node}");

            // Check that the bounds contain vertices for all eight sampled directions.
            let mesh = app
                .world()
                .resource::<Assets<Mesh>>()
                .get(&entity.get::<Mesh3d>().unwrap().0)
                .unwrap();
            let bounds = entity.get::<Aabb>().unwrap();
            let (min, max) = (Vec3::from(bounds.min()), Vec3::from(bounds.max()));
            for p in turned_positions(mesh) {
                assert!(
                    p.cmpge(min - 1e-3).all() && p.cmple(max + 1e-3).all(),
                    "{node}: {p}"
                );
            }
        }
    }
}

/// Checks that missing wind data or attribute registration preserves standard
/// leaf materials and the branches’ crossed fallback node.
#[test]
fn unloadable_billboards_are_left_as_stored() {
    for (name, register, wind) in [("unregistered", false, true), ("no_wind", true, false)] {
        let dir = export(name, wind);
        let mut app = app(&dir, register);
        spawn_scene(&mut app, name);

        let names = names(&mut app);
        assert!(
            names.iter().any(|n| n == "branches_billboard_cull"),
            "{name}"
        );
        for entity in meshes_under(&mut app, "leaves") {
            let entity = app.world().entity(entity);
            assert!(
                entity.contains::<MeshMaterial3d<StandardMaterial>>(),
                "{name}"
            );
            assert!(!entity.contains::<MeshMaterial3d<TreeMaterial>>(), "{name}");
        }
    }
}
