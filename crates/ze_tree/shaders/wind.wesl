// Wind animation for ze_tree meshes.
//
// In a vertex shader, add `wind_offset` to the vertex position before applying
// transforms. It combines trunk bend, branch sway, and leaf flutter. The default
// Rust WindParams matches ez-tree: only leaves move.
//
// The returned displacement follows `strength` and uses tree units.

/// Wind controls, laid out for the 32-byte buffer from Rust's `WindParams::to_uniform`.
struct WindParams {
    /// Direction and size of motion. Also sets the maximum leaf-tip displacement.
    strength: vec3<f32>,
    /// Animation speed: radians per second for the slowest leaf oscillation.
    frequency: f32,
    /// Distance over which the wind pattern varies. Larger values give more gradual
    /// variation across the tree. Must be nonzero.
    scale: f32,
    /// Multiplier for trunk movement. Set to 0 to disable trunk bend.
    trunk_bend: f32,
    /// Multiplier for branch movement, whose weight increases toward branch tips.
    /// Set to 0 to disable branch sway.
    branch_sway: f32,
}

/// Returns the displacement to add to a vertex at `time`, measured in seconds.
///
/// Supply the vertex's `MeshBuffers::wind` data as `wind`:
/// `(trunk_bend, branch_weight, branch_phase, flutter)`.
/// `position` controls how leaf motion varies across space. Use the position in
/// tree coordinates to match ez-tree, or world coordinates to vary flutter
/// between copies of the same tree.
///
/// Use one `anchor` for the entire tree, such as its world-space origin, to vary
/// trunk and branch timing between trees. Use zero to match ez-tree.
/// The offset follows `params.strength`; add it in that vector's coordinate system.
fn wind_offset(
    position: vec3<f32>,
    wind: vec4<f32>,
    anchor: vec3<f32>,
    time: f32,
    params: WindParams,
) -> vec3<f32> {
    return trunk_offset(wind.x, anchor, time, params)
        + branch_offset(wind.y, wind.z, anchor, time, params)
        + leaf_flutter(position, wind.w, time, params);
}

/// Moves the trunk downwind, varying between half and all of
/// `bend × trunk_bend × strength`. The mesh supplies `bend`: 0 at ground level
/// and 1 at the highest section.
fn trunk_offset(bend: f32, anchor: vec3<f32>, time: f32, params: WindParams) -> vec3<f32> {
    let angle = 0.5 * time * params.frequency + tree_phase(anchor, params);
    let lean = 0.75 + 0.25 * sin(angle);
    return params.strength * (params.trunk_bend * bend * lean);
}

/// Moves a branch back and forth along the wind direction, by up to
/// `weight × branch_sway × strength`. The mesh supplies the movement `weight`
/// and the timing offset `phase` in `[0, 1)` for the branch's main limb.
fn branch_offset(
    weight: f32,
    phase: f32,
    anchor: vec3<f32>,
    time: f32,
    params: WindParams,
) -> vec3<f32> {
    let angle = time * params.frequency + 6.283185307 * phase + tree_phase(anchor, params);
    let swing = 0.7 * sin(angle) + 0.3 * sin(2.3 * angle);
    return params.strength * (params.branch_sway * weight * swing);
}

/// Moves leaves using ez-tree's flutter pattern. A `weight` of 0 keeps the leaf
/// base still; 1 lets the tip move by up to `strength`. Position varies the timing.
fn leaf_flutter(position: vec3<f32>, weight: f32, time: f32, params: WindParams) -> vec3<f32> {
    // Preserve ez-tree's flutter pattern: it uses 2 × 3.14 rather than 2π.
    let offset = 2.0 * 3.14 * simplex3(position / params.scale);
    let t = time * params.frequency;
    let sway = 0.5 * sin(t + offset)
        + 0.3 * sin(2.0 * t + 1.3 * offset)
        + 0.2 * sin(5.0 * t + 1.5 * offset);
    return weight * params.strength * sway;
}

/// Uses the tree's anchor to choose a repeatable timing offset, in radians.
fn tree_phase(anchor: vec3<f32>, params: WindParams) -> f32 {
    return 6.283185307 * simplex3(anchor / params.scale);
}

