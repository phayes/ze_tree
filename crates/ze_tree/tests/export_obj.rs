//! Check that OBJ mesh text preserves geometry and that MTL material text names the
//! requested images. Also check part names and bark texture repetition.
#![cfg(feature = "export-obj")]

use glam::Vec3;
use ze_tree::export::obj::{ObjOptions, to_obj};
use ze_tree::export::{TextureMap, TextureRef};
use ze_tree::{BranchGrouping, MeshPart, Tree, TreePreset};

/// Counts lines whose first space-separated field equals `prefix`.
fn count(text: &str, prefix: &str) -> usize {
    text.lines()
        .filter(|l| l.split(' ').next() == Some(prefix))
        .count()
}

#[test]
fn obj_contains_every_vertex_and_triangle() {
    let tree = Tree::from_preset(TreePreset::OakSmall);
    let mesh = tree.mesh();
    let files = to_obj(mesh, tree.options(), &ObjOptions::default());
    let obj = &files.obj;

    assert!(obj.contains("mtllib tree.mtl\n"));
    assert_eq!(count(obj, "v"), mesh.vertex_count());
    assert_eq!(count(obj, "vt"), mesh.vertex_count());
    assert_eq!(count(obj, "vn"), mesh.vertex_count());
    assert_eq!(count(obj, "f"), mesh.triangle_count());

    // Default grouping separates trunk, combined branches, and leaves with matching materials.
    let objects: Vec<&str> = obj.lines().filter_map(|l| l.strip_prefix("o ")).collect();
    assert_eq!(objects, ["trunk", "branches", "leaves"]);
    let materials: Vec<&str> = obj
        .lines()
        .filter_map(|l| l.strip_prefix("usemtl "))
        .collect();
    assert_eq!(materials, ["bark", "bark", "leaves"]);

    // Each triangle corner must use matching, in-range position, UV, and normal indices.
    let vertices = mesh.vertex_count();
    for line in obj.lines().filter(|l| l.starts_with("f ")) {
        for corner in line.split(' ').skip(1) {
            let parts: Vec<usize> = corner.split('/').map(|p| p.parse().unwrap()).collect();
            assert_eq!(parts.len(), 3);
            assert!(parts.iter().all(|&i| i == parts[0]));
            assert!((1..=vertices).contains(&parts[0]), "{corner}");
        }
    }
}

#[test]
fn per_level_grouping_names_each_level() {
    let tree = Tree::from_preset(TreePreset::OakSmall);
    let options = ObjOptions {
        grouping: BranchGrouping::PerLevel,
        material_library: "oak.mtl",
        ..ObjOptions::default()
    };
    let files = to_obj(tree.mesh(), tree.options(), &options);
    let objects: Vec<&str> = files
        .obj
        .lines()
        .filter_map(|l| l.strip_prefix("o "))
        .collect();
    assert_eq!(
        objects,
        ["trunk", "branches_1", "branches_2", "branches_3", "leaves"]
    );
    assert!(files.obj.contains("mtllib oak.mtl\n"));
}

#[test]
fn bark_texture_coordinates_include_the_texture_scale() {
    let mut options = TreePreset::AshSmall.options();
    options.bark.texture_scale.y = 4.0;
    let tree = Tree::new(options).unwrap();
    let files = to_obj(tree.mesh(), tree.options(), &ObjOptions::default());

    // The first texture coordinate records belong to the trunk, which is exported first.
    let trunk = tree.mesh().part(MeshPart::Trunk).buffers;
    let vts: Vec<[f32; 2]> = files
        .obj
        .lines()
        .filter_map(|l| l.strip_prefix("vt "))
        .take(trunk.uvs.len())
        .map(|l| {
            let mut it = l.split(' ').map(|n| n.parse::<f32>().unwrap());
            [it.next().unwrap(), it.next().unwrap()]
        })
        .collect();
    for (written, original) in vts.iter().zip(&trunk.uvs) {
        assert_eq!(*written, [original[0], original[1] / 4.0]);
    }
}

