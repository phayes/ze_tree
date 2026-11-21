// Draws trees with wind movement while preserving Bevy's lighting and texture inputs.

import bevy_pbr::render::{
    forward_io::{Vertex, VertexOutput, decompress_vertex},
    mesh_functions,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
};
import package::shaders::tree_billboard::{TurnedVertex, turn_billboard, turn_lit_billboard};
import package::shaders::tree_wind::tree_wind_offset;

/// Wind controls and optional billboard rotation data added to Bevy’s vertex inputs.
struct TreeVertex {
    @location(8) wind: vec4<f32>,
    @if(BILLBOARD)
    @location(9) billboard_pivot: vec4<f32>,
    @if(BILLBOARD)
    @location(10) billboard_axis: vec4<f32>,
    @if(BILLBOARD_NORMAL)
    @location(11) billboard_normal: vec3<f32>,
}

@vertex
fn vertex(in: Vertex, tree: TreeVertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(in, in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);

    // Rotate billboards toward this view before adding wind movement.
    var position = vertex.position;
@if(BILLBOARD) {
    let local_from_world = mesh_functions::get_local_from_world(in.instance_index);
    var turned: TurnedVertex;
@if(BILLBOARD_NORMAL) {
    turned = turn_lit_billboard(
        tree.billboard_pivot,
        tree.billboard_axis,
        tree.billboard_normal,
        local_from_world,
    );
}
@if(!BILLBOARD_NORMAL) {
    turned = turn_billboard(tree.billboard_pivot, tree.billboard_axis, local_from_world);
}
    position = turned.position;
    out.world_normal = mesh_functions::mesh_normal_local_to_world(turned.normal, in.instance_index);
}

    // Apply wind locally, then use the entity transform to place the vertex in the world.
    position += tree_wind_offset(position, tree.wind, world_from_local, globals.time);
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(position, 1.0),
    );
    out.position = position_world_to_clip(out.world_position.xyz);

@if(VERTEX_NORMALS && !BILLBOARD)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, in.instance_index);
@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;
@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        in.instance_index,
    );
@if(VERTEX_COLORS)
    out.color = vertex.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = in.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        in.instance_index,
        world_from_local[3],
    );
    return out;
}
