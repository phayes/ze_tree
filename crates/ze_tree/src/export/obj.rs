//! Export a tree as Wavefront OBJ mesh text and an MTL material description.
//!
//! Save both returned strings to files for import into a 3D tool. This exporter
//! writes one mesh at a time, with no joints, wind attributes, or automatic detail switching.

use std::fmt::Write;

use glam::Vec3;

use super::{TextureMap, TextureRef, scaled_bark_uvs};
use crate::{BarkMaterial, BranchGrouping, LeafMaterial, MeshBuffers, TreeMesh, TreeOptions};

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
/// Material colours are linear RGB. Roughness (how matte a surface looks) and
/// metalness use MTL extensions `Pr`, `Pm`, and `map_Pr`, which not every importer
/// supports. The leaf image is referenced for both colour and transparency
/// (`map_d`). Set leaf transparency thresholds and visibility from both sides in
/// the target tool; these settings are not included in the OBJ export.
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
    for part in obj.grouping.parts(mesh) {
        let mut buffers = mesh.part(part).buffers;
        let material = if part.is_leaves() {
            "leaves"
        } else {
            buffers.uvs = scaled_bark_uvs(&buffers.uvs, options);
            "bark"
        };
        let flat = !part.is_leaves() && options.bark.flat_shading;
        let _ = writeln!(out, "o {}", part.name());
        normals_written += write_part(&mut out, &buffers, material, written, normals_written, flat);
        written += buffers.positions.len();
    }

    ObjFiles {
        obj: out,
        mtl: material_library(options, obj.textures),
    }
}

/// Writes one part's vertex attributes and triangles using the named material.
/// `offset` and `normal_offset` count positions and normals already written.
/// With `flat`, each triangle gets one face normal instead of the vertex normals.
/// Returns the number of normals written.
fn write_part(
    out: &mut String,
    buffers: &MeshBuffers,
    material: &str,
    offset: usize,
    normal_offset: usize,
    flat: bool,
) -> usize {
    for [x, y, z] in &buffers.positions {
        let _ = writeln!(out, "v {x} {y} {z}");
    }
    for [u, v] in &buffers.uvs {
        let _ = writeln!(out, "vt {u} {v}");
    }
    let triangles = buffers.indices.as_chunks::<3>().0;
    let normals = if flat {
        triangles.iter().map(|t| face_normal(buffers, t)).collect()
    } else {
        buffers.normals.clone()
    };
    for [x, y, z] in &normals {
        let _ = writeln!(out, "vn {x} {y} {z}");
    }

    // Positions and texture coordinates share a vertex index. Normals use the
    // vertex index for smooth parts and the triangle index for flat ones.
    // Smoothing groups tell importers which parts to shade smooth.
    let _ = writeln!(out, "usemtl {material}");
    let _ = writeln!(out, "s {}", if flat { "off" } else { "1" });
    for (t, triangle) in triangles.iter().enumerate() {
        out.push('f');
        for &i in triangle {
            let v = i as usize + offset + 1;
            let n = if flat { t } else { i as usize } + normal_offset + 1;
            let _ = write!(out, " {v}/{v}/{n}");
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

/// Builds the bark and leaf material text, including any paths returned by `textures`.
/// Does not request bark images when bark texturing is disabled.
fn material_library(options: &TreeOptions, textures: Option<TexturePathFn<'_>>) -> String {
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
    if let Some(p) = path(&options.leaves.kind, TextureMap::Leaf) {
        let _ = writeln!(out, "map_Kd {p}\nmap_d {p}");
    }
    out
}
