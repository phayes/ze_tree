//! Read exported scenes with the `gltf` crate to check geometry, materials, texture
//! references, joint assignments, detail-level names, and saved generation settings.
#![cfg(feature = "export-gltf")]

use std::collections::BTreeSet;

use gltf::Gltf;
use gltf::mesh::Semantic;
use ze_tree::export::gltf::{
    EmbeddedImage, GltfError, GltfOptions, GltfTextures, ImageFormat, to_glb, to_gltf,
};
use ze_tree::export::{TextureMap, TextureRef};
use ze_tree::{
    BranchGrouping, MeshPart, RigOptions, Tree, TreeOptions, TreePreset, default_lod_levels,
};

fn read(glb: &[u8]) -> Gltf {
    Gltf::from_slice(glb).expect("the exported file is valid glTF")
}

/// glTF reads a normal map's green channel along cross(normal, tangent) * w, which
/// must point toward the top of the image: the direction of decreasing glTF v.
#[test]
fn bark_bitangents_point_toward_the_top_of_the_image() {
    let mut options = TreePreset::OakSmall.options();
    options.bark.uv_mode = ze_tree::BarkUvMode::Continuous;
    let tree = Tree::new(options).unwrap();
    let glb = to_glb(
        std::slice::from_ref(tree.mesh()),
        tree.options(),
        &GltfOptions::default(),
    )
    .unwrap();
    let gltf = read(&glb);
    let trunk = gltf.meshes().next().unwrap().primitives().next().unwrap();
    let reader = trunk.reader(|_| gltf.blob.as_deref());
    let positions: Vec<glam::Vec3> = reader
        .read_positions()
        .unwrap()
        .map(glam::Vec3::from)
        .collect();
    let normals: Vec<glam::Vec3> = reader
        .read_normals()
        .unwrap()
        .map(glam::Vec3::from)
        .collect();
    let tangents: Vec<[f32; 4]> = reader.read_tangents().unwrap().collect();
    let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).unwrap().into_f32().collect();

    // Compare each trunk vertex with the matching vertex one ring further along.
    let ring = tree.mesh().branch_spans[0].segments as usize + 1;
    for i in 0..positions.len() - ring {
        let [x, y, z, w] = tangents[i];
        let bitangent = normals[i].cross(glam::Vec3::new(x, y, z)) * w;
        let along = positions[i + ring] - positions[i];
        assert!(uvs[i + ring][1] < uvs[i][1], "v decreases along the trunk");
        assert!(
            bitangent.dot(along) > 0.0,
            "vertex {i}: bitangent points away from image top"
        );
    }
}

fn mesh_names(gltf: &Gltf) -> Vec<String> {
    gltf.meshes()
        .map(|m| m.name().unwrap().to_owned())
        .collect()
}

#[test]
fn glb_holds_each_part_with_its_vertex_data() {
    let tree = Tree::from_preset(TreePreset::OakSmall);
    let glb = to_glb(
        std::slice::from_ref(tree.mesh()),
        tree.options(),
        &GltfOptions::default(),
    )
    .unwrap();
    assert_eq!(&glb[..4], b"glTF");
    assert_eq!(glb.len() % 4, 0);
    let gltf = read(&glb);
    let blob = gltf.blob.as_deref();

    assert_eq!(mesh_names(&gltf), ["trunk", "branches", "leaves"]);
    let scene = gltf.default_scene().unwrap();
    assert_eq!(scene.nodes().count(), 3);

    for (mesh, part) in gltf
        .meshes()
        .zip([MeshPart::Trunk, MeshPart::Branches, MeshPart::Leaves])
    {
        let expected = tree.mesh().part(part).buffers;
        let primitive = mesh.primitives().next().unwrap();
        let reader = primitive.reader(|_| blob);

        let positions: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
        assert_eq!(positions, expected.positions);
        let normals: Vec<[f32; 3]> = reader.read_normals().unwrap().collect();
        assert_eq!(normals, expected.normals);
        let indices: Vec<u32> = reader.read_indices().unwrap().into_u32().collect();
        assert_eq!(indices, expected.indices);
        let wind = primitive.get(&Semantic::Extras("WIND".to_owned())).unwrap();
        assert_eq!(wind.count(), expected.positions.len());

        // Check the lower bound against the smallest position on each axis.
        let bounds = primitive.bounding_box();
        for axis in 0..3 {
            let min = positions
                .iter()
                .map(|p| p[axis])
                .fold(f32::INFINITY, f32::min);
            assert_eq!(bounds.min[axis], min);
        }

        // Both bark and leaves must retain the tangents used for normal-map lighting.
        // Leaves also need an alpha cutoff to hide pixels outside their outline.
        let material = primitive.material();
        if part.is_leaves() {
            let tangents: Vec<[f32; 4]> = reader.read_tangents().unwrap().collect();
            assert_eq!(tangents, expected.tangents);
            assert_eq!(material.name(), Some("leaves"));
            assert_eq!(material.alpha_mode(), gltf::material::AlphaMode::Mask);
            assert_eq!(
                material.alpha_cutoff(),
                Some(tree.options().leaves.alpha_test)
            );
            assert!(material.double_sided());
            // The exported vertical coordinate must account for glTF's image origin.
            let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).unwrap().into_f32().collect();
            for (uv, original) in uvs.iter().zip(&expected.uvs) {
                assert_eq!(*uv, [original[0], 1.0 - original[1]]);
            }
        } else {
            let tangents: Vec<[f32; 4]> = reader.read_tangents().unwrap().collect();
            assert_eq!(tangents, expected.tangents);
            assert_eq!(material.name(), Some("bark"));
            assert_eq!(material.alpha_mode(), gltf::material::AlphaMode::Opaque);
        }
    }
}

