//! Verifies that mesh, material and wind conversions preserve the data Bevy needs to draw trees.

use bevy::mesh::{Mesh, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::encase::UniformBuffer;
use bevy::render::render_resource::{Face, ShaderType};
use ze_tree::{BarkMaterial, BarkUvMode, Tree, TreePreset, WindParams};
use ze_tree_bevy::{
    ATTRIBUTE_WIND, BarkTextures, LeafTextures, WindUniform, bark_material, bark_mesh,
    cluster_mesh, flower_material, fruit_material, leaf_material, leaf_mesh,
};

fn float2(mesh: &Mesh, id: impl Into<bevy::mesh::MeshVertexAttributeId>) -> Vec<[f32; 2]> {
    match mesh.attribute(id) {
        Some(VertexAttributeValues::Float32x2(values)) => values.clone(),
        other => panic!("unexpected attribute {other:?}"),
    }
}

fn float4(mesh: &Mesh, id: impl Into<bevy::mesh::MeshVertexAttributeId>) -> Vec<[f32; 4]> {
    match mesh.attribute(id) {
        Some(VertexAttributeValues::Float32x4(values)) => values.clone(),
        other => panic!("unexpected attribute {other:?}"),
    }
}

#[test]
fn bark_mesh_keeps_vertices_and_flips_scaled_texture_coordinates() {
    let mut options = TreePreset::OakSmall.options();
    options.bark.uv_mode = BarkUvMode::Continuous;
    options.bark.texture_scale.y = 4.0;
    let tree = Tree::new(options).unwrap();
    let bark = &tree.mesh().branches;
    let mesh = bark_mesh(bark, &tree.options().bark);

    assert_eq!(mesh.count_vertices(), bark.vertex_count());
    assert_eq!(mesh.indices().unwrap().len(), bark.indices.len());
    assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
    assert_eq!(float4(&mesh, ATTRIBUTE_WIND), bark.wind);

    // Apply bark texture repeat before flipping the vertical coordinate for Bevy images.
    let [su, sv] = BarkMaterial::new(&tree.options().bark).uv_scale;
    for (converted, &[u, v]) in float2(&mesh, Mesh::ATTRIBUTE_UV_0).iter().zip(&bark.uvs) {
        assert_eq!(*converted, [u * su, 1.0 - v * sv]);
    }
}

#[test]
fn flat_shaded_bark_has_one_normal_per_triangle() {
    let mut options = TreePreset::AshSmall.options();
    options.bark.flat_shading = true;
    options.bark.uv_mode = BarkUvMode::Continuous;
    let tree = Tree::new(options).unwrap();
    let bark = &tree.mesh().branches;
    let mesh = bark_mesh(bark, &tree.options().bark);

    // Flat shading gives each triangle three separate vertices with one shared surface normal.
    assert_eq!(mesh.count_vertices(), bark.indices.len());
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        panic!("no normals");
    };
    for triangle in normals.chunks(3) {
        assert_eq!(triangle[0], triangle[1]);
        assert_eq!(triangle[0], triangle[2]);
    }
    assert_eq!(
        float4(&mesh, Mesh::ATTRIBUTE_TANGENT).len(),
        bark.indices.len()
    );
    assert_eq!(float4(&mesh, ATTRIBUTE_WIND).len(), bark.indices.len());
}

#[test]
fn cluster_mesh_keeps_vertex_colours_and_wind() {
    let tree = Tree::from_preset(TreePreset::Apple);
    let fruit = &tree.mesh().fruit;
    assert!(fruit.vertex_count() > 0);
    let mesh = cluster_mesh(fruit);

    assert_eq!(mesh.count_vertices(), fruit.vertex_count());
    assert_eq!(mesh.indices().unwrap().len(), fruit.indices.len());
    assert_eq!(float4(&mesh, Mesh::ATTRIBUTE_COLOR), fruit.colors);
    assert_eq!(float4(&mesh, ATTRIBUTE_WIND), fruit.wind);
}

#[test]
fn fruit_and_flower_materials_take_colour_from_vertices_and_show_both_sides() {
    for material in [
        fruit_material(WindParams::default()),
        flower_material(WindParams::default()),
    ] {
        assert_eq!(material.base.base_color, Color::WHITE);
        assert!(material.base.base_color_texture.is_none());
        assert_eq!(material.base.cull_mode, None);
        assert!(material.base.double_sided);
    }
}

