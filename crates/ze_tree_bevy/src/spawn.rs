//! Generate trees in background tasks and display their parts as child entities.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, VisibilityRange};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use ze_tree::{
    BranchShape, LodLevel, MeshBuffers, OptionsError, Skeleton, TextureCompression, TextureSize,
    TreeError, TreeOptions, TreePreset, TrellisWire, default_lod_levels,
};

use crate::material::{
    BarkTextures, LeafTextures, TreeMaterial, average_bark_material, bark_material,
    flower_material, fruit_material, leaf_material, trellis_material,
};
use crate::material_key::{BarkKey, LeafKey, TrellisKey};
use crate::mesh::{bark_mesh, billboard_aabb, cluster_mesh, leaf_mesh};
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
    /// The tree's fruit and their stalks, drawn together.
    Fruit,
    /// The tree's flowers and their stalks, drawn together.
    Flowers,
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

/// Mesh and material settings for one entry in
/// [`ZeTreePlugin::lod_levels`](crate::ZeTreePlugin::lod_levels).
/// The plugin automatically renders [`BranchShape::Ribbons`] from both sides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeLodLevel {
    /// Mesh detail and the camera distance, in world units, where this level starts.
    pub level: LodLevel,
    /// Bark texture size or flat-colour shading for this level.
    pub bark: BarkTexture,
    /// Leaf image to use for this level's flat leaf cards.
    pub leaves: LeafTexture,
}

/// Bark appearance for a detail level. These choices apply when the plugin has
/// a texture folder configured; without one, bark uses its tint alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarkTexture {
    /// Use colour, normal and roughness images at the given size. Smaller
    /// images use less memory. Supply the files under the plugin's texture folder.
    Textured(TextureSize),
    /// Use one colour that approximates textured bark viewed from a distance.
    /// Requires no bark images; see [`average_bark_material`] for tint handling
    /// and the fallback for unknown bark kinds.
    AverageColor,
}

/// Image used on each flat leaf card. Requires a configured plugin texture folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LeafTexture {
    /// Use the leaf kind's regular colour image ([`Color`](ze_tree::LeafMap::Color))
    /// and its normal map when available.
    #[default]
    Leaf,
    /// Use the leaf kind's [`Clump`](ze_tree::LeafMap::Clump) image to show several
    /// leaves per card, without a normal map. For distant trees, increase
    /// [`leaf_stride`](ze_tree::LodDetail::leaf_stride) to draw fewer cards and
    /// [`leaf_scale`](ze_tree::LodDetail::leaf_scale) to make them larger.
    Clump,
}

/// Returns four detail levels, nearest first, using [`default_lod_levels`].
/// Bark uses 1k textures at the nearest level and 512 textures at the next two.
/// The fourth level uses a flat average bark colour and images of several leaves
/// per card. These material choices require a configured plugin texture folder.
pub fn default_tree_lod_levels() -> Vec<TreeLodLevel> {
    let [near, middle, far, farthest] = default_lod_levels();
    let textured = |level, size| TreeLodLevel {
        level,
        bark: BarkTexture::Textured(size),
        leaves: LeafTexture::Leaf,
    };
    vec![
        textured(near, TextureSize::Size1k),
        textured(middle, TextureSize::Size512),
        textured(far, TextureSize::Size512),
        TreeLodLevel {
            level: farthest,
            bark: BarkTexture::AverageColor,
            leaves: LeafTexture::Clump,
        },
    ]
}

/// Texture location, format and detail levels copied from the plugin configuration.
#[derive(Resource, Debug, Clone)]
pub(crate) struct SpawnSettings {
    pub texture_root: Option<String>,
    pub texture_compression: TextureCompression,
    pub lod_levels: Vec<TreeLodLevel>,
}

/// The tree's [`TreeOptions::tree_id`], used to find shared generation results.
/// Entities with the same key reuse one [`Generated`] entry.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TreeKey(u64);

/// Present on a [`ZeTree`] entity while its tree is being prepared.
///
/// The plugin adds this whenever a tree is spawned or its [`ZeTree`] changes,
/// and removes it once the tree's parts have been spawned or generation has
/// failed. A [`TreeFinished`] event is triggered at the same moment. While a
/// `.tree.json` asset has not loaded yet, the marker stays on.
#[derive(Component, Debug, Default)]
pub struct TreeGenerating;

