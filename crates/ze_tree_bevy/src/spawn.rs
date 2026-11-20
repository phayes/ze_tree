//! Generate trees in background tasks and display their parts as child entities.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use ze_tree::{LodLevel, OptionsError, Skeleton, TreeError, TreeOptions, TreePreset, TrellisWire};

use crate::material::{
    BarkTextures, TreeMaterial, bark_material, leaf_material, load_leaf_texture, trellis_material,
};
use crate::mesh::{bark_mesh, leaf_mesh};
use crate::wind::Wind;

/// A generated tree positioned by this entity's transform.
///
/// Add this component to an entity after installing [`ZeTreePlugin`](crate::ZeTreePlugin).
/// The tree appears when background generation finishes. Bark and leaf children
/// provide each configured detail level; visible trellis wires get their own children.
/// Trees with identical options share meshes and materials. Changing this component
/// rebuilds its parts; removing it despawns the generated children.
#[derive(Component, Debug, Clone, PartialEq)]
#[require(Transform, Visibility)]
pub enum ZeTree {
    /// Generate from options supplied in memory. Use [`ZeTree::new`] to wrap owned options.
    Options(Arc<TreeOptions>),
    /// Load options from an ez-tree JSON file with a `.tree.json` extension.
    /// The tree waits for the asset to load and rebuilds when Bevy reports it modified.
    /// Live file edits require Bevy asset watching to be enabled.
    Asset(Handle<TreeOptionsAsset>),
}

impl ZeTree {
    /// Creates a tree component from options. Generation starts after it is spawned.
    pub fn new(options: TreeOptions) -> Self {
        Self::Options(Arc::new(options))
    }

    /// Creates a tree component using the settings of a built-in preset.
    pub fn preset(preset: TreePreset) -> Self {
        Self::new(preset.options())
    }
}

/// Tree settings loaded from an ez-tree `.tree.json` file for [`ZeTree::Asset`].
///
/// Omitted JSON fields use their defaults. The loader validates the settings
/// before making the asset available.
#[derive(Asset, TypePath, Debug, Clone, PartialEq)]
pub struct TreeOptionsAsset(pub TreeOptions);

/// Identifies a generated child entity that draws one part of a [`ZeTree`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreePart {
    /// Index in the detail levels sorted by distance, starting at 0. Trellis wires use 0.
    pub lod: usize,
    /// The geometry drawn by this child entity.
    pub kind: TreePartKind,
}

/// Parts that can be drawn as children of a tree entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreePartKind {
    /// The trunk and branches, drawn together.
    Bark,
    /// The tree's leaves, drawn together.
    Leaves,
    /// A single support wire for a trellis.
    Trellis,
}

/// Reads and validates ez-tree JSON files with the `.tree.json` extension.
#[derive(Debug, Default, TypePath)]
pub(crate) struct TreeOptionsLoader;

impl AssetLoader for TreeOptionsLoader {
    type Asset = TreeOptionsAsset;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _context: &mut LoadContext<'_>,
    ) -> Result<TreeOptionsAsset, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let options =
            TreeOptions::from_json(std::str::from_utf8(&bytes)?).map_err(TreeError::Json)?;
        options.validate().map_err(TreeError::Options)?;
        Ok(TreeOptionsAsset(options))
    }

    fn extensions(&self) -> &[&str] {
        &["tree.json"]
    }
}

/// Texture location and detail levels copied from the plugin configuration.
#[derive(Resource, Debug, Clone)]
pub(crate) struct SpawnSettings {
    pub texture_root: Option<String>,
    pub lod_levels: Vec<LodLevel>,
}

/// Serialized tree options used to find shared generation results. Entities with
/// the same key reuse one [`Generated`] entry.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TreeKey(String);

/// Marks a tree waiting for its options, generation result or child entities.
#[derive(Component, Debug, Default)]
pub(crate) struct AwaitingParts;

/// Bark and leaf meshes produced by a background task for one detail level.
struct LevelMeshes {
    level: LodLevel,
    bark: Option<Mesh>,
    leaves: Option<Mesh>,
}

/// Generated detail levels, or the validation error that prevented generation.
type TaskOutput = Result<Vec<LevelMeshes>, OptionsError>;