#[test]
fn leaf_mesh_puts_the_leaf_tip_at_the_top_of_the_image() {
    let tree = Tree::from_preset(TreePreset::AspenSmall);
    let leaves = &tree.mesh().leaves;
    let mesh = leaf_mesh(leaves);
    assert_eq!(mesh.count_vertices(), leaves.vertex_count());
    for (converted, &[u, v]) in float2(&mesh, Mesh::ATTRIBUTE_UV_0).iter().zip(&leaves.uvs) {
        assert_eq!(*converted, [u, 1.0 - v]);
    }
    assert_eq!(float4(&mesh, ATTRIBUTE_WIND), leaves.wind);
    assert_eq!(float4(&mesh, Mesh::ATTRIBUTE_TANGENT), leaves.tangents);
}

#[test]
fn leaf_material_uses_the_supplied_leaf_textures() {
    let leaves = TreePreset::OakSmall.options().leaves;
    let wind = WindParams::default();

    // Colour-only textures must leave the material without a normal map.
    let color_only = LeafTextures {
        color: Handle::default(),
        normal: None,
    };
    let material = leaf_material(&leaves, Some(&color_only), wind);
    assert!(material.base.base_color_texture.is_some());
    assert!(material.base.normal_map_texture.is_none());

    // A supplied normal-map handle must reach Bevy's material so it can add lighting detail.
    let with_normal = LeafTextures {
        color: Handle::default(),
        normal: Some(Handle::default()),
    };
    let material = leaf_material(&leaves, Some(&with_normal), wind);
    assert!(material.base.normal_map_texture.is_some());
}

#[test]
fn bark_normal_map_needs_continuous_texture_coordinates() {
    let textures = BarkTextures {
        color: Handle::default(),
        normal: Handle::default(),
        roughness: Handle::default(),
    };
    let mut bark = TreePreset::OakSmall.options().bark;
    let wind = WindParams::default();

    bark.uv_mode = BarkUvMode::Mirrored;
    let mirrored = bark_material(&bark, Some(&textures), wind);
    assert!(mirrored.base.base_color_texture.is_some());
    assert!(mirrored.base.normal_map_texture.is_none());

    bark.uv_mode = BarkUvMode::Continuous;
    let continuous = bark_material(&bark, Some(&textures), wind);
    assert!(continuous.base.normal_map_texture.is_some());

    // Disabling bark textures must ignore even explicitly supplied image handles.
    bark.textured = false;
    let plain = bark_material(&bark, Some(&textures), wind);
    assert!(plain.base.base_color_texture.is_none());
    assert_eq!(plain.extension.wind, WindUniform::from(wind));
}

#[test]
fn leaves_draw_both_sides_and_cut_out_their_shape() {
    let mut leaves = TreePreset::OakSmall.options().leaves;
    leaves.alpha_test = 0.4;

    leaves.rounded_normals = false;
    let flipped = leaf_material(&leaves, None, WindParams::default());
    assert_eq!(flipped.base.alpha_mode, AlphaMode::Mask(0.4));
    assert_eq!(flipped.base.cull_mode, None);
    assert!(flipped.base.double_sided);

    // Rounded canopy lighting must preserve the mesh normals when viewing leaf backs.
    leaves.rounded_normals = true;
    let rounded = leaf_material(&leaves, None, WindParams::default());
    assert_eq!(rounded.base.cull_mode, None::<Face>);
    assert!(!rounded.base.double_sided);
}

#[test]
fn wind_uniform_matches_the_core_layout() {
    let params = WindParams {
        strength: Vec3::new(0.25, -0.5, 0.75),
        frequency: 1.5,
        scale: 30.0,
        trunk_bend: 0.6,
        branch_sway: 0.02,
    };
    let uniform = WindUniform::from(params);
    assert_eq!(WindUniform::min_size().get(), 32);

    let mut buffer = UniformBuffer::new(Vec::<u8>::new());
    buffer.write(&uniform).unwrap();
    let bytes = buffer.into_inner();
    let floats: Vec<f32> = bytes
        .chunks(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(floats[..7], params.to_uniform()[..7]);
}
