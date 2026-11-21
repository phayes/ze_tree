//! Export a tree as Wavefront OBJ mesh text and an MTL material description.
//!
//! Save both returned strings to files for import into a 3D tool. This exporter
//! writes one mesh at a time, with no joints, wind attributes, or automatic detail switching.

use std::fmt::Write;

use glam::Vec3;

use super::{TextureMap, TextureRef, export_parts, scaled_bark_uvs};
use crate::{
    BarkMaterial, BranchGrouping, FlowerMaterial, FruitMaterial, LeafMaterial, MeshBuffers,
    MeshPart, TreeMesh, TreeOptions, leaf_has_normal_map,
};

/// Callback returning an image path for a requested texture, or `None` to omit it.
/// Paths are written unchanged into the material file; use paths relative to that
/// file for a portable export. The caller must supply the images separately.
pub type TexturePathFn<'a> = &'a dyn Fn(TextureRef<'_>) -> Option<String>;

/// Object grouping, material file name, and texture lookup for [`to_obj`].
#[derive(Clone, Copy)]
pub struct ObjOptions<'a> {
    /// How to separate non-trunk branches into named objects.
    /// Defaults to [`BranchGrouping::Merged`], which combines them into one object.
    pub grouping: BranchGrouping,
    /// Material file name referenced by the OBJ, defaulting to `tree.mtl`.
    /// Save [`ObjFiles::mtl`] at this path relative to the OBJ file.
    pub material_library: &'a str,
    /// Callback supplying image paths for the materials. The default, `None`,
    /// exports material colours without images. A callback may omit individual maps.
    pub textures: Option<TexturePathFn<'a>>,
}

impl Default for ObjOptions<'_> {
    fn default() -> Self {
        Self {
            grouping: BranchGrouping::default(),
            material_library: "tree.mtl",
            textures: None,
        }
    }
}

impl std::fmt::Debug for ObjOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjOptions")
            .field("grouping", &self.grouping)
            .field("material_library", &self.material_library)
            .field("textures", &self.textures.map(|_| "<fn>"))
            .finish()
    }
}

/// Mesh and material file contents ready to save, returned by [`to_obj`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjFiles {
    /// Text to save as the `.obj` mesh file.
    pub obj: String,
    /// Text to save at the path given by [`ObjOptions::material_library`].
    pub mtl: String,
}