/// Mesh handles and the camera-distance range for one detail level.
struct LevelAssets {
    /// Camera-distance limits; `None` means the sole detail level has no distance limit.
    range: Option<VisibilityRange>,
    /// Bark mesh, absent when this level contains no bark triangles.
    bark: Option<Handle<Mesh>>,
    /// Leaf mesh, absent when this level contains no leaf triangles.
    leaves: Option<Handle<Mesh>>,
}

/// Meshes and materials reused by trees with identical options.
struct Generated {
    /// Detail levels sorted from the nearest switching distance to the farthest.
    levels: Vec<LevelAssets>,
    bark: Handle<TreeMaterial>,
    leaves: Handle<TreeMaterial>,
    /// One cylinder mesh and material shared by all wires, plus each wire's placement.
    trellis: Option<(Handle<Mesh>, Handle<StandardMaterial>, Vec<Transform>)>,
}

enum Entry {
    Pending(Task<TaskOutput>, Arc<TreeOptions>),
    Ready(Arc<Generated>),
    Failed,
}

/// Caches generation tasks and results by tree options, and textures by bark or leaf kind.
#[derive(Resource, Default)]
pub(crate) struct TreeCache {
    trees: HashMap<TreeKey, Entry>,
    bark_textures: HashMap<String, BarkTextures>,
    leaf_textures: HashMap<String, Handle<Image>>,
}

/// Removes generated children when a tree changes or loses its [`ZeTree`] component.
/// Changed trees are marked to generate replacement parts; unrelated children are kept.
pub(crate) fn reset_changed_trees(
    mut commands: Commands,
    changed: Query<(Entity, Option<&Children>), Changed<ZeTree>>,
    mut removed: RemovedComponents<ZeTree>,
    children: Query<&Children>,
    parts: Query<(), With<TreePart>>,
) {
    let despawn_parts = |commands: &mut Commands, kids: Option<&Children>| {
        for &child in kids.into_iter().flatten() {
            if parts.contains(child) {
                commands.entity(child).despawn();
            }
        }
    };
    for (entity, kids) in &changed {
        despawn_parts(&mut commands, kids);
        commands
            .entity(entity)
            .remove::<TreeKey>()
            .insert(AwaitingParts);
    }
    for entity in removed.read() {
        despawn_parts(&mut commands, children.get(entity).ok());
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.remove::<(TreeKey, AwaitingParts)>();
        }
    }
}

/// Marks trees for rebuilding when Bevy reports a change to their options asset.
pub(crate) fn reload_modified_options(
    mut events: MessageReader<AssetEvent<TreeOptionsAsset>>,
    mut trees: Query<&mut ZeTree>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            for mut tree in &mut trees {
                if matches!(&*tree, ZeTree::Asset(handle) if handle.id() == *id) {
                    tree.set_changed();
                }
            }
        }
    }
}

/// Selects trees still waiting for an options key, including assets that have not loaded.
type AwaitingKey = (With<AwaitingParts>, Without<TreeKey>);

/// Assigns an options key to each waiting tree and starts a background task only
/// when those options are not already cached.
pub(crate) fn start_generation(
    mut commands: Commands,
    waiting: Query<(Entity, &ZeTree), AwaitingKey>,
    options_assets: Res<Assets<TreeOptionsAsset>>,
    settings: Res<SpawnSettings>,
    mut cache: ResMut<TreeCache>,
) {
    for (entity, tree) in &waiting {
        let options = match tree {
            ZeTree::Options(options) => options.clone(),
            ZeTree::Asset(handle) => match options_assets.get(handle) {
                Some(asset) => Arc::new(asset.0.clone()),
                None => continue,
            },
        };
        let key = TreeKey(options.to_json());
        cache.trees.entry(key.clone()).or_insert_with(|| {
            let levels = settings.lod_levels.clone();
            let task_options = options.clone();
            let task =
                AsyncComputeTaskPool::get().spawn(async move { generate(&task_options, &levels) });
            Entry::Pending(task, options)
        });
        commands.entity(entity).insert(key);
    }
}