#[test]
fn bark_texture_coordinates_include_the_texture_scale() {
    let mut options = TreePreset::AshSmall.options();
    options.bark.texture_scale.y = 4.0;
    let tree = Tree::new(options).unwrap();
    let glb = to_glb(
        std::slice::from_ref(tree.mesh()),
        tree.options(),
        &GltfOptions::default(),
    )
    .unwrap();
    let gltf = read(&glb);
    let trunk = gltf.meshes().next().unwrap().primitives().next().unwrap();
    let uvs: Vec<[f32; 2]> = trunk
        .reader(|_| gltf.blob.as_deref())
        .read_tex_coords(0)
        .unwrap()
        .into_f32()
        .collect();
    let original = tree.mesh().part(MeshPart::Trunk).buffers.uvs;
    for (uv, original) in uvs.iter().zip(&original) {
        assert_eq!(*uv, [original[0], 1.0 - original[1] / 4.0]);
    }
}

#[test]
fn settings_are_stored_in_the_file() {
    let tree = Tree::from_preset(TreePreset::PineSmall);
    let glb = to_glb(
        std::slice::from_ref(tree.mesh()),
        tree.options(),
        &GltfOptions::default(),
    )
    .unwrap();
    let gltf = read(&glb);
    let asset = &gltf.document.as_json().asset;
    assert!(asset.generator.as_deref().unwrap().starts_with("ze_tree "));

    let extras: serde_json::Value =
        serde_json::from_str(asset.extras.as_ref().unwrap().get()).unwrap();
    let stored = TreeOptions::from_json(&extras["treeOptions"].to_string()).unwrap();
    assert_eq!(&stored, tree.options());
}