/// Triggered on a [`ZeTree`] entity when preparing its tree ends.
///
/// Observe it on the entity, or globally, to learn when a new or changed tree
/// has appeared. On success the tree's [`TreePart`] children are queued with
/// the same commands, so they exist once this frame's commands have run.
#[derive(EntityEvent, Debug, Clone, PartialEq)]
pub struct TreeFinished {
    /// The tree entity.
    pub entity: Entity,
    /// `Ok` when the parts were spawned, or why the options were rejected.
    pub result: Result<(), OptionsError>,
}

/// A part’s Bevy mesh and optional bounds covering its camera-facing rotations.
struct PartMesh<M> {
    mesh: M,
    bounds: Option<Aabb>,
}

/// Meshes produced by a background task for one detail level; each is absent
/// when the level has no triangles of that kind.
struct LevelMeshes {
    level: TreeLodLevel,
    bark: Option<PartMesh<Mesh>>,
    leaves: Option<PartMesh<Mesh>>,
    fruit: Option<PartMesh<Mesh>>,
    flowers: Option<PartMesh<Mesh>>,
}

/// Generated detail levels, or the validation error that prevented generation.
type TaskOutput = Result<Vec<LevelMeshes>, OptionsError>;

/// Mesh handles and the camera-distance range for one detail level.
struct LevelAssets {
    /// Camera-distance limits; `None` means the sole detail level has no distance limit.
    range: Option<VisibilityRange>,
    /// Bark mesh, absent when this level contains no bark triangles.
    bark: Option<PartMesh<Handle<Mesh>>>,
    /// Leaf mesh, absent when this level contains no leaf triangles.
    leaves: Option<PartMesh<Handle<Mesh>>>,
    /// Fruit mesh, absent when this level contains no fruit triangles.
    fruit: Option<PartMesh<Handle<Mesh>>>,
    /// Flower mesh, absent when this level contains no flower triangles.
    flowers: Option<PartMesh<Handle<Mesh>>>,
    /// Bark material, shared by levels with matching bark settings and sidedness.
    bark_material: Handle<TreeMaterial>,
    /// Leaf material, shared by levels using the same leaf image selection.
    leaf_material: Handle<TreeMaterial>,
}

/// Meshes and materials reused by trees with identical options.
struct Generated {
    /// Detail levels sorted from the nearest switching distance to the farthest.
    levels: Vec<LevelAssets>,
    fruit: Handle<TreeMaterial>,
    flowers: Handle<TreeMaterial>,
    /// One cylinder mesh and material shared by all wires, plus each wire's placement.
    trellis: Option<(Handle<Mesh>, Handle<StandardMaterial>, Vec<Transform>)>,
}

enum Entry {
    Pending(Task<TaskOutput>, Arc<TreeOptions>),
    Ready(Arc<Generated>),
    Failed(OptionsError),
}

/// Reuses generated trees when their options match. Trees with different shapes
/// can still share texture images and materials (surface colour, texture and shine)
/// when those settings match, avoiding duplicate assets.
#[derive(Resource, Default)]
pub(crate) struct TreeCache {
    trees: HashMap<TreeKey, Entry>,
    bark_textures: HashMap<(String, TextureSize), BarkTextures>,
    leaf_textures: HashMap<(String, LeafTexture), LeafTextures>,
    bark_materials: HashMap<BarkKey, Handle<TreeMaterial>>,
    leaf_materials: HashMap<LeafKey, Handle<TreeMaterial>>,
    trellis_materials: HashMap<TrellisKey, Handle<StandardMaterial>>,
    /// Shared fruit appearance, retained even when no trees use it.
    fruit_material: Option<Handle<TreeMaterial>>,
    /// Shared flower appearance, retained even when no trees use it.
    flower_material: Option<Handle<TreeMaterial>>,
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
            .insert(TreeGenerating);
    }
    for entity in removed.read() {
        despawn_parts(&mut commands, children.get(entity).ok());
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.remove::<(TreeKey, TreeGenerating)>();
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
type AwaitingKey = (With<TreeGenerating>, Without<TreeKey>);

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
        let key = TreeKey(options.tree_id());
        cache.trees.entry(key).or_insert_with(|| {
            let levels = settings.lod_levels.clone();
            let task_options = options.clone();
            let task =
                AsyncComputeTaskPool::get().spawn(async move { generate(&task_options, &levels) });
            Entry::Pending(task, options)
        });
        commands.entity(entity).insert(key);
    }
}

