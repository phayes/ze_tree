// Rotates flat tree shapes for the current render view, including depth, shadows
// and deferred rendering.

import bevy_pbr::render::mesh_view_bindings::view;

/// Outward tilt of the lighting direction for camera-facing shapes without a
/// stored normal, such as fruit. Larger values make the side of the canopy away
/// from the light darker.
const OUTWARD_LEAN: f32 = 0.35;
/// Upward tilt of the lighting direction for camera-facing shapes without a
/// stored normal, helping overhead light reach them.
const UPWARD_LEAN: f32 = 0.5;

/// A rotated vertex and its lighting normal, expressed in tree coordinates.
struct TurnedVertex {
    position: vec3<f32>,
    normal: vec3<f32>,
}

/// The directions a billboard shape is laid out along for the current view.
struct BillboardFrame {
    /// Unit direction from the shape toward the viewer or light.
    to_view: vec3<f32>,
    /// Unit direction across the shape.
    right: vec3<f32>,
    /// Unit direction along the shape.
    up: vec3<f32>,
}

/// Identifies shadow views by comparing the view position with Bevy’s LOD camera position.
fn is_shadow_view() -> bool {
    // Shadow views use the main camera’s position for LOD selection.
    return any(view.lod_view_world_position != view.world_position);
}

/// Works out the directions a billboard vertex is laid out along for the current
/// view. See [`turn_billboard`] for the inputs.
fn billboard_frame(
    pivot: vec4<f32>,
    axis: vec4<f32>,
    local_from_world: mat4x4<f32>,
) -> BillboardFrame {
    // Express the direction toward this viewer or light in tree coordinates.
    var to_view: vec3<f32>;
    if view.clip_from_view[3][3] == 1.0 {
        to_view = (local_from_world * vec4<f32>(view.world_from_view[2].xyz, 0.0)).xyz;
    } else {
        to_view = (local_from_world * vec4<f32>(view.world_position, 1.0)).xyz - pivot.xyz;
    }
    to_view = normalize(to_view);
    let view_right = normalize((local_from_world * vec4<f32>(view.world_from_view[0].xyz, 0.0)).xyz);

    // Build horizontal and vertical directions for the rotated shape. When looking
    // along the rotation axis, use the view’s sideways direction as a fallback.
    let facing = dot(axis.xyz, axis.xyz) == 0.0;
    var up = axis.xyz;
    if facing {
        up = vec3<f32>(0.0, 1.0, 0.0);
    }
    var right = cross(up, to_view);
    if dot(right, right) < 1e-8 {
        right = view_right - up * dot(view_right, up);
    }
    right = normalize(right);
    if facing {
        up = cross(to_view, right);
    }

    var frame: BillboardFrame;
    frame.to_view = to_view;
    frame.right = right;
    frame.up = up;
    return frame;
}

/// Places a vertex in `frame`. Shadow views also shift it away from the light,
/// so the light-facing copy of a shape does not cut through and shade the
/// camera-facing copy.
fn place(pivot: vec4<f32>, axis: vec4<f32>, frame: BillboardFrame) -> vec3<f32> {
    var position = pivot.xyz + frame.right * pivot.w + frame.up * axis.w;
    if is_shadow_view() {
        position -= frame.to_view * length(vec2<f32>(pivot.w, axis.w));
    }
    return position;
}

/// Rotates one billboard vertex without a stored normal toward the current view,
/// and returns a lighting normal facing the view.
///
/// Pass the pivot and horizontal offset in `pivot.xyz` and `pivot.w`. Pass a unit
/// rotation axis in `axis.xyz`, or zero for full camera-facing rotation, and the
/// vertical offset in `axis.w`. `local_from_world` converts world to tree coordinates.
/// Perspective views use the direction to the viewer; orthographic views use the
/// opposite of the viewing direction.
fn turn_billboard(
    pivot: vec4<f32>,
    axis: vec4<f32>,
    local_from_world: mat4x4<f32>,
) -> TurnedVertex {
    let frame = billboard_frame(pivot, axis, local_from_world);

    // Tilt camera-facing lighting upward and outward for a rounded canopy appearance.
    var normal = cross(frame.right, frame.up);
    if dot(axis.xyz, axis.xyz) == 0.0 {
        var lean = vec3<f32>(0.0, UPWARD_LEAN, 0.0);
        let outward = vec3<f32>(pivot.x, 0.0, pivot.z);
        if dot(outward, outward) > 0.0 {
            lean += OUTWARD_LEAN * normalize(outward);
        }
        normal = normalize(normal + lean);
    }

    var out: TurnedVertex;
    out.position = place(pivot, axis, frame);
    out.normal = normal;
    return out;
}

/// Rotates one billboard vertex toward the current view and lights it with its
/// stored `normal`, the same for every corner of the shape.
///
/// When `normal` faces away from the view, the shape is mirrored so the view sees
/// its back, as it would see the back of the fixed leaf it replaces. Bevy then
/// lights the back the same way it lights the back of a fixed leaf, including
/// the flip for double-sided materials. See [`turn_billboard`] for the other inputs.
fn turn_lit_billboard(
    pivot: vec4<f32>,
    axis: vec4<f32>,
    normal: vec3<f32>,
    local_from_world: mat4x4<f32>,
) -> TurnedVertex {
    var frame = billboard_frame(pivot, axis, local_from_world);
    if dot(normal, frame.to_view) < 0.0 {
        frame.right = -frame.right;
    }

    var out: TurnedVertex;
    out.position = place(pivot, axis, frame);
    out.normal = normal;
    return out;
}
