//! Convert generated bark, leaf, fruit and flower geometry into meshes Bevy can draw.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, MeshVertexAttribute, PrimitiveTopology, VertexFormat};
use ze_tree::{BarkMaterial, BarkOptions, MeshBuffers};

/// Wind controls stored on each vertex: trunk bend, branch movement weight,
/// branch timing offset and leaf flutter. See [`MeshBuffers::wind`] for the layout.
/// Every mesh drawn with [`TreeMaterial`](crate::TreeMaterial) needs this attribute.
pub const ATTRIBUTE_WIND: MeshVertexAttribute = MeshVertexAttribute::new(
    "ZeTree_Wind",
    0x5a45_5452_4545_0001,
    VertexFormat::Float32x4,
);

/// Creates a Bevy bark mesh from buffers such as [`TreeMesh::branches`](ze_tree::TreeMesh::branches).
///
/// Applies the texture repeat settings in `bark` and flips the vertical texture
/// coordinate for Bevy images. When flat shading is enabled, each triangle gets
/// its own vertices and normal, giving the bark a faceted appearance.
pub fn bark_mesh(buffers: &MeshBuffers, bark: &BarkOptions) -> Mesh {
    let [su, sv] = BarkMaterial::new(bark).uv_scale;
    let uvs = buffers
        .uvs
        .iter()
        .map(|&[u, v]| [u * su, 1.0 - v * sv])
        .collect();
    let mut mesh = base_mesh(buffers, uvs);
    if !buffers.tangents.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, buffers.tangents.clone());
    }
    if bark.flat_shading {
        mesh.duplicate_vertices();
        mesh.compute_flat_normals();
        // Rebuild the texture directions to match the new flat surface normals.
        if mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) {
            mesh.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
            if let Err(error) = mesh.generate_tangents() {
                bevy::log::warn!("ze_tree_bevy: no tangents for flat-shaded bark: {error}");
            }
        }
    }
    mesh
}

/// Creates a Bevy leaf mesh from buffers such as [`TreeMesh::leaves`](ze_tree::TreeMesh::leaves).
///
/// Flips the vertical texture coordinate so the top of the image maps to the leaf tip.
pub fn leaf_mesh(buffers: &MeshBuffers) -> Mesh {
    let uvs = buffers.uvs.iter().map(|&[u, v]| [u, 1.0 - v]).collect();
    base_mesh(buffers, uvs)
}

/// Creates a Bevy mesh for fruit or flowers from buffers such as
/// [`TreeMesh::fruit`](ze_tree::TreeMesh::fruit) or
/// [`TreeMesh::flowers`](ze_tree::TreeMesh::flowers).
///
/// The surface colour comes from the per-vertex colours, so draw it with
/// [`fruit_material`](crate::fruit_material) or [`flower_material`](crate::flower_material),
/// or another material with a white base colour.
pub fn cluster_mesh(buffers: &MeshBuffers) -> Mesh {
    let uvs = buffers.uvs.iter().map(|&[u, v]| [u, 1.0 - v]).collect();
    base_mesh(buffers, uvs).with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, buffers.colors.clone())
}

/// Builds the shared triangle mesh layout, preserving positions, normals, wind
/// controls and triangle indices while using the supplied texture coordinates.
fn base_mesh(buffers: &MeshBuffers, uvs: Vec<[f32; 2]>) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, buffers.normals.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(ATTRIBUTE_WIND, buffers.wind.clone())
    .with_inserted_indices(Indices::U32(buffers.indices.clone()))
}
