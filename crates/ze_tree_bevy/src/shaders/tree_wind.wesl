// Wind displacement shared by the main, depth, shadow and deferred tree shaders.

import ze_tree::wind::{WindParams, wind_offset};

/// The material's wind settings, supplied by the Rust `WindExtension`.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> wind_params: WindParams;

/// Returns movement in tree coordinates to add to `position` at `time` seconds.
///
/// `wind` holds this vertex's animation weights and timing offset. The tree's
/// world translation varies the wind pattern so copies at different positions
/// do not all move together.
fn tree_wind_offset(
    position: vec3<f32>,
    wind: vec4<f32>,
    world_from_local: mat4x4<f32>,
    time: f32,
) -> vec3<f32> {
    let anchor = world_from_local[3].xyz;
    return wind_offset(position + anchor, wind, anchor, time, wind_params);
}