#[test]
fn skin_matches_the_rig() {
    let tree = Tree::from_preset(TreePreset::AspenSmall);
    let rig = tree.rig(&RigOptions::default()).unwrap();
    let options = GltfOptions {
        rig: Some(&rig),
        grouping: BranchGrouping::PerLevel,
        ..GltfOptions::default()
    };
    let glb = to_glb(std::slice::from_ref(tree.mesh()), tree.options(), &options).unwrap();
    let gltf = read(&glb);
    let blob = gltf.blob.as_deref();

    // The exported skin must contain every rig joint, with nodes preserving parent links.
    let skin = gltf.skins().next().unwrap();
    let joints: Vec<gltf::Node> = skin.joints().collect();
    assert_eq!(joints.len(), rig.joint_count());
    for (node, joint) in joints.iter().zip(rig.joints()) {
        let (translation, rotation, _) = node.transform().decomposed();
        assert_eq!(translation, joint.translation.to_array());
        assert_eq!(rotation, joint.rotation.to_array());
        let children: BTreeSet<usize> = node.children().map(|c| c.index()).collect();
        let expected: BTreeSet<usize> = rig
            .joints()
            .iter()
            .enumerate()
            .filter(|(_, j)| j.parent.map(|p| joints[p].index()) == Some(node.index()))
            .map(|(i, _)| joints[i].index())
            .collect();
        assert_eq!(children, expected);
    }
    let matrices: Vec<[[f32; 4]; 4]> = skin
        .reader(|_| blob)
        .read_inverse_bind_matrices()
        .unwrap()
        .collect();
    for (m, joint) in matrices.iter().zip(rig.joints()) {
        assert_eq!(*m, joint.inverse_bind.to_cols_array_2d());
    }
    assert_eq!(skin.skeleton().unwrap().index(), joints[0].index());

    // Mesh nodes and the root joint must be directly reachable from the scene.
    let scene: Vec<gltf::Node> = gltf.default_scene().unwrap().nodes().collect();
    assert_eq!(scene[0].index(), joints[0].index());
    let mesh_nodes = &scene[1..];
    assert!(
        mesh_nodes
            .iter()
            .all(|n| n.skin().is_some() && n.mesh().is_some())
    );

    // Splitting a mesh into parts must preserve each original vertex's joint assignments.
    let weights = rig.skin(tree.mesh());
    for node in mesh_nodes {
        let mesh = node.mesh().unwrap();
        let part = match mesh.name().unwrap() {
            "trunk" => MeshPart::Trunk,
            "leaves" => MeshPart::Leaves,
            name => MeshPart::BranchLevel(name.strip_prefix("branches_").unwrap().parse().unwrap()),
        };
        let source = tree.mesh().part(part).source_vertices;
        let expected = if part.is_leaves() {
            &weights.leaves
        } else {
            &weights.branches
        };
        let primitive = mesh.primitives().next().unwrap();
        let reader = primitive.reader(|_| blob);
        let read_joints: Vec<[u16; 4]> = reader.read_joints(0).unwrap().into_u16().collect();
        let read_weights: Vec<[f32; 4]> = reader.read_weights(0).unwrap().into_f32().collect();
        for (k, &v) in source.iter().enumerate() {
            assert_eq!(read_joints[k], expected.joints[v as usize]);
            assert_eq!(read_weights[k], expected.weights[v as usize]);
        }
    }
}

#[test]
fn levels_of_detail_get_suffixed_names() {
    let tree = Tree::from_preset(TreePreset::AshMedium);
    let rig = tree.rig(&RigOptions::default()).unwrap();
    let meshes: Vec<_> = tree
        .lod_meshes(&default_lod_levels())
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    let options = GltfOptions {
        rig: Some(&rig),
        wind: false,
        ..GltfOptions::default()
    };
    let gltf = read(&to_glb(&meshes, tree.options(), &options).unwrap());

    let mut expected = Vec::new();
    for lod in 0..3 {
        for part in ["trunk", "branches", "leaves"] {
            expected.push(format!("{part}_LOD{lod}"));
        }
    }
    assert_eq!(mesh_names(&gltf), expected);
    // Detail variants reuse one joint set, and disabling wind must omit its attribute.
    assert_eq!(gltf.skins().count(), 1);
    for mesh in gltf.meshes() {
        let primitive = mesh.primitives().next().unwrap();
        assert!(
            primitive
                .get(&Semantic::Extras("WIND".to_owned()))
                .is_none()
        );
    }
}

