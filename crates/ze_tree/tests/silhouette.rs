//! Compare distant tree outlines with full-detail and neighbouring levels.
//!
//! Each preset is projected into a small black-and-white mask from eight directions
//! around the trunk. Intersection over union (IoU) measures shared area divided by
//! combined area; 1 means identical masks. Leaf cards are solid squares here, so
//! these approximate checks do not include texture transparency or lighting.

use glam::{Vec2, Vec3};
use ze_tree::{BillboardVertex, MeshBuffers, Skeleton, TreeMesh, TreePreset, default_lod_levels};

/// Width and height of each square mask, in pixels.
const RESOLUTION: usize = 96;
/// Number of views sampled at equal angles around the trunk.
const DIRECTIONS: usize = 8;

/// Three triangle corners in tree coordinates.
type Triangle = [Vec3; 3];

/// Projection axes and square image bounds used to compare two outlines.
struct View {
    /// Horizontal and vertical projection directions in tree coordinates.
    right: Vec3,
    up: Vec3,
    /// Lower-left image bound and square image size in projected tree units.
    min: Vec2,
    size: f32,
}

impl View {
    /// Fits a square image around `reference`, with 15% extra space. `forward`
    /// sets the horizontal viewing direction; the vertical image axis stays at +Y.
    fn framing(forward: Vec3, reference: &[Triangle]) -> Self {
        let right = Vec3::Y.cross(-forward).normalize();
        let up = Vec3::Y;
        let mut min = Vec2::splat(f32::MAX);
        let mut max = Vec2::splat(f32::MIN);
        for p in reference.iter().flatten() {
            let q = Vec2::new(p.dot(right), p.dot(up));
            min = min.min(q);
            max = max.max(q);
        }
        let size = (max - min).max_element() * 1.15;
        let pad = (Vec2::splat(size) - (max - min)) / 2.0;
        Self {
            right,
            up,
            min: min - pad,
            size,
        }
    }

    /// Returns a mask marking pixel centres covered by any projected triangle.
    fn mask(&self, triangles: &[Triangle]) -> Vec<bool> {
        let mut mask = vec![false; RESOLUTION * RESOLUTION];
        let scale = RESOLUTION as f32 / self.size;
        let pixel = |p: Vec3| (Vec2::new(p.dot(self.right), p.dot(self.up)) - self.min) * scale;
        let last = RESOLUTION as f32 - 1.0;
        for t in triangles {
            let [a, b, c] = t.map(pixel);
            let area = (b - a).perp_dot(c - a);
            if area.abs() < 1e-9 {
                continue;
            }
            let lo = a.min(b).min(c).floor().max(Vec2::ZERO);
            let hi = a.max(b).max(c).ceil().min(Vec2::splat(last));
            if lo.x > hi.x || lo.y > hi.y {
                continue;
            }
            for y in lo.y as usize..=hi.y as usize {
                for x in lo.x as usize..=hi.x as usize {
                    let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let inside = (b - a).perp_dot(p - a) / area >= 0.0
                        && (c - b).perp_dot(p - b) / area >= 0.0
                        && (a - c).perp_dot(p - c) / area >= 0.0;
                    if inside {
                        mask[y * RESOLUTION + x] = true;
                    }
                }
            }
        }
        mask
    }
}

/// Returns shared area divided by combined area for equal-sized masks.
/// Two empty masks return 0.
fn iou(a: &[bool], b: &[bool]) -> f32 {
    let both = a.iter().zip(b).filter(|(a, b)| **a && **b).count();
    let either = a.iter().zip(b).filter(|(a, b)| **a || **b).count();
    both as f32 / either.max(1) as f32
}

/// Copies complete triangles using the mesh’s fixed vertex positions.
fn stored(buffers: &MeshBuffers, out: &mut Vec<Triangle>) {
    let p = |i: u32| Vec3::from_array(buffers.positions[i as usize]);
    for t in buffers.indices.as_chunks::<3>().0 {
        out.push(t.map(p));
    }
}

/// Collects fixed bark, leaf and fruit triangles, including crossed fallback
/// shapes, to represent a renderer without billboard rotation. Omits flowers.
fn stored_triangles(mesh: &TreeMesh) -> Vec<Triangle> {
    let cull = &mesh.billboard_cull;
    let mut out = Vec::new();
    for buffers in [&mesh.branches, &mesh.leaves, &mesh.fruit] {
        stored(buffers, &mut out);
    }
    for buffers in [&cull.branches, &cull.leaves, &cull.fruit] {
        stored(buffers, &mut out);
    }
    out
}