/// Generates one skeleton and meshes for each detail level, sorted by distance.
/// Skips [`TreeMesh::billboard_cull`](ze_tree::TreeMesh::billboard_cull) because
/// the tree shaders rotate the main shapes toward each view.
fn generate(options: &TreeOptions, levels: &[TreeLodLevel]) -> TaskOutput {
    let skeleton = Skeleton::generate(options)?;
    let mut levels = levels.to_vec();
    levels.sort_by(|a, b| a.level.distance.total_cmp(&b.level.distance));
    let part = |buffers: &MeshBuffers, convert: &dyn Fn(&MeshBuffers) -> Mesh| {
        (!buffers.indices.is_empty()).then(|| PartMesh {
            mesh: convert(buffers),
            bounds: billboard_aabb(buffers),
        })
    };
    Ok(levels
        .into_iter()
        .map(|level| {
            let mesh = skeleton.mesh(options, &level.level.detail);
            LevelMeshes {
                level,
                bark: part(&mesh.branches, &|b| bark_mesh(b, &options.bark)),
                leaves: part(&mesh.leaves, &leaf_mesh),
                fruit: part(&mesh.fruit, &cluster_mesh),
                flowers: part(&mesh.flowers, &cluster_mesh),
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
                *entry = Entry::Failed(error);
                continue;
            }
        };

        // Share matching materials across trees and their distance-based detail
        // levels. Bark visible from both sides needs a separate material.
        // Reuse texture images; without a texture folder, use plain colours.
        let compression = settings.texture_compression;
        let root = settings.texture_root.as_deref();
        let ranges = visibility_ranges(levels.iter().map(|l| l.level.level));
        let mut level_assets = Vec::with_capacity(levels.len());
        for (l, range) in levels.into_iter().zip(ranges) {
            // Render both faces of each ribbon strip.
            let two_sided = l.level.level.detail.branch_shape == BranchShape::Ribbons;
            let look = root.map(|_| l.level.bark);
            let key = BarkKey::new(&options.bark, look, two_sided);
            let bark = cache.bark_materials.entry(key).or_insert_with(|| {
                let mut material = match (look, root) {
                    (Some(BarkTexture::Textured(size)), Some(root)) => {
                        let textures = cache
                            .bark_textures
                            .entry((options.bark.kind.clone(), size))
                            .or_insert_with(|| {
                                BarkTextures::load(
                                    &asset_server,
                                    root,
                                    &options.bark.kind,
                                    size,
                                    compression,
                                )
                            });
                        bark_material(&options.bark, Some(&*textures), wind.0)
                    }
                    (Some(BarkTexture::AverageColor), _) => {
                        average_bark_material(&options.bark, wind.0)
                    }
                    _ => bark_material(&options.bark, None, wind.0),
                };
                // Ribbon billboards have no tangents to orient a normal map.
                if two_sided {
                    material.base.cull_mode = None;
                    material.base.double_sided = true;
                    material.base.normal_map_texture = None;
                }
                tree_materials.add(material)
            });

            // Omit normal maps for camera-facing leaves, which have no tangents.
            let image = root.map(|_| l.level.leaves);
            let billboard = l.level.level.detail.leaf_billboard.is_some();
            let key = LeafKey::new(&options.leaves, image, billboard);
            let leaves = cache.leaf_materials.entry(key).or_insert_with(|| {
                let textures = image.zip(root).map(|(image, root)| {
                    let kind = &options.leaves.kind;
                    let textures = cache
                        .leaf_textures
                        .entry((kind.clone(), image))
                        .or_insert_with(|| match image {
                            LeafTexture::Leaf => {
                                LeafTextures::load(&asset_server, root, kind, compression)
                            }
                            LeafTexture::Clump => {
                                LeafTextures::load_clump(&asset_server, root, kind, compression)
                            }
                        })
                        .clone();
                    LeafTextures {
                        normal: textures.normal.filter(|_| !billboard),
                        ..textures
                    }
                });
                tree_materials.add(leaf_material(&options.leaves, textures.as_ref(), wind.0))
            });

            let mut add = |part: Option<PartMesh<Mesh>>| {
                part.map(|p| PartMesh {
                    mesh: meshes.add(p.mesh),
                    bounds: p.bounds,
                })
            };
            level_assets.push(LevelAssets {
                range,
                bark: add(l.bark),
                leaves: add(l.leaves),
                fruit: add(l.fruit),
                flowers: add(l.flowers),
                bark_material: bark.clone(),
                leaf_material: leaves.clone(),
            });
        }

        // All trees share one fruit material and one flower material because
        // these surface settings do not vary between trees.
        let fruit = cache
            .fruit_material
            .get_or_insert_with(|| tree_materials.add(fruit_material(wind.0)))
            .clone();
        let flowers = cache
            .flower_material
            .get_or_insert_with(|| tree_materials.add(flower_material(wind.0)))
            .clone();
        let trellis = (options.trellis.enabled && options.trellis.visible).then(|| {
            let wires = options.trellis.wires();
            let material = cache
                .trellis_materials
                .entry(TrellisKey::new(&options.trellis))
                .or_insert_with(|| standard_materials.add(trellis_material(&options.trellis)))
                .clone();
            (
                meshes.add(Cylinder::new(options.trellis.cylinder_radius, 1.0)),
                material,
                wires.iter().map(wire_transform).collect(),
            )
        });
        let generated = Generated {
            levels: level_assets,
            fruit,
            flowers,
            trellis,
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
/// Stops waiting after a failed generation attempt. Either way, triggers [`TreeFinished`].
pub(crate) fn spawn_parts(
    mut commands: Commands,
    waiting: Query<(Entity, &TreeKey), With<TreeGenerating>>,
    cache: Res<TreeCache>,
) {
    for (entity, key) in &waiting {
        let result = match cache.trees.get(key) {
            Some(Entry::Ready(generated)) => {
                spawn_tree_parts(&mut commands, entity, generated);
                Ok(())
            }
            Some(Entry::Failed(error)) => Err(error.clone()),
            Some(Entry::Pending(..)) | None => continue,
        };
        commands.entity(entity).remove::<TreeGenerating>();
        commands.trigger(TreeFinished { entity, result });
    }
}

fn spawn_tree_parts(commands: &mut Commands, tree: Entity, generated: &Generated) {
    for (lod, level) in generated.levels.iter().enumerate() {
        let parts = [
            (TreePartKind::Bark, &level.bark, &level.bark_material),
            (TreePartKind::Leaves, &level.leaves, &level.leaf_material),
            (TreePartKind::Fruit, &level.fruit, &generated.fruit),
            (TreePartKind::Flowers, &level.flowers, &generated.flowers),
        ];
        for (kind, mesh, material) in parts {
            let Some(mesh) = mesh else { continue };
            let mut part = commands.spawn((
                TreePart { lod, kind },
                Mesh3d(mesh.mesh.clone()),
                MeshMaterial3d(material.clone()),
                ChildOf(tree),
            ));
            if let Some(range) = &level.range {
                part.insert(range.clone());
            }
            // Use rotation-aware bounds so Bevy does not hide a billboard whose
            // stored shape is outside the view but whose rotated shape is visible.
            if let Some(bounds) = mesh.bounds {
                part.insert((bounds, NoAutoAabb));
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

/// Removes unused tree generation entries and their unneeded bark, leaf and
/// trellis materials. Their shapes and materials can be freed when nothing else
/// references them. Texture images and shared fruit and flower materials stay
/// cached for reuse.
pub(crate) fn prune_cache(
    mut removed: RemovedComponents<TreeKey>,
    keys: Query<&TreeKey>,
    mut cache: ResMut<TreeCache>,
) {
    if removed.read().count() == 0 {
        return;
    }
    let used: HashSet<&TreeKey> = keys.iter().collect();
    cache.trees.retain(|key, _| used.contains(key));

    // Preserve materials referenced by the generated trees still in the cache.
    let mut drawn = HashSet::new();
    let mut wires = HashSet::new();
    for entry in cache.trees.values() {
        let Entry::Ready(generated) = entry else {
            continue;
        };
        for level in &generated.levels {
            drawn.insert(level.bark_material.id());
            drawn.insert(level.leaf_material.id());
        }
        if let Some((_, material, _)) = &generated.trellis {
            wires.insert(material.id());
        }
    }
    cache.bark_materials.retain(|_, m| drawn.contains(&m.id()));
    cache.leaf_materials.retain(|_, m| drawn.contains(&m.id()));
    cache
        .trellis_materials
        .retain(|_, m| wires.contains(&m.id()));
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use ze_tree::default_lod_levels;

    /// Results of every [`TreeFinished`] event, in order.
    #[derive(Resource, Default)]
    struct Finished(Vec<Result<(), OptionsError>>);

    /// Creates a test app with one full-detail level and the spawn systems,
    /// without rendering or loading textures.
    fn spawn_app() -> App {
        spawn_app_with(vec![TreeLodLevel {
            level: LodLevel::default(),
            bark: BarkTexture::Textured(TextureSize::Size1k),
            leaves: LeafTexture::Leaf,
        }])
    }

    /// Creates a test app with the supplied levels and the spawn systems,
    /// without rendering or loading textures.
    fn spawn_app_with(lod_levels: Vec<TreeLodLevel>) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_asset::<TreeMaterial>()
            .init_asset::<TreeOptionsAsset>()
            .init_resource::<Wind>()
            .init_resource::<TreeCache>()
            .init_resource::<Finished>()
            .insert_resource(SpawnSettings {
                texture_root: None,
                texture_compression: TextureCompression::Bc7,
                lod_levels,
            })
            .add_systems(
                Update,
                (
                    reset_changed_trees,
                    start_generation,
                    finish_generation,
                    spawn_parts,
                )
                    .chain(),
            )
            .add_observer(|event: On<TreeFinished>, mut finished: ResMut<Finished>| {
                finished.0.push(event.result.clone());
            });
        app
    }

    /// Updates the app until `tree` loses [`TreeGenerating`].
    fn update_until_finished(app: &mut App, tree: Entity) {
        let deadline = Instant::now() + Duration::from_secs(30);
        app.update();
        while app.world().entity(tree).contains::<TreeGenerating>() {
            assert!(Instant::now() < deadline, "tree generation did not finish");
            std::thread::sleep(Duration::from_millis(1));
            app.update();
        }
    }

    fn part_count(app: &mut App, tree: Entity) -> usize {
        let mut parts = app.world_mut().query::<(&TreePart, &ChildOf)>();
        parts
            .iter(app.world())
            .filter(|(_, parent)| parent.parent() == tree)
            .count()
    }

    #[test]
    fn generating_marker_lasts_until_parts_spawn() {
        let mut app = spawn_app();
        let tree = app
            .world_mut()
            .spawn(ZeTree::preset(TreePreset::OakSmall))
            .id();

        app.update();
        assert!(app.world().entity(tree).contains::<TreeGenerating>());
        assert_eq!(part_count(&mut app, tree), 0);

        update_until_finished(&mut app, tree);
        let parts = part_count(&mut app, tree);
        assert!(parts > 0);
        assert_eq!(app.world().resource::<Finished>().0, vec![Ok(())]);

        // Touching the tree replaces its parts. The options are cached, so
        // this finishes within one update.
        app.world_mut()
            .get_mut::<ZeTree>(tree)
            .unwrap()
            .set_changed();
        app.update();
        assert!(!app.world().entity(tree).contains::<TreeGenerating>());
        assert_eq!(part_count(&mut app, tree), parts);
        assert_eq!(app.world().resource::<Finished>().0, vec![Ok(()), Ok(())]);

        // New options generate again.
        let mut options = TreePreset::OakSmall.options();
        options.seed += 1;
        *app.world_mut().get_mut::<ZeTree>(tree).unwrap() = ZeTree::new(options);
        app.update();
        assert!(app.world().entity(tree).contains::<TreeGenerating>());
        update_until_finished(&mut app, tree);
        assert_eq!(app.world().resource::<Finished>().0.len(), 3);
    }

    /// Checks that all four default levels have bark and leaves, and that bark
    /// is two-sided only at the fourth level, which uses ribbons.
    #[test]
    fn ribbon_bark_is_drawn_two_sided() {
        let mut app = spawn_app_with(default_tree_lod_levels());
        let tree = app
            .world_mut()
            .spawn(ZeTree::preset(TreePreset::OakSmall))
            .id();
        update_until_finished(&mut app, tree);

        let mut parts = app
            .world_mut()
            .query::<(&TreePart, &MeshMaterial3d<TreeMaterial>)>();
        let materials = app.world().resource::<Assets<TreeMaterial>>();
        let mut seen = Vec::new();
        for (part, material) in parts.iter(app.world()) {
            seen.push((part.lod, part.kind));
            if part.kind == TreePartKind::Bark {
                let base = &materials.get(&material.0).unwrap().base;
                let ribbons = part.lod == 3;
                assert_eq!(base.cull_mode.is_none(), ribbons, "lod {}", part.lod);
                assert_eq!(base.double_sided, ribbons, "lod {}", part.lod);
            }
        }
        for lod in 0..4 {
            assert!(seen.contains(&(lod, TreePartKind::Bark)), "lod {lod}");
            assert!(seen.contains(&(lod, TreePartKind::Leaves)), "lod {lod}");
        }
    }

    /// Checks that LOD3 spawns only the main parts, with rotation data and bounds
    /// for their camera-facing shapes. Crossed fallback shapes must not be spawned.
    #[test]
    fn farthest_level_parts_turn_toward_the_camera() {
        let mut app = spawn_app_with(default_tree_lod_levels());
        let tree = app
            .world_mut()
            .spawn(ZeTree::preset(TreePreset::Apple))
            .id();
        update_until_finished(&mut app, tree);

        let mut parts = app
            .world_mut()
            .query::<(&TreePart, &Mesh3d, Option<&Aabb>, Has<NoAutoAabb>)>();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let mut far_kinds = Vec::new();
        for (part, mesh, bounds, fixed) in parts.iter(app.world()) {
            let mesh = meshes.get(&mesh.0).unwrap();
            let billboard = mesh.contains_attribute(crate::ATTRIBUTE_BILLBOARD_PIVOT)
                && mesh.contains_attribute(crate::ATTRIBUTE_BILLBOARD_AXIS);
            let far = part.lod == 3 && part.kind != TreePartKind::Flowers;
            assert_eq!(billboard, far, "{part:?}");
            assert_eq!(fixed, far, "{part:?}");
            assert_eq!(bounds.is_some(), far, "{part:?}");
            if part.lod == 3 {
                far_kinds.push(part.kind);
            }
        }
        far_kinds.sort_by_key(|k| *k as u8);
        assert_eq!(
            far_kinds,
            [
                TreePartKind::Bark,
                TreePartKind::Leaves,
                TreePartKind::Fruit
            ]
        );
    }

    /// Checks that camera-facing leaves omit normal maps even when their leaf
    /// texture set provides one.
    #[test]
    fn billboard_leaves_leave_out_the_normal_map() {
        let [near, .., farthest] = default_lod_levels();
        let leaf_image = |level| TreeLodLevel {
            level,
            bark: BarkTexture::AverageColor,
            leaves: LeafTexture::Leaf,
        };
        let mut app = spawn_app_with(vec![leaf_image(near), leaf_image(farthest)]);
        app.world_mut().resource_mut::<SpawnSettings>().texture_root = Some("textures".into());
        let mut options = TreePreset::OakSmall.options();
        options.leaves.kind = "white_oak".to_owned();
        assert!(ze_tree::leaf_has_normal_map(&options.leaves.kind));
        let tree = app.world_mut().spawn(ZeTree::new(options)).id();
        update_until_finished(&mut app, tree);

        let mut parts = app
            .world_mut()
            .query::<(&TreePart, &MeshMaterial3d<TreeMaterial>)>();
        let materials = app.world().resource::<Assets<TreeMaterial>>();
        let mut seen = 0;
        for (part, material) in parts.iter(app.world()) {
            if part.kind == TreePartKind::Leaves {
                let base = &materials.get(&material.0).unwrap().base;
                assert_eq!(base.normal_map_texture.is_some(), part.lod == 0, "{part:?}");
                seen += 1;
            }
        }
        assert_eq!(seen, 2);
    }

    /// Returns the distinct material identifiers for one kind of part on a tree.
    fn part_materials(
        app: &mut App,
        tree: Entity,
        kind: TreePartKind,
    ) -> HashSet<AssetId<TreeMaterial>> {
        let mut parts = app
            .world_mut()
            .query::<(&TreePart, &MeshMaterial3d<TreeMaterial>, &ChildOf)>();
        parts
            .iter(app.world())
            .filter(|(part, _, parent)| part.kind == kind && parent.parent() == tree)
            .map(|(_, material, _)| material.0.id())
            .collect()
    }

    #[test]
    fn trees_that_look_the_same_share_materials() {
        let mut app = spawn_app();
        app.add_systems(Update, prune_cache.after(spawn_parts));
        let spawn = |app: &mut App, options: TreeOptions| {
            let tree = app
                .world_mut()
                .spawn(ZeTree::Options(Arc::new(options)))
                .id();
            update_until_finished(app, tree);
            tree
        };
        let oak = TreePreset::OakSmall.options();

        // Changing the random seed varies tree shape while preserving the
        // shared bark and leaf materials.
        let first = spawn(&mut app, oak.clone());
        let reseeded = TreeOptions {
            seed: oak.seed + 1,
            ..oak.clone()
        };
        let second = spawn(&mut app, reseeded);
        for kind in [TreePartKind::Bark, TreePartKind::Leaves] {
            let shared = part_materials(&mut app, first, kind);
            assert_eq!(shared.len(), 1, "{kind:?}");
            assert_eq!(part_materials(&mut app, second, kind), shared, "{kind:?}");
        }

        // Changing bark colour creates a separate bark material; leaves still
        // share their original material.
        let mut tinted = oak.clone();
        tinted.bark.tint ^= 0x10;
        let third = spawn(&mut app, tinted);
        assert!(
            part_materials(&mut app, third, TreePartKind::Bark).is_disjoint(&part_materials(
                &mut app,
                first,
                TreePartKind::Bark
            ))
        );
        assert_eq!(
            part_materials(&mut app, third, TreePartKind::Leaves),
            part_materials(&mut app, first, TreePartKind::Leaves)
        );
        assert_eq!(app.world().resource::<TreeCache>().bark_materials.len(), 2);

        // Once the differently coloured tree is removed, its unique bark
        // material is no longer needed in the cache.
        app.world_mut().entity_mut(third).despawn();
        app.update();
        let cache = app.world().resource::<TreeCache>();
        assert_eq!(cache.bark_materials.len(), 1);
        assert_eq!(cache.leaf_materials.len(), 1);
    }

    #[test]
    fn rejected_options_finish_with_their_error() {
        let mut app = spawn_app();
        let mut options = TreePreset::OakSmall.options();
        options.branch.sections[0] = 0;
        let tree = app.world_mut().spawn(ZeTree::new(options)).id();

        update_until_finished(&mut app, tree);
        assert_eq!(part_count(&mut app, tree), 0);
        assert_eq!(
            app.world().resource::<Finished>().0,
            vec![Err(OptionsError::ZeroSections { level: 0 })]
        );
    }

    #[test]
    fn detail_levels_hand_over_at_their_distances() {
        let ranges = visibility_ranges(default_lod_levels().into_iter());
        let ranges: Vec<VisibilityRange> = ranges.into_iter().map(Option::unwrap).collect();
        assert_eq!(ranges[0].start_margin, 0.0..0.0);
        assert_eq!(ranges[0].end_margin, 95.0..105.0);
        assert_eq!(ranges[1].start_margin, 95.0..105.0);
        assert_eq!(ranges[1].end_margin, 237.5..262.5);
        assert_eq!(ranges[2].start_margin, 237.5..262.5);
        assert_eq!(ranges[2].end_margin, 475.0..525.0);
        assert_eq!(ranges[3].start_margin, 475.0..525.0);
        assert_eq!(ranges[3].end_margin.start, f32::INFINITY);

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