#[test]
fn embedded_and_referenced_textures() {
    let tree = Tree::from_preset(TreePreset::OakSmall);
    let mesh = std::slice::from_ref(tree.mesh());
    let embed = |t: TextureRef<'_>| {
        let format = if t.map == TextureMap::LeafColor {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        };
        Some(EmbeddedImage {
            bytes: format!("{}:{:?}", t.kind, t.map).into_bytes(),
            format,
        })
    };
    let options = GltfOptions {
        textures: GltfTextures::Embed(&embed),
        ..GltfOptions::default()
    };
    let gltf = read(&to_glb(mesh, tree.options(), &options).unwrap());
    let blob = gltf.blob.as_deref().unwrap();

    // Check that each placeholder image payload and declared format survive export.
    let bark = &tree.options().bark.kind;
    let leaf = &tree.options().leaves.kind;
    let images: Vec<(String, &str)> = gltf
        .images()
        .map(|image| match image.source() {
            gltf::image::Source::View { view, mime_type } => {
                let bytes = &blob[view.offset()..view.offset() + view.length()];
                (String::from_utf8(bytes.to_vec()).unwrap(), mime_type)
            }
            gltf::image::Source::Uri { .. } => panic!("expected an embedded image"),
        })
        .collect();
    assert_eq!(
        images,
        [
            (format!("{bark}:BarkColor"), "image/jpeg"),
            (format!("{bark}:BarkRoughness"), "image/jpeg"),
            (format!("{bark}:BarkNormal"), "image/jpeg"),
            (format!("{leaf}:LeafColor"), "image/png"),
        ]
    );
    // The colour-only "oak" texture must not create a leaf normal-map reference.
    assert!(gltf.materials().nth(1).unwrap().normal_texture().is_none());

    // Check the image assigned to each material input and the leaf edge sampling mode.
    let materials: Vec<gltf::Material> = gltf.materials().collect();
    let pbr = materials[0].pbr_metallic_roughness();
    assert_eq!(
        pbr.base_color_texture().unwrap().texture().name(),
        Some(&*format!("{bark}_color"))
    );
    assert_eq!(
        pbr.metallic_roughness_texture().unwrap().texture().name(),
        Some(&*format!("{bark}_roughness"))
    );
    assert_eq!(
        materials[0].normal_texture().unwrap().texture().name(),
        Some(&*format!("{bark}_normal"))
    );
    let leaf_texture = materials[1]
        .pbr_metallic_roughness()
        .base_color_texture()
        .unwrap()
        .texture();
    assert_eq!(
        leaf_texture.sampler().wrap_s(),
        gltf::texture::WrappingMode::ClampToEdge
    );

    // External image URIs must be preserved; disabling bark textures leaves only the leaf image.
    let mut untextured = tree.options().clone();
    untextured.bark.textured = false;
    let uri = |t: TextureRef<'_>| Some(format!("textures/{}.png", t.kind));
    let options = GltfOptions {
        textures: GltfTextures::Uri(&uri),
        ..GltfOptions::default()
    };
    let gltf = read(&to_glb(mesh, &untextured, &options).unwrap());
    let uris: Vec<String> = gltf
        .images()
        .map(|image| match image.source() {
            gltf::image::Source::Uri { uri, .. } => uri.to_owned(),
            gltf::image::Source::View { .. } => panic!("expected a referenced image"),
        })
        .collect();
    assert_eq!(uris, [format!("textures/{leaf}.png")]);
}

/// Checks that a leaf kind with a normal map exports both the material's lighting
/// texture and one tangent per leaf vertex to orient that lighting detail.
#[test]
fn leaf_normal_maps_and_tangents() {
    let mut options = TreePreset::OakSmall.options();
    options.leaves.kind = "white_oak".to_owned();
    let tree = Tree::new(options).unwrap();
    let uri = |t: TextureRef<'_>| Some(format!("{}:{:?}", t.kind, t.map));
    let gltf_options = GltfOptions {
        textures: GltfTextures::Uri(&uri),
        ..GltfOptions::default()
    };
    let gltf = read(
        &to_glb(
            std::slice::from_ref(tree.mesh()),
            tree.options(),
            &gltf_options,
        )
        .unwrap(),
    );

    let leaves = gltf.materials().find(|m| m.name() == Some("leaves")).unwrap();
    let normal = leaves.normal_texture().unwrap().texture();
    assert_eq!(normal.name(), Some("white_oak_normal"));
    assert!(matches!(
        normal.source().source(),
        gltf::image::Source::Uri { uri: "white_oak:LeafNormal", .. }
    ));

    let leaf_mesh = gltf.meshes().find(|m| m.name() == Some("leaves")).unwrap();
    let primitive = leaf_mesh.primitives().next().unwrap();
    let tangents = primitive.get(&Semantic::Tangents).unwrap();
    assert_eq!(tangents.count(), tree.mesh().leaves.vertex_count());
}

#[test]
fn gltf_with_separate_binary_file() {
    let tree = Tree::from_preset(TreePreset::Bush1);
    let files = to_gltf(
        std::slice::from_ref(tree.mesh()),
        tree.options(),
        &GltfOptions::default(),
        "bush.bin",
    )
    .unwrap();
    let gltf = Gltf::from_slice(files.json.as_bytes()).unwrap();
    let buffer = gltf.buffers().next().unwrap();
    assert_eq!(buffer.length(), files.bin.len());
    assert!(matches!(
        buffer.source(),
        gltf::buffer::Source::Uri("bush.bin")
    ));

    // The JSON's geometry records must read correctly from the returned binary buffer.
    let leaves = gltf.meshes().last().unwrap();
    let primitive = leaves.primitives().next().unwrap();
    let reader = primitive.reader(|_| Some(&files.bin));
    assert_eq!(
        reader.read_positions().unwrap().count(),
        tree.mesh().leaves.positions.len()
    );
}

#[test]
fn no_meshes_is_an_error() {
    let options = TreeOptions::default();
    assert!(matches!(
        to_glb(&[], &options, &GltfOptions::default()),
        Err(GltfError::NoMeshes)
    ));
}