/// Builds the branch structure once, then creates meshes for detail levels sorted
/// by their switching distances.
fn generate(options: &TreeOptions, levels: &[LodLevel]) -> TaskOutput {
    let skeleton = Skeleton::generate(options)?;
    let mut levels = levels.to_vec();
    levels.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    Ok(levels
        .into_iter()
        .map(|level| {
            let mesh = skeleton.mesh(options, &level.detail);
            LevelMeshes {
                level,
                bark: (!mesh.branches.indices.is_empty())
                    .then(|| bark_mesh(&mesh.branches, &options.bark)),
                leaves: (!mesh.leaves.indices.is_empty()).then(|| leaf_mesh(&mesh.leaves)),
            }
        })
        .collect())
}

/// Collects completed tasks and registers shared meshes and materials.
/// Invalid options are logged and cached as a failed result.
pub(crate) fn finish_generation(
    mut cache: ResMut<TreeCache>,
    settings: Res<SpawnSettings>,
    wind: Res<Wind>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tree_materials: ResMut<Assets<TreeMaterial>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
) {
    let cache = &mut *cache;
    for entry in cache.trees.values_mut() {
        let Entry::Pending(task, options) = entry else {
            continue;
        };
        let Some(output) = block_on(poll_once(task)) else {
            continue;
        };
        let levels = match output {
            Ok(levels) => levels,
            Err(error) => {
                error!("ze_tree_bevy: cannot generate tree: {error}");
                *entry = Entry::Failed;
                continue;
            }
        };

        // Reuse textures by bark or leaf kind, even when the trees have different options.
        let (bark_textures, leaf_texture) = match &settings.texture_root {
            Some(root) => {
                let bark = cache
                    .bark_textures
                    .entry(options.bark.kind.clone())
                    .or_insert_with(|| BarkTextures::load(&asset_server, root, &options.bark.kind));
                let leaf = cache
                    .leaf_textures
                    .entry(options.leaves.kind.clone())
                    .or_insert_with(|| {
                        load_leaf_texture(&asset_server, root, &options.leaves.kind)
                    });
                (Some(bark.clone()), Some(leaf.clone()))
            }
            None => (None, None),
        };

        let ranges = visibility_ranges(levels.iter().map(|l| l.level));
        let generated = Generated {
            levels: levels
                .into_iter()
                .zip(ranges)
                .map(|(l, range)| LevelAssets {
                    range,
                    bark: l.bark.map(|m| meshes.add(m)),
                    leaves: l.leaves.map(|m| meshes.add(m)),
                })
                .collect(),
            bark: tree_materials.add(bark_material(&options.bark, bark_textures.as_ref(), wind.0)),
            leaves: tree_materials.add(leaf_material(&options.leaves, leaf_texture, wind.0)),
            trellis: (options.trellis.enabled && options.trellis.visible).then(|| {
                let wires = options.trellis.wires();
                (
                    meshes.add(Cylinder::new(options.trellis.cylinder_radius, 1.0)),
                    standard_materials.add(trellis_material(&options.trellis)),
                    wires.iter().map(wire_transform).collect(),
                )
            }),
        };
        *entry = Entry::Ready(Arc::new(generated));
    }
}

/// Builds camera-distance limits for detail levels supplied in nearest-first order.
/// A single level has no distance limit.
///
/// The first level starts at distance zero; the last extends indefinitely.
/// At each intervening switch, adjacent levels fade over a band extending
/// `distance × hysteresis` to either side of the switching distance.
fn visibility_ranges(levels: impl Iterator<Item = LodLevel>) -> Vec<Option<VisibilityRange>> {
    let levels: Vec<LodLevel> = levels.collect();
    if levels.len() < 2 {
        return vec![None; levels.len()];
    }
    let margin = |level: &LodLevel| {
        let half = level.distance * level.hysteresis;
        (level.distance - half).max(0.0)..level.distance + half
    };
    (0..levels.len())
        .map(|i| {
            let start_margin = if i == 0 { 0.0..0.0 } else { margin(&levels[i]) };
            let end_margin = match levels.get(i + 1) {
                Some(next) => margin(next),
                None => f32::INFINITY..f32::INFINITY,
            };
            Some(VisibilityRange {
                start_margin,
                end_margin,
                use_aabb: false,
            })
        })
        .collect()
}

