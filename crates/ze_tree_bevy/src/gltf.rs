//! Load exported ze_tree billboards with wind animation and camera-facing rotation.
//! Billboards are flat shapes used to represent tree parts at a distance.

use std::collections::{HashMap, HashSet};

use bevy::asset::LoadContext;
use bevy::camera::visibility::NoAutoAabb;
use bevy::gltf::extensions::{
    ErasedGltfExtensionHandler, GltfExtensionHandler, GltfExtensionHandlers,
};
use bevy::gltf::gltf::{self as gltf_data, Semantic};
use bevy::gltf::{GltfAssetLabel, GltfExtras, GltfMaterial, GltfPlugin};
use bevy::mesh::MeshVertexAttribute;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::pbr::gltf::standard_material_from_gltf_material;
use bevy::prelude::*;
use serde_json::Value;

use crate::material::{TreeMaterial, WindExtension};
use crate::mesh::{
    ATTRIBUTE_BILLBOARD_AXIS, ATTRIBUTE_BILLBOARD_PIVOT, ATTRIBUTE_WIND, mesh_billboard_aabb,
};

/// Wind and billboard attributes from `ze_tree::export::gltf`. Bevy looks up
/// these names without the leading underscore used in the file, such as `_WIND`.
const ATTRIBUTES: [(&str, MeshVertexAttribute); 3] = [
    ("WIND", ATTRIBUTE_WIND),
    ("BILLBOARD_PIVOT", ATTRIBUTE_BILLBOARD_PIVOT),
    ("BILLBOARD_AXIS", ATTRIBUTE_BILLBOARD_AXIS),
];

/// Configures the supplied [`GltfPlugin`] to preserve `_WIND`, `_BILLBOARD_PIVOT`
/// and `_BILLBOARD_AXIS` vertex data in exported ze_tree meshes.
///
/// Use the returned plugin in `DefaultPlugins`, then add
/// [`ZeTreePlugin`](crate::ZeTreePlugin) to animate eligible billboards. Export
/// with wind data enabled; registering attributes alone does not animate meshes.
/// This function is available with this crate's `gltf` feature.
///
/// ```no_run
/// use bevy::gltf::GltfPlugin;
/// use bevy::prelude::*;
/// use ze_tree_bevy::{ZeTreePlugin, add_tree_gltf_attributes};
///
/// App::new().add_plugins((
///     DefaultPlugins.set(add_tree_gltf_attributes(GltfPlugin::default())),
///     ZeTreePlugin::default(),
/// ));
/// ```
pub fn add_tree_gltf_attributes(plugin: GltfPlugin) -> GltfPlugin {
    ATTRIBUTES
        .into_iter()
        .fold(plugin, |plugin, (name, attribute)| {
            plugin.add_custom_vertex_attribute(name, attribute)
        })
}

/// Marks a billboard mesh awaiting bounds that cover its camera-facing rotation.
/// Removed after the mesh becomes available, even if no bounds can be calculated.
#[derive(Component, Reflect, Default, Clone, Copy)]
#[reflect(Component, Default, Clone)]
pub(crate) struct PendingBillboardBounds;

