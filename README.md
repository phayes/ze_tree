# ze_tree

Generate procedural trees in Rust, from small bushes to branching oaks and pines.
A port of [ez-tree](https://github.com/dgreenheck/ez-tree), `ze_tree` produces
branch and leaf meshes, repeatable shapes from a seed, and simpler meshes for
distant trees. Your application supplies the renderer.

## Generate a tree

Start with a built-in preset:

```rust
use ze_tree::{Tree, TreePreset};

let tree = Tree::from_preset(TreePreset::OakSmall);
let mesh = tree.mesh();

println!("{} triangles", tree.triangle_count());
println!("{} bark vertices", mesh.branches.positions.len());
println!("{} leaf vertices", mesh.leaves.positions.len());
```

`mesh.branches` and `mesh.leaves` contain positions, normals, texture coordinates,
and triangle indices. Upload these buffers to your renderer and give each mesh
its own material. Presets include ash, aspen, oak, pine, bushes, and a tree trained
along a trellis. `TreePreset::ALL` lists them all.

## Customize the shape

Edit a preset's options, then generate a new tree. The same options and seed
produce the same shape; change the seed for another variation.

```rust
use ze_tree::{Tree, TreePreset};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut options = TreePreset::OakSmall.options();
    options.seed = 42;
    options.branch.length[0] = 12.0; // Trunk length, in tree units.
    options.leaves.size = 1.5;

    let tree = Tree::new(options)?;
    println!("{} triangles", tree.triangle_count());
    Ok(())
}
```

Branch settings use index `0` for the trunk and indices `1` through `3` for
successive branch levels. You can also start with `TreeOptions::default()`.
`Tree::new` checks branch depth and section counts; it does not validate every
numeric setting, so use sensible dimensions and counts.

## Load and save JSON

Load full ez-tree settings or a partial JSON object. Missing fields, including
nested fields, use defaults. JSON support is always enabled.

```rust
use ze_tree::Tree;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::from_json(r#"{
        "seed": 42,
        "branch": { "length": { "0": 12.0 } }
    }"#)?;

    let json = tree.options().to_json();
    let restored = Tree::from_json(&json)?;
    assert_eq!(tree, restored);
    Ok(())
}
```

Use `TreeOptions::from_json` if you only need settings. Unlike this helper,
deserializing `TreeOptions` directly with serde requires every field.

## Create simpler meshes

Generate several levels of detail (LOD) from the same tree. Your renderer chooses
which mesh to show based on camera distance.

```rust
use ze_tree::{Tree, TreePreset, default_lod_levels};

let tree = Tree::from_preset(TreePreset::AshSmall);
for (level, mesh) in tree.lod_meshes(&default_lod_levels()) {
    println!(
        "From distance {}: {} triangles",
        level.distance,
        mesh.triangle_count(),
    );
}
```

The four default levels use progressively simpler meshes at distances of 0, 100,
250 and 500 tree units. To choose your own mesh detail, pass a `LodDetail` to
`Tree::mesh_with_detail`.
If you only need branch shapes and leaf placements, use `Skeleton::generate`
without building a mesh.

## Export a model

Optional Cargo features enable file export:

- `export-obj`: Wavefront OBJ geometry and MTL materials.
- `export-gltf`: glTF (`.gltf` and `.bin`) or a single binary `.glb` file.

With `export-gltf` enabled, save a tree as GLB:

```no_run
use ze_tree::{Tree, TreePreset};
use ze_tree::export::gltf::{GltfOptions, to_glb};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::from_preset(TreePreset::PineSmall);
    let bytes = to_glb(
        std::slice::from_ref(tree.mesh()),
        tree.options(),
        &GltfOptions::default(),
    )?;
    std::fs::write("pine.glb", bytes)?;
    Ok(())
}
```

Exporters return file contents for you to save. By default, exports use material
colours without images; leaves appear as rectangles without a texture supplying
their outline. Use `GltfOptions::textures` to reference or embed PNG/JPEG images.
An optional rig prepares a model for bending animation, but does not add animation
clips. Exported LOD meshes require the receiving tool to choose which to display.

To export every preset as OBJ and GLB from this repository:

```sh
cargo run -p ze_tree --example export --features export-obj,export-gltf
```

Files are written to `target/ze_tree_export` by default. The example accepts an
output directory and an optional source texture directory as positional arguments.

## Materials and animation

`BarkMaterial` and `LeafMaterial` describe the appearance settings for a renderer.
The repository's `assets/textures/` directory holds KTX2 textures, which are not
included in the crate. Download them with `tools/textures/download.sh`, which
fetches every leaf and the 512 and 1k bark sizes (add `--sizes 512,1k,2k` for
2k bark). Every texture comes in two formats:

- BC7 for GPUs that support BC7 compression, including most desktop GPUs.
- UASTC for a Basis Universal loader to transcode to a supported GPU format.

`bark_texture_path` and `leaf_texture_path` give each file's path inside that
directory, such as `bark/sakura_bark-1k-color-bc7.ktx2`. The options' `bark.kind`
and `leaves.kind` select the texture names; bark comes in three sizes.
`LEAF_KINDS` lists 24 leaf textures: four single leaves from ez-tree, and 20
leafy branch tips from Nobiax's CC0 packs, which also have normal maps
(`leaf_has_normal_map`). Every leaf kind also has a 256 × 256 clump image
(`LeafMap::Clump`), showing several leaves on one flat card. Use it with fewer,
larger cards for distant trees. For bark without texture images, multiply the
linear RGB colour from `bark_average_color` by the bark tint to approximate
the textured colour. `assets/textures/assets_manifest.json` lists every
file with its download address. Load colour images as sRGB and normal/roughness maps as linear data. Normal maps use the OpenGL/glTF convention; leaf alpha is straight,
not premultiplied. Keep the metallic factor at zero if using the roughness image
as a combined metallic-roughness texture.

Normal maps add lighting detail without extra geometry. Supply the directions
in `MeshBuffers::tangents` from the corresponding bark or leaf mesh to orient
that detail. For bark, keep the default `BarkUvMode::Continuous`.
`BarkUvMode::Mirrored` matches the JavaScript ez-tree generator's texture layout
but reverses normal-map lighting detail on every other branch section, so
Bevy and glTF export omit bark normal maps in that mode.

For GPU wind, pass `MeshBuffers::wind` and `WindParams` to the `WIND_WESL` shader
module and add its `wind_offset` to vertex positions. For joint-based animation,
create joints with `Tree::rig` and vertex weights with `Rig::skin`.
`BranchGrouping::parts` and `TreeMesh::part` let you extract separate tree parts.
`TrellisOptions::wires` and `TrellisMaterial` describe the optional support wires.

## Bevy integration

The workspace also includes `ze_tree_bevy`. Add its `ZeTreePlugin` after Bevy's
default plugins and spawn a `ZeTree` component to generate and display trees.
It handles background generation, materials, wind, and mesh detail switching.
See that crate's documentation for setup and texture paths.