/// Returns OBJ mesh text and its companion MTL material text without writing files.
///
/// Pass the tree's generation settings as `options` to preserve its colours,
/// texture choices, and bark texture scale. Parts become named objects such as
/// `trunk`, `branches`, and `leaves`, each using the `bark` or `leaves` material.
/// With [`BarkOptions::flat_shading`](crate::BarkOptions::flat_shading), trunk and
/// branch objects get one normal per triangle, so importers shade each face flat.
///
/// OBJ preserves fixed billboard shapes but cannot store camera-facing rotation
/// data. Parts with [`MeshBuffers::billboard`] use their stored positions; bark
/// and fruit omit normals, and leaves keep one normal per card. Crossed fallback shapes from [`TreeMesh::billboard_cull`] follow as
/// `<part>_billboard_cull` objects, such as `branches_billboard_cull`, using the same
/// material. Draw both objects to keep the fixed shapes visible from more angles.
///
/// Fruit and flowers are separate `fruit` and `flowers` objects. Their colours
/// are stored at points on the model using the OBJ vertex-colour extension
/// (`v x y z r g b`, with sRGB colours). Support depends on the receiving tool.
/// The companion materials are white: in Blender, connect the imported colour
/// attribute to the material to display these colours. Prefer glTF when fruit
/// and flower colours should work without this extra setup.
///
/// Material colours are linear RGB. Roughness (how matte a surface looks) and
/// metalness use MTL extensions `Pr`, `Pm`, and `map_Pr`, which not every importer
/// supports. The leaf colour image supplies both colour and transparency (`map_d`).
/// Leaf kinds with normal maps also get a `map_Bump` reference for lighting detail.
/// Importer support varies: check that the target tool reads this as a normal map
/// and uses the colour image's alpha for transparency. Set the transparency cutoff
/// and enable visibility from both sides in that tool; OBJ does not carry those settings.
///
/// # Panics
///
/// May panic for inconsistent, manually edited mesh buffers; see [`TreeMesh::part`].
pub fn to_obj(mesh: &TreeMesh, options: &TreeOptions, obj: &ObjOptions<'_>) -> ObjFiles {
    let mut out = String::new();
    out.push_str("# Tree exported by ze_tree\n");
    let _ = writeln!(out, "mtllib {}", obj.material_library);

    // OBJ numbers positions and normals from 1 across all objects, so retain the
    // preceding counts.
    let mut written = 0;
    let mut normals_written = 0;
    let parts = export_parts(obj.grouping, mesh);
    for &part in &parts {
        let material = match part {
            MeshPart::Leaves => "leaves",
            MeshPart::Fruit => "fruit",
            MeshPart::Flowers => "flowers",
            MeshPart::Trunk | MeshPart::Branches | MeshPart::BranchLevel(_) => "bark",
        };
        let flat = part.is_bark() && options.bark.flat_shading;

        // Write the main geometry followed by any crossed fallback shapes.
        let cull = mesh.billboard_cull_part(part).buffers;
        let cull = (!cull.indices.is_empty()).then_some(cull);
        let groups = [Some(mesh.part(part).buffers), cull];
        for (k, mut buffers) in groups.into_iter().flatten().enumerate() {
            if part.is_bark() {
                buffers.uvs = scaled_bark_uvs(&buffers.uvs, options);
            }
            let suffix = if k == 0 { "" } else { "_billboard_cull" };
            let _ = writeln!(out, "o {}{suffix}", part.name());
            normals_written +=
                write_part(&mut out, &buffers, material, written, normals_written, flat);
            written += buffers.positions.len();
        }
    }

    ObjFiles {
        obj: out,
        mtl: material_library(options, obj.textures, &parts),
    }
}

/// Appends a part’s vertices and faces with the named material, returning the
/// number of normals written. Pass the counts of previously written positions
/// and normals as `offset` and `normal_offset`. If normals are present, `flat`
/// replaces them with one normal per triangle. Buffers without normals, including
/// billboard shapes, remain without normals even when `flat` is enabled.
fn write_part(
    out: &mut String,
    buffers: &MeshBuffers,
    material: &str,
    offset: usize,
    normal_offset: usize,
    flat: bool,
) -> usize {
    // For parts with vertex colours, append sRGB red, green and blue values
    // to each vertex position using the OBJ colour extension.
    for (i, [x, y, z]) in buffers.positions.iter().enumerate() {
        match buffers.colors.get(i) {
            Some(&[r, g, b, _]) => {
                let [r, g, b] = [r, g, b].map(linear_to_srgb);
                let _ = writeln!(out, "v {x} {y} {z} {r} {g} {b}");
            }
            None => {
                let _ = writeln!(out, "v {x} {y} {z}");
            }
        }
    }
    for [u, v] in &buffers.uvs {
        let _ = writeln!(out, "vt {u} {v}");
    }
    let triangles = buffers.indices.as_chunks::<3>().0;
    let lit = !buffers.normals.is_empty();
    let normals = match (lit, flat) {
        (false, _) => Vec::new(),
        (true, true) => triangles.iter().map(|t| face_normal(buffers, t)).collect(),
        (true, false) => buffers.normals.clone(),
    };
    for [x, y, z] in &normals {
        let _ = writeln!(out, "vn {x} {y} {z}");
    }

    // Positions and texture coordinates share a vertex index. Normals use the
    // vertex index for smooth parts and the triangle index for flat ones.
    // Smoothing groups tell importers which parts to shade smooth.
    let _ = writeln!(out, "usemtl {material}");
    let _ = writeln!(out, "s {}", if flat || !lit { "off" } else { "1" });
    for (t, triangle) in triangles.iter().enumerate() {
        out.push('f');
        for &i in triangle {
            let v = i as usize + offset + 1;
            if lit {
                let n = if flat { t } else { i as usize } + normal_offset + 1;
                let _ = write!(out, " {v}/{v}/{n}");
            } else {
                let _ = write!(out, " {v}/{v}");
            }
        }
        out.push('\n');
    }
    normals.len()
}

