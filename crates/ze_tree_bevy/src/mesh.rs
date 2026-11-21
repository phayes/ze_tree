//! Convert generated bark, leaf, fruit and flower geometry into meshes Bevy can draw.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::math::Vec3;
use bevy::mesh::{
    Indices, Mesh, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues, VertexFormat,
};
use ze_tree::{BarkMaterial, BarkOptions, MeshBuffers};

/// Wind controls stored on each vertex: trunk bend, branch movement weight,
/// branch timing offset and leaf flutter. See [`MeshBuffers::wind`] for the layout.
/// Every mesh drawn with [`TreeMaterial`](crate::TreeMaterial) needs this attribute.
pub const ATTRIBUTE_WIND: MeshVertexAttribute = MeshVertexAttribute::new(
    "ZeTree_Wind",
    0x5a45_5452_4545_0001,
    VertexFormat::Float32x4,
);

/// Billboard pivot in `xyz` and horizontal offset in `w`, in tree coordinates.
/// See [`MeshBuffers::billboard`] for the source data.
///
/// Supply this together with [`ATTRIBUTE_BILLBOARD_AXIS`] to make
/// [`TreeMaterial`](crate::TreeMaterial) rotate the mesh toward each render view,
/// including shadow views. Billboard meshes do not need stored normals.
pub const ATTRIBUTE_BILLBOARD_PIVOT: MeshVertexAttribute = MeshVertexAttribute::new(
    "ZeTree_BillboardPivot",
    0x5a45_5452_4545_0002,
    VertexFormat::Float32x4,
);

/// Billboard rotation axis in `xyz` and vertical offset in `w`. The axis must
/// be unit length, or zero for full camera-facing rotation. Supply this together
/// with [`ATTRIBUTE_BILLBOARD_PIVOT`]; both use tree coordinates.
pub const ATTRIBUTE_BILLBOARD_AXIS: MeshVertexAttribute = MeshVertexAttribute::new(
    "ZeTree_BillboardAxis",
    0x5a45_5452_4545_0003,
    VertexFormat::Float32x4,
);

/// Optional unit lighting normal for a billboard shape, in tree coordinates, the
/// same at every corner of the shape. Billboard leaves from [`leaf_mesh`] have it.
///
/// With it, [`TreeMaterial`](crate::TreeMaterial) lights the turned shape with
/// this normal instead of one facing the camera, and shows the shape's back when
/// the normal faces away from the view, so the shape shades like the fixed leaf
/// it replaces. Billboard meshes store it here instead of in
/// [`Mesh::ATTRIBUTE_NORMAL`].
pub const ATTRIBUTE_BILLBOARD_NORMAL: MeshVertexAttribute = MeshVertexAttribute::new(
    "ZeTree_BillboardNormal",
    0x5a45_5452_4545_0004,
    VertexFormat::Float32x3,
);

/// Converts bark buffers, such as [`TreeMesh::branches`](ze_tree::TreeMesh::branches),
/// to a Bevy mesh using the supplied bark settings. Texture coordinates include
/// `bark`’s repeat settings and Bevy’s vertical image orientation.
///
/// Flat shading gives each triangle its own normal for a faceted appearance.
/// Ribbon billboards ignore flat shading and retain their camera-facing data;
/// use [`TreeMaterial`](crate::TreeMaterial) to turn them toward the view.
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
    if bark.flat_shading && buffers.billboard.is_empty() {
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
/// Flips the vertical texture coordinate to place the top of the image at the leaf
/// tip. Preserves any supplied tangents, which orient normal-map lighting detail.
pub fn leaf_mesh(buffers: &MeshBuffers) -> Mesh {
    let uvs = buffers.uvs.iter().map(|&[u, v]| [u, 1.0 - v]).collect();
    let mut mesh = base_mesh(buffers, uvs);
    if !buffers.tangents.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, buffers.tangents.clone());
    }
    mesh
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