/// Checks that flat-shaded bark omits normals and tangents so viewers calculate
/// a normal for each triangle. Leaf normals and tangents must remain available.
#[test]
fn flat_shaded_bark_omits_normals_and_tangents() {
    let mut options = TreePreset::OakSmall.options();
    options.bark.flat_shading = true;
    options.bark.uv_mode = ze_tree::BarkUvMode::Continuous;
    let tree = Tree::new(options).unwrap();
    let glb = to_glb(
        &[tree.mesh().clone()],
        tree.options(),
        &GltfOptions::default(),
    )
    .unwrap();
    let gltf = read(&glb);

    for mesh in gltf.meshes() {
        let primitive = mesh.primitives().next().unwrap();
        let leaves = mesh.name() == Some("leaves");
        assert_eq!(primitive.get(&Semantic::Normals).is_some(), leaves);
        assert_eq!(primitive.get(&Semantic::Tangents).is_some(), leaves);
    }
    assert_eq!(mesh_names(&gltf), ["trunk", "branches", "leaves"]);
}

/// A small tree with fruit and flowers, flat-shaded bark, and a rig to export.
fn fruiting_tree() -> Tree {
    let mut options = TreePreset::AshSmall.options();
    for (placement, cluster) in [
        (&mut options.fruit.placement, &mut options.fruit.cluster),
        (&mut options.flowers.placement, &mut options.flowers.cluster),
    ] {
        placement.count = 2;
        placement.min_level = 2;
        cluster.kind = ze_tree::ClusterKind::Umbel;
        cluster.items = 5;
    }
    options.bark.flat_shading = true;
    Tree::new(options).unwrap()
}

/// Fruit and flowers are their own parts, coloured per vertex, with smooth
/// normals even when the bark is flat-shaded, and outside the rig.
#[test]
fn fruit_and_flowers_are_exported_with_their_colours() {
    let tree = fruiting_tree();
    let rig = tree.rig(&RigOptions::default()).unwrap();
    let options = GltfOptions {
        rig: Some(&rig),
        ..GltfOptions::default()
    };
    let glb = to_glb(std::slice::from_ref(tree.mesh()), tree.options(), &options).unwrap();
    let gltf = read(&glb);
    let blob = gltf.blob.as_deref();
    assert_eq!(
        mesh_names(&gltf),
        ["trunk", "branches", "leaves", "fruit", "flowers"]
    );

    for node in gltf.nodes().filter(|n| n.mesh().is_some()) {
        let mesh = node.mesh().unwrap();
        let part = match mesh.name().unwrap() {
            "fruit" => MeshPart::Fruit,
            "flowers" => MeshPart::Flowers,
            _ => {
                // Bark and leaves stay on the rig.
                assert!(node.skin().is_some());
                continue;
            }
        };
        let expected = tree.mesh().part(part).buffers;
        let primitive = mesh.primitives().next().unwrap();
        let reader = primitive.reader(|_| blob);

        // Geometry, smooth normals and linear vertex colours come through unchanged.
        let positions: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
        assert_eq!(positions, expected.positions);
        let normals: Vec<[f32; 3]> = reader.read_normals().unwrap().collect();
        assert_eq!(normals, expected.normals);
        let colors: Vec<[f32; 4]> = reader.read_colors(0).unwrap().into_rgba_f32().collect();
        assert_eq!(colors, expected.colors);
        let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).unwrap().into_f32().collect();
        for (uv, original) in uvs.iter().zip(&expected.uvs) {
            assert_eq!(*uv, [original[0], 1.0 - original[1]]);
        }

        // A white material lets the vertex colours show; no joints are attached.
        let material = primitive.material();
        assert_eq!(material.name(), Some(mesh.name().unwrap()));
        assert_eq!(material.pbr_metallic_roughness().base_color_factor(), [1.0; 4]);
        assert!(material.double_sided());
        assert!(node.skin().is_none());
        assert!(reader.read_joints(0).is_none());
    }
}

/// A tree without fruit or flowers gets no materials for them.
#[test]
fn trees_without_fruit_get_no_fruit_material() {
    let tree = Tree::from_preset(TreePreset::OakSmall);
    let glb = to_glb(
        std::slice::from_ref(tree.mesh()),
        tree.options(),
        &GltfOptions::default(),
    )
    .unwrap();
    let names: Vec<_> = read(&glb)
        .materials()
        .map(|m| m.name().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["bark", "leaves"]);
}