/// Returns the unit normal of a counter-clockwise triangle, or its first vertex's
/// normal if the triangle has no area.
fn face_normal(buffers: &MeshBuffers, triangle: &[u32; 3]) -> [f32; 3] {
    let [a, b, c] = triangle.map(|i| Vec3::from_array(buffers.positions[i as usize]));
    let fallback = Vec3::from_array(buffers.normals[triangle[0] as usize]);
    (b - a).cross(c - a).normalize_or(fallback).to_array()
}

/// Converts one colour component from linear values to sRGB for OBJ vertex
/// colours, clamping it to the supported 0–1 range first.
fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Creates the material file with bark and leaf colours and available image paths.
/// Adds neutral white materials for fruit and flower parts that are present,
/// so their vertex colours are not tinted. Skips bark images when texturing is off.
fn material_library(
    options: &TreeOptions,
    textures: Option<TexturePathFn<'_>>,
    parts: &[MeshPart],
) -> String {
    let bark = BarkMaterial::new(&options.bark);
    let leaf = LeafMaterial::new(&options.leaves);
    let path = |kind: &str, map| textures.and_then(|f| f(TextureRef { kind, map }));
    let mut out = String::from("# Materials exported by ze_tree\n");

    // Write bark colour and surface properties before its optional image paths.
    let [r, g, b] = bark.base_color;
    let _ = write!(
        out,
        "\nnewmtl bark\nKd {r} {g} {b}\nKs 0 0 0\nPr {}\nPm {}\n",
        bark.roughness, bark.metallic
    );
    if bark.textured {
        let kind = options.bark.kind.as_str();
        if let Some(p) = path(kind, TextureMap::BarkColor) {
            let _ = writeln!(out, "map_Kd {p}");
        }
        if let Some(p) = path(kind, TextureMap::BarkNormal) {
            let _ = writeln!(out, "map_Bump {p}");
        }
        if let Some(p) = path(kind, TextureMap::BarkRoughness) {
            let _ = writeln!(out, "map_Pr {p}");
        }
    }

    // Reference the leaf image for both colour and transparency.
    let [r, g, b] = leaf.base_color;
    let _ = write!(
        out,
        "\nnewmtl leaves\nKd {r} {g} {b}\nKs 0 0 0\nPr {}\nPm {}\n",
        leaf.roughness, leaf.metallic
    );
    let kind = options.leaves.kind.as_str();
    if let Some(p) = path(kind, TextureMap::LeafColor) {
        let _ = writeln!(out, "map_Kd {p}\nmap_d {p}");
    }
    if leaf_has_normal_map(kind)
        && let Some(p) = path(kind, TextureMap::LeafNormal)
    {
        let _ = writeln!(out, "map_Bump {p}");
    }

    // Use white materials to avoid tinting fruit and flower vertex colours
    // when the receiving application uses those colours.
    let fruit = FruitMaterial::default();
    let flower = FlowerMaterial::default();
    for (part, name, roughness, metallic) in [
        (MeshPart::Fruit, "fruit", fruit.roughness, fruit.metallic),
        (
            MeshPart::Flowers,
            "flowers",
            flower.roughness,
            flower.metallic,
        ),
    ] {
        if parts.contains(&part) {
            let _ = write!(
                out,
                "\nnewmtl {name}\nKd 1 1 1\nKs 0 0 0\nPr {roughness}\nPm {metallic}\n"
            );
        }
    }
    out
}