/// Returns a bounding box in tree coordinates that contains every rotation of
/// the billboards in `buffers.billboard`. Returns `None` if that data is empty.
///
/// Attach the returned bounds and Bevy’s `NoAutoAabb` to custom billboard mesh
/// entities to prevent shapes disappearing while they turn toward the camera.
/// Bevy’s automatic bounds only cover the original vertex positions.
/// [`ZeTreePlugin`](crate::ZeTreePlugin) sets these bounds for generated billboard
/// parts automatically. The bounds do not cover wind movement or vertices absent
/// from `buffers.billboard`.
pub fn billboard_aabb(buffers: &MeshBuffers) -> Option<Aabb> {
    turning_bounds(
        buffers
            .billboard
            .iter()
            .map(|b| (Vec3::from_array(b.pivot), b.offset[0].hypot(b.offset[1]))),
    )
}

/// Returns mesh-local bounds covering billboard rotation, without wind movement.
/// Reads [`ATTRIBUTE_BILLBOARD_PIVOT`] and [`ATTRIBUTE_BILLBOARD_AXIS`] as
/// four-component float attributes. Returns `None` if either is missing, empty
/// or in another format. If their lengths differ, only paired entries contribute.
pub(crate) fn mesh_billboard_aabb(mesh: &Mesh) -> Option<Aabb> {
    let (
        Some(VertexAttributeValues::Float32x4(pivots)),
        Some(VertexAttributeValues::Float32x4(axes)),
    ) = (
        mesh.attribute(ATTRIBUTE_BILLBOARD_PIVOT),
        mesh.attribute(ATTRIBUTE_BILLBOARD_AXIS),
    )
    else {
        return None;
    };
    turning_bounds(
        pivots
            .iter()
            .zip(axes)
            .map(|(p, a)| (Vec3::new(p[0], p[1], p[2]), p[3].hypot(a[3]))),
    )
}

/// Encloses each `(pivot, radius)` pair in one bounding box, or returns `None`
/// for an empty iterator. Supply each vertex’s distance from its pivot as the
/// radius to cover all rotations around that pivot.
fn turning_bounds(points: impl Iterator<Item = (Vec3, f32)>) -> Option<Aabb> {
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for (pivot, reach) in points {
        let (min, max) = bounds.get_or_insert((pivot, pivot));
        *min = min.min(pivot - reach);
        *max = max.max(pivot + reach);
    }
    bounds.map(|(min, max)| Aabb::from_min_max(min, max))
}

/// Converts positions, indices, wind controls and any normals or billboard data
/// into Bevy attributes. Supply one texture-coordinate pair per position in `uvs`.
fn base_mesh(buffers: &MeshBuffers, uvs: Vec<[f32; 2]>) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(ATTRIBUTE_WIND, buffers.wind.clone())
    .with_inserted_indices(Indices::U32(buffers.indices.clone()));
    // Billboard normals go in their own attribute, which every render pass reads,
    // so depth, shadow and main passes mirror each shape the same way.
    if !buffers.normals.is_empty() {
        let attribute = if buffers.billboard.is_empty() {
            Mesh::ATTRIBUTE_NORMAL
        } else {
            ATTRIBUTE_BILLBOARD_NORMAL
        };
        mesh.insert_attribute(attribute, buffers.normals.clone());
    }

    // Pair each pivot with its horizontal offset, and each axis with its vertical
    // offset, in the two four-component attributes read by the shader.
    if !buffers.billboard.is_empty() {
        let (pivots, axes): (Vec<[f32; 4]>, Vec<[f32; 4]>) = buffers
            .billboard
            .iter()
            .map(|b| {
                let [px, py, pz] = b.pivot;
                let [ax, ay, az] = b.axis;
                ([px, py, pz, b.offset[0]], [ax, ay, az, b.offset[1]])
            })
            .unzip();
        mesh.insert_attribute(ATTRIBUTE_BILLBOARD_PIVOT, pivots);
        mesh.insert_attribute(ATTRIBUTE_BILLBOARD_AXIS, axes);
    }
    mesh
}
