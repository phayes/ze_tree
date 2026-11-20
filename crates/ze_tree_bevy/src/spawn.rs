//! Generate trees in background tasks and display their parts as child entities.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use ze_tree::{
    LodLevel, OptionsError, Skeleton, TextureCompression, TextureSize, TreeError, TreeOptions,
    TreePreset, TrellisWire, default_lod_levels,
};

use crate::material::{
    BarkTextures, LeafTextures, TreeMaterial, bark_material, flower_material, fruit_material,
    leaf_material, trellis_material,
};
use crate::mesh::{bark_mesh, cluster_mesh, leaf_mesh};
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

/// One detail level for [`ZeTreePlugin::lod_levels`](crate::ZeTreePlugin::lod_levels):
/// the mesh detail and switching distance, plus the bark texture size to use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeLodLevel {
    /// Mesh detail settings and the camera distance where this level starts.
    pub level: LodLevel,
    /// Bark texture size for this level. Smaller sizes use less memory; the file
    /// must be present under the plugin's texture folder.
    pub bark_texture_size: TextureSize,
}

/// Returns the plugin's three default detail levels, nearest first: the
/// [`default_lod_levels`] meshes with 1k bark textures up close and 512 bark
/// textures at the two farther levels.
pub fn default_tree_lod_levels() -> Vec<TreeLodLevel> {
    let sizes = [TextureSize::Size1k, TextureSize::Size512, TextureSize::Size512];
    default_lod_levels()
        .into_iter()
        .zip(sizes)
        .map(|(level, bark_texture_size)| TreeLodLevel {
            level,
            bark_texture_size,
        })
        .collect()
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

/// Meshes produced by a background task for one detail level; each is absent
/// when the level has no triangles of that kind.
struct LevelMeshes {
    level: TreeLodLevel,
    bark: Option<Mesh>,
    leaves: Option<Mesh>,
    fruit: Option<Mesh>,
    flowers: Option<Mesh>,
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
    /// Fruit mesh, absent when this level contains no fruit triangles.
    fruit: Option<Handle<Mesh>>,
    /// Flower mesh, absent when this level contains no flower triangles.
    flowers: Option<Handle<Mesh>>,
    /// Bark material for this level's texture size, shared by levels with the same size.
    bark_material: Handle<TreeMaterial>,
}

/// Meshes and materials reused by trees with identical options.
struct Generated {
    /// Detail levels sorted from the nearest switching distance to the farthest.
    levels: Vec<LevelAssets>,
    leaves: Handle<TreeMaterial>,
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

/// Caches generation tasks and results by tree options, bark textures by kind and
/// size, and leaf textures by kind.
#[derive(Resource, Default)]
pub(crate) struct TreeCache {
    trees: HashMap<TreeKey, Entry>,
    bark_textures: HashMap<(String, TextureSize), BarkTextures>,
    leaf_textures: HashMap<String, LeafTextures>,
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

/// Builds the branch structure once, then creates meshes for detail levels sorted
/// by their switching distances.
fn generate(options: &TreeOptions, levels: &[TreeLodLevel]) -> TaskOutput {
    let skeleton = Skeleton::generate(options)?;
    let mut levels = levels.to_vec();
    levels.sort_by(|a, b| a.level.distance.total_cmp(&b.level.distance));
    Ok(levels
        .into_iter()
        .map(|level| {
            let mesh = skeleton.mesh(options, &level.level.detail);
            LevelMeshes {
                level,
                bark: (!mesh.branches.indices.is_empty())
                    .then(|| bark_mesh(&mesh.branches, &options.bark)),
                leaves: (!mesh.leaves.indices.is_empty()).then(|| leaf_mesh(&mesh.leaves)),
                fruit: (!mesh.fruit.indices.is_empty()).then(|| cluster_mesh(&mesh.fruit)),
                flowers: (!mesh.flowers.indices.is_empty()).then(|| cluster_mesh(&mesh.flowers)),
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

        // Reuse leaf textures by kind, even when the trees have different options.
        let compression = settings.texture_compression;
        let leaf_textures = settings.texture_root.as_ref().map(|root| {
            cache
                .leaf_textures
                .entry(options.leaves.kind.clone())
                .or_insert_with(|| {
                    LeafTextures::load(&asset_server, root, &options.leaves.kind, compression)
                })
                .clone()
        });

        // Make one bark material per texture size; levels with the same size share it.
        // Bark textures are reused by kind and size across trees. Without textures,
        // every level shares one untextured material.
        let mut bark_materials: HashMap<Option<TextureSize>, Handle<TreeMaterial>> = HashMap::new();
        let ranges = visibility_ranges(levels.iter().map(|l| l.level.level));
        let mut level_assets = Vec::with_capacity(levels.len());
        for (l, range) in levels.into_iter().zip(ranges) {
            let size = l.level.bark_texture_size;
            let key = settings.texture_root.is_some().then_some(size);
            let bark = bark_materials.entry(key).or_insert_with(|| {
                let textures = settings.texture_root.as_ref().map(|root| {
                    cache
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
                        })
                        .clone()
                });
                tree_materials.add(bark_material(&options.bark, textures.as_ref(), wind.0))
            });
            level_assets.push(LevelAssets {
                range,
                bark: l.bark.map(|m| meshes.add(m)),
                leaves: l.leaves.map(|m| meshes.add(m)),
                fruit: l.fruit.map(|m| meshes.add(m)),
                flowers: l.flowers.map(|m| meshes.add(m)),
                bark_material: bark.clone(),
            });
        }

        let generated = Generated {
            levels: level_assets,
            leaves: tree_materials.add(leaf_material(
                &options.leaves,
                leaf_textures.as_ref(),
                wind.0,
            )),
            fruit: tree_materials.add(fruit_material(wind.0)),
            flowers: tree_materials.add(flower_material(wind.0)),
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
            (TreePartKind::Leaves, &level.leaves, &generated.leaves),
            (TreePartKind::Fruit, &level.fruit, &generated.fruit),
            (TreePartKind::Flowers, &level.flowers, &generated.flowers),
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
    use std::time::{Duration, Instant};

    use super::*;
    use ze_tree::default_lod_levels;

    /// Results of every [`TreeFinished`] event, in order.
    #[derive(Resource, Default)]
    struct Finished(Vec<Result<(), OptionsError>>);

    /// An app running only the spawn systems, without rendering or textures.
    fn spawn_app() -> App {
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
                lod_levels: vec![TreeLodLevel {
                    level: LodLevel::default(),
                    bark_texture_size: TextureSize::Size1k,
                }],
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