/// Registers the billboard bounds component and update system, then adds the
/// glTF handler. Logs a warning and skips the handler if `GltfPlugin` is missing.
pub(crate) fn register(app: &mut App) {
    app.register_type::<PendingBillboardBounds>().add_systems(
        PostUpdate,
        fit_billboard_bounds.before(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
    );
    let Some(handlers) = app.world_mut().get_resource_mut::<GltfExtensionHandlers>() else {
        warn!("ze_tree_bevy: add ZeTreePlugin after GltfPlugin to turn billboards in glTF files");
        return;
    };
    let handler = Box::new(BillboardGltfHandler::default());
    #[cfg(target_family = "wasm")]
    bevy::tasks::block_on(async { handlers.0.write().await.push(handler) });
    #[cfg(not(target_family = "wasm"))]
    handlers.0.write_blocking().push(handler);
}

/// Updates billboard bounds once mesh assets are available, so camera-facing
/// rotation does not make visible shapes disappear at the edge of the view.
/// These bounds cover rotation, but not movement caused by wind.
fn fit_billboard_bounds(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    pending: Query<(Entity, &Mesh3d), With<PendingBillboardBounds>>,
) {
    for (entity, mesh) in &pending {
        let Some(mesh) = meshes.get(&mesh.0) else {
            continue;
        };
        let mut entity = commands.entity(entity);
        entity.remove::<PendingBillboardBounds>();
        if let Some(bounds) = mesh_billboard_aabb(mesh) {
            entity.insert((bounds, NoAutoAabb));
        }
    }
}

/// Tracks the meshes and materials needed to animate billboards in one glTF file.
#[derive(Default, Clone)]
struct BillboardGltfHandler {
    /// Mesh primitive labels with all required wind and billboard attributes registered.
    turnable: HashSet<String>,
    /// Whether any billboard primitive has missing or unregistered attributes.
    unregistered: bool,
    /// Standard materials indexed by glTF label, ready to use as tree material bases.
    materials: HashMap<String, StandardMaterial>,
}

impl GltfExtensionHandler for BillboardGltfHandler {
    fn dyn_clone(&self) -> Box<dyn ErasedGltfExtensionHandler> {
        Box::new(self.clone())
    }

    fn on_material(
        &mut self,
        _load_context: &mut LoadContext<'_>,
        _gltf_material: &gltf_data::Material,
        _material: Handle<GltfMaterial>,
        material_asset: &GltfMaterial,
        material_label: &str,
    ) {
        let material = standard_material_from_gltf_material(material_asset);
        self.materials.insert(material_label.to_owned(), material);
    }

    #[allow(clippy::too_many_arguments)]
    fn on_gltf_primitive(
        &mut self,
        _load_context: &mut LoadContext<'_>,
        _gltf_document: &gltf_data::Gltf,
        gltf_mesh: &gltf_data::Mesh,
        gltf_primitive: &gltf_data::Primitive,
        _buffer_data: &[Vec<u8>],
        custom_vertex_attributes: &bevy::platform::collections::HashMap<
            Box<str>,
            MeshVertexAttribute,
        >,
        _gltf_mesh_on_skinned_nodes: bool,
        _gltf_mesh_on_non_skinned_nodes: bool,
        _user_mesh: &mut Option<Mesh>,
    ) -> impl bevy::tasks::ConditionalSendFuture<Output = ()> {
        // Check that each mesh primitive supplies all three attributes and that
        // the loader is configured to keep them.
        let has = |name: &str| {
            gltf_primitive
                .get(&Semantic::Extras(name.to_owned()))
                .is_some()
        };
        if has("BILLBOARD_PIVOT") {
            let loaded = |name: &str| custom_vertex_attributes.contains_key(name);
            if ATTRIBUTES.iter().all(|(name, _)| has(name) && loaded(name)) {
                let label = GltfAssetLabel::Primitive {
                    mesh: gltf_mesh.index(),
                    primitive: gltf_primitive.index(),
                };
                self.turnable.insert(label.to_string());
            } else {
                self.unregistered = true;
            }
        }
        async {}
    }

    fn on_scene_completed(
        &mut self,
        load_context: &mut LoadContext<'_>,
        _scene: &gltf_data::Scene,
        _world_root_id: Entity,
        scene_world: &mut World,
    ) {
        if self.unregistered {
            warn!(
                "ze_tree_bevy: {} has billboard shapes Bevy cannot turn; register their \
                 attributes with add_tree_gltf_attributes, and export with wind enabled",
                load_context.path()
            );
            self.unregistered = false;
        }

        // Find billboard nodes and their fallback partners in the glTF metadata.
        let mut billboards = Vec::new();
        let mut culls = Vec::new();
        let mut tagged = scene_world.query::<(Entity, &GltfExtras, Option<&Name>)>();
        for (entity, extras, name) in tagged.iter(scene_world) {
            let Ok(value) = serde_json::from_str::<Value>(&extras.value) else {
                continue;
            };
            if value.get("billboard").is_some() {
                billboards.push((entity, name.map(|n| n.as_str().to_owned())));
            } else if let Some(partner) = value["billboard_cull"]["partner"].as_str() {
                culls.push((entity, partner.to_owned()));
            }
        }

        // Give eligible child meshes the material that animates wind and rotation.
        let turnable: HashSet<_> = self
            .turnable
            .iter()
            .map(|label| load_context.get_label_handle::<Mesh>(label.clone()).id())
            .collect();
        let mut converted = HashSet::new();
        for (node, name) in billboards {
            let children: Vec<Entity> = scene_world
                .get::<Children>(node)
                .map(|c| c.iter().collect())
                .unwrap_or_default();
            let mut all = !children.is_empty();
            for child in children {
                all &= self.convert(load_context, scene_world, child, &turnable);
            }
            if all && let Some(name) = name {
                converted.insert(name);
            }
        }

        // Remove a fallback partner only when all children of its named
        // billboard node were converted successfully.
        for (entity, partner) in culls {
            if converted.contains(&partner) {
                scene_world.entity_mut(entity).despawn();
            }
        }
    }
}

impl BillboardGltfHandler {
    /// Applies `TreeMaterial` and requests new bounds for an eligible mesh entity.
    /// Returns `false` without changing the entity if its mesh is not in
    /// `turnable`, it uses animation joints, or its standard material is unavailable.
    fn convert(
        &mut self,
        load_context: &mut LoadContext<'_>,
        world: &mut World,
        entity: Entity,
        turnable: &HashSet<AssetId<Mesh>>,
    ) -> bool {
        let entity_ref = world.entity(entity);
        let (Some(mesh), Some(material)) = (
            entity_ref.get::<Mesh3d>(),
            entity_ref.get::<MeshMaterial3d<StandardMaterial>>(),
        ) else {
            return false;
        };
        // TreeMaterial does not support meshes deformed by animation joints.
        if !turnable.contains(&mesh.0.id()) || entity_ref.contains::<SkinnedMesh>() {
            return false;
        }

        // Preserve the original material settings and share one tree material
        // among meshes that use the same glTF material.
        let material_id = material.0.id();
        let Some((label, base)) = self.materials.iter().find(|(label, _)| {
            load_context
                .get_label_handle::<StandardMaterial>(format!("{label}/std"))
                .id()
                == material_id
        }) else {
            return false;
        };
        let tree_label = format!("{label}/ze_tree_billboard");
        let handle = if load_context.has_labeled_asset(&tree_label) {
            load_context.get_label_handle::<TreeMaterial>(tree_label)
        } else {
            // Remove the normal map: exported billboards lack the tangent
            // directions needed to apply its surface detail correctly.
            let base = StandardMaterial {
                normal_map_texture: None,
                ..base.clone()
            };
            let material = TreeMaterial {
                base,
                extension: WindExtension::default(),
            };
            load_context.add_labeled_asset(tree_label, material)
        };

        world
            .entity_mut(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((MeshMaterial3d(handle), PendingBillboardBounds));
        true
    }
}