/// Collects bark, leaf and fruit triangles for an orthographic view toward
/// `to_camera`. Rotates billboards with the [`BillboardVertex`] formula and
/// omits their crossed fallback shapes. Non-billboard geometry stays fixed.
fn turned_triangles(mesh: &TreeMesh, to_camera: Vec3) -> Vec<Triangle> {
    let mut out = Vec::new();
    for buffers in [&mesh.branches, &mesh.leaves, &mesh.fruit] {
        if buffers.billboard.is_empty() {
            stored(buffers, &mut out);
            continue;
        }
        let p = |i: u32| turned(&buffers.billboard[i as usize], to_camera);
        for t in buffers.indices.as_chunks::<3>().0 {
            out.push(t.map(p));
        }
    }
    out
}

/// Reconstructs one billboard vertex for the unit direction `to_camera`.
/// The sampled views avoid +Y; axis-aligned views use a zero sideways vector.
fn turned(b: &BillboardVertex, to_camera: Vec3) -> Vec3 {
    let (pivot, axis) = (Vec3::from_array(b.pivot), Vec3::from_array(b.axis));
    let (right, up) = if axis == Vec3::ZERO {
        let right = Vec3::Y.cross(to_camera).normalize();
        (right, to_camera.cross(right))
    } else {
        (axis.cross(to_camera).normalize_or_zero(), axis)
    };
    pivot + right * b.offset[0] + up * b.offset[1]
}

/// Average outline overlap across all presets and sampled views.
#[derive(Debug, Default)]
struct Scores {
    /// Fixed-shape overlap of LOD1, LOD2 and LOD3 with LOD0.
    stored: [f32; 3],
    /// Fixed-shape overlap across the LOD1 → LOD2 and LOD2 → LOD3 switches.
    stored_switch: [f32; 2],
    /// Camera-facing overlap of LOD1, LOD2 and LOD3 with LOD0.
    turned: [f32; 3],
    /// Camera-facing overlap across the LOD1 → LOD2 and LOD2 → LOD3 switches.
    turned_switch: [f32; 2],
}

/// Measures overlap for the default levels, using LOD0 to frame every comparison.
fn measure() -> Scores {
    let levels = default_lod_levels();
    let directions: Vec<Vec3> = (0..DIRECTIONS)
        .map(|k| {
            let angle = std::f32::consts::TAU * k as f32 / DIRECTIONS as f32;
            Vec3::new(angle.cos(), -0.15, angle.sin()).normalize()
        })
        .collect();

    let mut scores = Scores::default();
    for preset in TreePreset::ALL {
        let options = preset.options();
        let skeleton = Skeleton::generate(&options).unwrap();
        let meshes: Vec<TreeMesh> = levels
            .iter()
            .map(|level| skeleton.mesh(&options, &level.detail))
            .collect();
        let triangles: Vec<Vec<Triangle>> = meshes.iter().map(stored_triangles).collect();
        for &forward in &directions {
            let view = View::framing(forward, &triangles[0]);
            let stored: Vec<Vec<bool>> = triangles.iter().map(|t| view.mask(t)).collect();
            let turned: Vec<Vec<bool>> = meshes
                .iter()
                .map(|m| view.mask(&turned_triangles(m, -forward)))
                .collect();
            for level in 1..4 {
                scores.stored[level - 1] += iou(&stored[0], &stored[level]);
                scores.turned[level - 1] += iou(&turned[0], &turned[level]);
            }
            for level in 1..3 {
                scores.stored_switch[level - 1] += iou(&stored[level], &stored[level + 1]);
                scores.turned_switch[level - 1] += iou(&turned[level], &turned[level + 1]);
            }
        }
    }

    let count = (TreePreset::ALL.len() * DIRECTIONS) as f32;
    for s in scores
        .stored
        .iter_mut()
        .chain(&mut scores.turned)
        .chain(&mut scores.stored_switch)
        .chain(&mut scores.turned_switch)
    {
        *s /= count;
    }
    scores
}

/// Checks minimum average outline overlap with full detail and across detail
/// switches. Covers fixed fallback rendering and camera-facing LOD3 rendering.
#[test]
fn distant_levels_keep_the_outline() {
    let scores = measure();
    eprintln!("{scores:#?}");
    let checks = [
        ("stored level 1 vs 0", scores.stored[0], 0.88),
        ("stored level 2 vs 0", scores.stored[1], 0.84),
        ("stored level 3 vs 0", scores.stored[2], 0.80),
        ("turned level 3 vs 0", scores.turned[2], 0.84),
        ("stored level 1 → 2", scores.stored_switch[0], 0.88),
        ("stored level 2 → 3", scores.stored_switch[1], 0.82),
        ("turned level 2 → 3", scores.turned_switch[1], 0.84),
    ];
    for (name, score, minimum) in checks {
        assert!(score >= minimum, "{name}: {score:.3} is below {minimum}");
    }
}