// Simplex noise gives nearby positions smoothly varying values, roughly -1 to 1.
// This is the 3D noise used by ez-tree, from
// https://github.com/ashima/webgl-noise (MIT licence).

fn mod289_3(x: vec3<f32>) -> vec3<f32> {
    return x - floor(x * (1.0 / 289.0)) * 289.0;
}

fn mod289_4(x: vec4<f32>) -> vec4<f32> {
    return x - floor(x * (1.0 / 289.0)) * 289.0;
}

fn permute(x: vec4<f32>) -> vec4<f32> {
    return mod289_4(((x * 34.0) + 1.0) * x);
}

fn taylor_inv_sqrt(r: vec4<f32>) -> vec4<f32> {
    return 1.79284291400159 - 0.85373472095314 * r;
}

fn simplex3(v: vec3<f32>) -> f32 {
    const C = vec2<f32>(1.0 / 6.0, 1.0 / 3.0);
    const D = vec4<f32>(0.0, 0.5, 1.0, 2.0);

    // Locate the first corner of the tetrahedron containing this point.
    var i = floor(v + dot(v, C.yyy));
    let x0 = v - i + dot(i, C.xxx);

    // Find the remaining three corners and the point's offset from each.
    let g = step(x0.yzx, x0.xyz);
    let l = 1.0 - g;
    let i1 = min(g.xyz, l.zxy);
    let i2 = max(g.xyz, l.zxy);
    let x1 = x0 - i1 + C.xxx;
    let x2 = x0 - i2 + C.yyy;
    let x3 = x0 - D.yyy;

    // Hash the corner coordinates to select repeatable gradient directions.
    i = mod289_3(i);
    let p = permute(
        permute(permute(i.z + vec4<f32>(0.0, i1.z, i2.z, 1.0)) + i.y + vec4<f32>(0.0, i1.y, i2.y, 1.0))
            + i.x + vec4<f32>(0.0, i1.x, i2.x, 1.0)
    );

    // Map a 7 × 7 grid onto an octahedron to generate gradient directions.
    let n_ = 0.142857142857;
    let ns = n_ * D.wyz - D.xzx;
    let j = p - 49.0 * floor(p * ns.z * ns.z);
    let x_ = floor(j * ns.z);
    let y_ = floor(j - 7.0 * x_);
    let x = x_ * ns.x + ns.yyyy;
    let y = y_ * ns.x + ns.yyyy;
    let h = 1.0 - abs(x) - abs(y);

    let b0 = vec4<f32>(x.xy, y.xy);
    let b1 = vec4<f32>(x.zw, y.zw);
    let s0 = floor(b0) * 2.0 + 1.0;
    let s1 = floor(b1) * 2.0 + 1.0;
    let sh = -step(h, vec4<f32>(0.0));
    let a0 = b0.xzyw + s0.xzyw * sh.xxyy;
    let a1 = b1.xzyw + s1.xzyw * sh.zzww;

    // Scale the gradient vectors to approximately unit length.
    let norm = taylor_inv_sqrt(vec4<f32>(
        dot(vec3<f32>(a0.xy, h.x), vec3<f32>(a0.xy, h.x)),
        dot(vec3<f32>(a0.zw, h.y), vec3<f32>(a0.zw, h.y)),
        dot(vec3<f32>(a1.xy, h.z), vec3<f32>(a1.xy, h.z)),
        dot(vec3<f32>(a1.zw, h.w), vec3<f32>(a1.zw, h.w)),
    ));
    let g0 = vec3<f32>(a0.xy, h.x) * norm.x;
    let g1 = vec3<f32>(a0.zw, h.y) * norm.y;
    let g2 = vec3<f32>(a1.xy, h.z) * norm.z;
    let g3 = vec3<f32>(a1.zw, h.w) * norm.w;

    // Blend the four corners' contributions, fading each with distance.
    var m = max(0.6 - vec4<f32>(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)), vec4<f32>(0.0));
    m = m * m;
    return 42.0 * dot(m * m, vec4<f32>(dot(g0, x0), dot(g1, x1), dot(g2, x2), dot(g3, x3)));
}