#[test]
fn mtl_lists_textures_from_the_path_function() {
    let tree = Tree::from_preset(TreePreset::AshSmall);
    let paths = |t: TextureRef<'_>| match t.map {
        TextureMap::BarkColor => Some(format!("{}_color.jpg", t.kind)),
        TextureMap::BarkNormal => Some(format!("{}_normal.jpg", t.kind)),
        TextureMap::BarkRoughness => None,
        TextureMap::Leaf => Some(format!("{}.png", t.kind)),
    };
    let options = ObjOptions {
        textures: Some(&paths),
        ..ObjOptions::default()
    };
    let mtl = to_obj(tree.mesh(), tree.options(), &options).mtl;

    let bark = &tree.options().bark.kind;
    let leaf = &tree.options().leaves.kind;
    assert!(mtl.contains("newmtl bark\n"));
    assert!(mtl.contains(&format!("map_Kd {bark}_color.jpg\n")));
    assert!(mtl.contains(&format!("map_Bump {bark}_normal.jpg\n")));
    assert!(!mtl.contains("map_Pr"));
    assert!(mtl.contains("newmtl leaves\n"));
    assert!(mtl.contains(&format!("map_Kd {leaf}.png\nmap_d {leaf}.png\n")));

    // Omitting the image callback must retain colours without emitting texture paths.
    let plain = to_obj(tree.mesh(), tree.options(), &ObjOptions::default()).mtl;
    assert_eq!(count(&plain, "Kd"), 2);
    assert!(!plain.contains("map_"));
}

/// Flat-shaded bark gets one outward normal per triangle; leaves keep vertex normals.
#[test]
fn flat_shaded_bark_gets_face_normals() {
    let mut options = TreePreset::OakSmall.options();
    options.bark.flat_shading = true;
    let tree = Tree::new(options).unwrap();
    let mesh = tree.mesh();
    let files = to_obj(mesh, tree.options(), &ObjOptions::default());
    let obj = &files.obj;

    let bark = mesh.branches.vertex_count();
    let bark_triangles = mesh.branches.indices.len() / 3;
    assert_eq!(count(obj, "v"), mesh.vertex_count());
    assert_eq!(
        count(obj, "vn"),
        bark_triangles + mesh.leaves.vertex_count()
    );
    let smoothing: Vec<&str> = obj.lines().filter_map(|l| l.strip_prefix("s ")).collect();
    assert_eq!(smoothing, ["off", "off", "1"]);
    let normals: Vec<Vec3> = obj
        .lines()
        .filter_map(|l| l.strip_prefix("vn "))
        .map(|l| {
            let v: Vec<f32> = l.split(' ').map(|x| x.parse().unwrap()).collect();
            Vec3::new(v[0], v[1], v[2])
        })
        .collect();

    // Bark triangles share one normal per face, facing the same way as the
    // smooth vertex normals. Leaf vertex normals are numbered after the face normals.
    let mut triangle = 0;
    for line in obj.lines() {
        let Some(corners) = line.strip_prefix("f ") else {
            continue;
        };
        let corners: Vec<Vec<usize>> = corners
            .split(' ')
            .map(|c| c.split('/').map(|p| p.parse().unwrap()).collect())
            .collect();
        for c in &corners {
            assert_eq!((c.len(), c[0]), (3, c[1]), "{line}");
        }
        if triangle < bark_triangles {
            let face = normals[triangle];
            assert!((face.length() - 1.0).abs() < 1e-5, "{line}");
            for c in &corners {
                assert_eq!(c[2], triangle + 1, "{line}");
                let smooth = Vec3::from_array(mesh.branches.normals[c[0] - 1]);
                assert!(face.dot(smooth) > 0.0, "{line}");
            }
        } else {
            for c in &corners {
                assert_eq!(c[2], bark_triangles + c[0] - bark, "{line}");
            }
        }
        triangle += 1;
    }
}