/// Positions, rotates and stretches a unit-height cylinder to connect a wire's endpoints.
fn wire_transform(wire: &TrellisWire) -> Transform {
    let span = wire.end - wire.start;
    Transform {
        translation: (wire.start + wire.end) / 2.0,
        rotation: Quat::from_rotation_arc(Vec3::Y, span.normalize_or(Vec3::Y)),
        scale: Vec3::new(1.0, span.length(), 1.0),
    }
}

/// Adds child entities once a waiting tree's assets are ready.
/// Stops waiting after a failed generation attempt.
pub(crate) fn spawn_parts(
    mut commands: Commands,
    waiting: Query<(Entity, &TreeKey), With<AwaitingParts>>,
    cache: Res<TreeCache>,
) {
    for (entity, key) in &waiting {
        match cache.trees.get(key) {
            Some(Entry::Ready(generated)) => {
                commands.entity(entity).remove::<AwaitingParts>();
                spawn_tree_parts(&mut commands, entity, generated);
            }
            Some(Entry::Failed) => {
                commands.entity(entity).remove::<AwaitingParts>();
            }
            Some(Entry::Pending(..)) | None => {}
        }
    }
}

fn spawn_tree_parts(commands: &mut Commands, tree: Entity, generated: &Generated) {
    for (lod, level) in generated.levels.iter().enumerate() {
        let parts = [
            (TreePartKind::Bark, &level.bark, &generated.bark),
            (TreePartKind::Leaves, &level.leaves, &generated.leaves),
        ];
        for (kind, mesh, material) in parts {
            let Some(mesh) = mesh else { continue };
            let mut part = commands.spawn((
                TreePart { lod, kind },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                ChildOf(tree),
            ));
            if let Some(range) = &level.range {
                part.insert(range.clone());
            }
        }
    }
    if let Some((mesh, material, transforms)) = &generated.trellis {
        for transform in transforms {
            commands.spawn((
                TreePart {
                    lod: 0,
                    kind: TreePartKind::Trellis,
                },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                *transform,
                ChildOf(tree),
            ));
        }
    }
}

/// Removes cached generation entries no tree entity uses. Meshes and materials
/// can be freed once no other handles retain them. Cached textures are kept.
pub(crate) fn prune_cache(
    mut removed: RemovedComponents<TreeKey>,
    keys: Query<&TreeKey>,
    mut cache: ResMut<TreeCache>,
) {
    if removed.read().count() == 0 {
        return;
    }
    let used: std::collections::HashSet<&TreeKey> = keys.iter().collect();
    cache.trees.retain(|key, _| used.contains(key));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ze_tree::default_lod_levels;

    #[test]
    fn detail_levels_hand_over_at_their_distances() {
        let ranges = visibility_ranges(default_lod_levels().into_iter());
        let ranges: Vec<VisibilityRange> = ranges.into_iter().map(Option::unwrap).collect();
        assert_eq!(ranges[0].start_margin, 0.0..0.0);
        assert_eq!(ranges[0].end_margin, 95.0..105.0);
        assert_eq!(ranges[1].start_margin, 95.0..105.0);
        assert_eq!(ranges[1].end_margin, 237.5..262.5);
        assert_eq!(ranges[2].start_margin, 237.5..262.5);
        assert_eq!(ranges[2].end_margin.start, f32::INFINITY);

        // A lone detail level must remain visible at every camera distance.
        let single = visibility_ranges([LodLevel::default()].into_iter());
        assert!(matches!(single[..], [None]));
    }

    #[test]
    fn wire_cylinders_span_their_wires() {
        let wire = TrellisWire {
            start: Vec3::new(-2.0, 1.0, 3.0),
            end: Vec3::new(4.0, 1.0, 3.0),
        };
        let transform = wire_transform(&wire);
        // Transform both ends of the unit cylinder to check that they reach the wire endpoints.
        let bottom = transform.transform_point(Vec3::new(0.0, -0.5, 0.0));
        let top = transform.transform_point(Vec3::new(0.0, 0.5, 0.0));
        assert!(bottom.abs_diff_eq(wire.start, 1e-5) || bottom.abs_diff_eq(wire.end, 1e-5));
        assert!(top.abs_diff_eq(wire.start, 1e-5) || top.abs_diff_eq(wire.end, 1e-5));
        assert!(!bottom.abs_diff_eq(top, 1e-3));
    }
}
