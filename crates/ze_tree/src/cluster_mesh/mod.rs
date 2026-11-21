//! Create coloured fruit, flowers, and supporting stalks from a tree skeleton.

mod flower;
mod fruit;

use std::f32::consts::TAU;

use glam::{Quat, Vec3, Vec4};

use crate::material::srgb_hex_to_linear;
use crate::mesh::{ClusterSpan, ItemDetail, LodDetail, MeshBuffers};
use crate::wind::{BranchWind, wind_at};
use crate::{ClusterItem, ClusterKind, ClusterStalk, FlowerOptions, FruitOptions, SkeletonCluster};

/// Number of sides around a full-detail stalk.
pub(super) const STALK_SIDES: usize = 6;
/// Number of sides around a simplified stalk or fruit stem.
pub(super) const SIMPLE_STALK_SIDES: usize = 3;
/// Below this sine of the angle between a strip's path and its facing
/// direction, the strip keeps its previous sideways direction.
const STRIP_PARALLEL: f32 = 0.1;

/// Adds fruit geometry and returns spans for retained clusters in input order.
/// Pass validated options and wind data for the source branches. Ordinary fruit
/// go into `out` with supporting stalks; catkin fruit have no individual stems.
/// With [`LodDetail::billboard_fruit`], each fruit becomes a main hexagon in `out`
/// and a crossed fallback hexagon in `cull`, with no stalks.
pub(crate) fn mesh_fruit(
    [out, cull]: [&mut MeshBuffers; 2],
    clusters: &[SkeletonCluster],
    fruit: &FruitOptions,
    winds: &[BranchWind],
    detail: &LodDetail,
) -> Vec<ClusterSpan> {
    if fruit.placement.size < detail.min_item_size {
        return Vec::new();
    }
    let stalk_color = rgba(fruit.cluster.stalk_color);
    let catkin = fruit.cluster.kind == ClusterKind::Catkin;
    let level = detail.item_detail;

    // Keep all billboard fruit at their original sizes, without supporting stalks.
    if detail.billboard_fruit {
        let every_item = LodDetail {
            cluster_stride: 1,
            item_stride: 1,
            item_scale: 1.0,
            ..*detail
        };
        return mesh_clusters(
            [out, cull],
            clusters,
            None,
            winds,
            &every_item,
            |[out, cull], cluster, items, wind| {
                let facing = outward(cluster.origin);
                for (k, item) in items.iter().enumerate() {
                    let item_wind = wind.at(item.distance);
                    fruit::mesh_billboard([out, cull], item, k, fruit, facing, item_wind);
                }
            },
        );
    }

    let flat = !catkin && fruit.placement.size < detail.flat_item_size;
    mesh_clusters(
        [out, cull],
        clusters,
        Some(stalk_color),
        winds,
        &keep_flat_items(detail, flat),
        |[out, _], cluster, items, wind| {
            if flat {
                let facing = outward(cluster.origin);
                for (k, item) in items.iter().enumerate() {
                    fruit::mesh_disc(out, item, k, fruit, facing, wind.at(item.distance));
                }
                return;
            }
            let style = fruit::Style::new(fruit, level, stalk_color, !catkin, cluster.origin);
            for item in items {
                fruit::mesh_item(out, item, fruit, &style, wind.at(item.distance));
            }
        },
    )
}

/// Appends flowers and supporting stalks, returning buffer ranges for retained
/// clusters in input order. Pass validated flower options and wind data for
/// the skeleton branches. Catkins have rounded flowers instead of petals.
pub(crate) fn mesh_flowers(
    out: &mut MeshBuffers,
    clusters: &[SkeletonCluster],
    flowers: &FlowerOptions,
    winds: &[BranchWind],
    detail: &LodDetail,
) -> Vec<ClusterSpan> {
    if flowers.placement.size < detail.min_item_size {
        return Vec::new();
    }
    let stalk_color = rgba(flowers.cluster.stalk_color);
    let catkin = flowers.cluster.kind == ClusterKind::Catkin;
    let level = detail.item_detail;
    let flat = !catkin && flowers.placement.size < detail.flat_item_size;
    // Flowers do not use crossed fallback shapes; their cull buffer stays empty.
    let mut no_cull = MeshBuffers::default();
    mesh_clusters(
        [out, &mut no_cull],
        clusters,
        Some(stalk_color),
        winds,
        &keep_flat_items(detail, flat),
        |[out, _], _, items, wind| {
            if flat {
                for item in items {
                    flower::mesh_triangle(out, item, flowers, wind.at(item.distance));
                }
                return;
            }
            for item in items {
                let item_wind = wind.at(item.distance);
                if catkin {
                    flower::mesh_catkin_item(out, item, flowers, item_wind, level);
                } else {
                    flower::mesh_item(out, item, flowers, item_wind, level);
                }
            }
        },
    )
}

/// Returns `detail` with every item kept when items are drawn flat.
fn keep_flat_items(detail: &LodDetail, flat: bool) -> LodDetail {
    if !flat {
        return *detail;
    }
    LodDetail {
        item_stride: 1,
        ..*detail
    }
}

/// Processes retained clusters in input order and returns their main and fallback
/// buffer ranges. Adds supporting stalks to `out` unless `stalk_color` is `None`,
/// then calls `items` with both buffers and the retained, resized items. Wind
/// values still use the original cluster extent when items are skipped.
fn mesh_clusters(
    [out, cull]: [&mut MeshBuffers; 2],
    clusters: &[SkeletonCluster],
    stalk_color: Option<Vec4>,
    winds: &[BranchWind],
    detail: &LodDetail,
    mut items: impl FnMut([&mut MeshBuffers; 2], &SkeletonCluster, &[ClusterItem], &ClusterWind),
) -> Vec<ClusterSpan> {
    let cluster_stride = detail.cluster_stride.max(1) as usize;
    let item_stride = detail.item_stride.max(1) as usize;

    let mut spans = Vec::with_capacity(clusters.len().div_ceil(cluster_stride));
    for (index, cluster) in clusters.iter().enumerate().step_by(cluster_stride) {
        let (first_vertex, first_index) = (out.next_index(), out.indices.len());
        let (first_cull_vertex, first_cull_index) = (cull.next_index(), cull.indices.len());
        let wind = ClusterWind::new(cluster, wind_at(winds, &cluster.parent));

        // Resize retained items and keep their full chain of supporting stalks.
        let kept: Vec<ClusterItem> = cluster
            .items
            .iter()
            .step_by(item_stride)
            .map(|item| ClusterItem {
                size: item.size * detail.item_scale,
                ..*item
            })
            .collect();
        if let Some(stalk_color) = stalk_color {
            let needed = needed_stalks(&cluster.stalks, &kept);
            let shape = match detail.item_detail {
                ItemDetail::Full => StalkShape::Tube(STALK_SIDES),
                ItemDetail::Simple => StalkShape::Tube(SIMPLE_STALK_SIDES),
                ItemDetail::Minimal => StalkShape::Strip(outward(cluster.origin)),
            };
            for (stalk, _) in cluster.stalks.iter().zip(needed).filter(|(_, n)| *n) {
                mesh_stalk(out, stalk, shape, stalk_color, &wind);
            }
        }
        items([&mut *out, &mut *cull], cluster, &kept, &wind);

        spans.push(ClusterSpan {
            cluster: index,
            vertices: first_vertex..out.next_index(),
            indices: index_range(first_index, out.indices.len()),
            cull_vertices: first_cull_vertex..cull.next_index(),
            cull_indices: index_range(first_cull_index, cull.indices.len()),
        });
    }
    spans
}

/// Returns one flag per stalk, marking those that support any retained item,
/// including the chain of parent stalks back to the cluster base. Item stalk
/// indices and parent indices must refer to entries in `stalks`.
fn needed_stalks(stalks: &[ClusterStalk], items: &[ClusterItem]) -> Vec<bool> {
    let mut needed = vec![false; stalks.len()];
    for item in items {
        let mut next = item.stalk;
        while let Some(s) = next {
            if needed[s] {
                break;
            }
            needed[s] = true;
            next = stalks[s].parent;
        }
    }
    needed
}

/// Wind values for a cluster. The cluster follows its branch, with extra
/// flutter increasing from its base to its farthest stalk tip or item.
struct ClusterWind {
    attachment: [f32; 3],
    reach: f32,
}

impl ClusterWind {
    /// Combines the branch attachment’s wind values with the cluster’s total reach.
    fn new(cluster: &SkeletonCluster, attachment: [f32; 3]) -> Self {
        let stalk_reach = cluster
            .stalks
            .iter()
            .map(|s| s.base_distance + polyline_length(&s.points));
        let item_reach = cluster.items.iter().map(|i| i.distance);
        let reach = stalk_reach.chain(item_reach).fold(0.0, f32::max);
        Self { attachment, reach }
    }

    /// Returns `[trunk_bend, branch_weight, branch_phase, flutter]` at a distance
    /// along the stalks from the cluster base. Flutter stays within 0–1 and is
    /// zero for a cluster with no length.
    fn at(&self, distance: f32) -> [f32; 4] {
        let flutter = if self.reach > 0.0 {
            (distance / self.reach).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let [bend, weight, phase] = self.attachment;
        [bend, weight, phase, flutter]
    }
}

/// How a stalk's surface is built.
#[derive(Clone, Copy)]
enum StalkShape {
    /// An open-ended tube with this many sides (at least one).
    Tube(usize),
    /// A flat strip as wide as the stalk, turned to face this unit direction.
    Strip(Vec3),
}

/// Returns the horizontal unit direction from the trunk's axis toward `point`,
/// or +X on the axis itself.
fn outward(point: Vec3) -> Vec3 {
    Vec3::new(point.x, 0.0, point.z).normalize_or(Vec3::X)
}

/// Appends a stalk following its points with the given shape. Adds nothing
/// for a nonpositive radius, fewer than two points, or zero total length.
fn mesh_stalk(
    out: &mut MeshBuffers,
    stalk: &ClusterStalk,
    shape: StalkShape,
    color: Vec4,
    wind: &ClusterWind,
) {
    let mut distance = stalk.base_distance;
    let mut row_winds = Vec::with_capacity(stalk.points.len());
    for (k, point) in stalk.points.iter().enumerate() {
        if k > 0 {
            distance += point.distance(stalk.points[k - 1]);
        }
        row_winds.push(wind.at(distance));
    }
    let (grid, fallback) = match shape {
        StalkShape::Tube(sides) => (tube(&stalk.points, stalk.radius, sides, color), Vec3::Y),
        StalkShape::Strip(facing) => (strip(&stalk.points, stalk.radius, facing, color), facing),
    };
    if let Some(grid) = grid {
        grid.emit(out, fallback, |row| row_winds[row]);
    }
}

/// Creates a flat strip along `points`, `2 × radius` wide, with its flat side
/// turned as far toward the unit direction `facing` as the path allows.
/// Where the path runs along `facing`, the strip keeps the sideways direction
/// it had before, so it never flips over; its front may therefore face away
/// from `facing`, so draw strips two-sided. Returns `None` for a nonpositive
/// radius, fewer than two points, or zero total length.
fn strip(points: &[Vec3], radius: f32, facing: Vec3, color: Vec4) -> Option<Grid> {
    if radius <= 0.0 || points.len() < 2 || polyline_length(points) <= 0.0 {
        return None;
    }
    let n = points.len();
    let mut grid = Grid::new(2, false);
    let mut previous: Option<Vec3> = None;
    for k in 0..n {
        let tangent =
            (points[(k + 1).min(n - 1)] - points[k.saturating_sub(1)]).normalize_or(Vec3::Y);

        // Choose a sideways direction across the strip, keeping it steady along the path.
        let across = tangent.cross(facing);
        let across = match previous {
            _ if across.length() > STRIP_PARALLEL => across.normalize(),
            Some(p) => {
                (p - tangent * tangent.dot(p)).normalize_or(tangent.any_orthonormal_vector())
            }
            None => tangent.any_orthonormal_vector(),
        };
        let across = match previous {
            Some(p) if across.dot(p) < 0.0 => -across,
            _ => across,
        };
        previous = Some(across);

        // Listing the `across` edge first makes the front face `facing` until any sign flip above.
        let v = k as f32 / (n - 1) as f32;
        grid.push(points[k] + across * radius, [0.0, v], color);
        grid.push(points[k] - across * radius, [1.0, v], color);
    }
    Some(grid)
}

/// Creates an open-ended tube along `points`. Supply a radius in tree units,
/// a positive side count, and a linear RGBA colour. Returns `None` for a
/// nonpositive radius, fewer than two points, or zero total length.
/// Panics for zero sides when the radius and path would otherwise produce a tube.
pub(super) fn tube(points: &[Vec3], radius: f32, sides: usize, color: Vec4) -> Option<Grid> {
    if radius <= 0.0 {
        return None;
    }
    sweep(points, &vec![radius; points.len()], sides, color)
}

/// Creates a surface along `points` with `sides` sides and a radius in tree
/// units for each point. A zero radius brings the surface to a tip.
/// Returns `None` for fewer than two points or zero total length. Otherwise,
/// panics if `sides` is zero or `radii` has fewer entries than `points`.
fn sweep(points: &[Vec3], radii: &[f32], sides: usize, color: Vec4) -> Option<Grid> {
    if points.len() < 2 || polyline_length(points) <= 0.0 {
        return None;
    }

    // Align each ring with the path through its neighbouring points.
    let n = points.len();
    let tangents: Vec<Vec3> = (0..n)
        .map(|k| {
            let before = points[k.saturating_sub(1)];
            let after = points[(k + 1).min(n - 1)];
            (after - before).normalize_or(Vec3::Y)
        })
        .collect();

    // Rotate successive rings as the path bends.
    let mut grid = Grid::new(sides + 1, true);
    let mut rotation = Quat::from_rotation_arc(Vec3::Y, tangents[0]);
    for k in 0..n {
        if k > 0 {
            rotation = Quat::from_rotation_arc(tangents[k - 1], tangents[k]) * rotation;
        }
        let v = k as f32 / (n - 1) as f32;
        for j in 0..=sides {
            let angle = loop_angle(j, sides);
            let around = rotation * Vec3::new(angle.cos(), 0.0, angle.sin());
            let u = j as f32 / sides as f32;
            grid.push(points[k] + around * radii[k], [u, v], color);
        }
    }
    Some(grid)
}

/// A surface made from equal-length rows of points. Neighbouring points form
/// triangles, with the front facing the cross product of the direction to the
/// next row and the direction to the next column.
pub(super) struct Grid {
    columns: usize,
    /// Whether each row ends by repeating its first position to close a texture seam.
    wrap: bool,
    points: Vec<Vec3>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<Vec4>,
}

impl Grid {
    /// Starts an empty surface with `columns` points per row. Use at least two
    /// columns. Set `wrap` when the last point in each row repeats the first,
    /// so lighting remains smooth across that seam.
    pub(super) fn new(columns: usize, wrap: bool) -> Self {
        Self {
            columns,
            wrap,
            points: Vec::new(),
            uvs: Vec::new(),
            colors: Vec::new(),
        }
    }

    /// Adds a position, texture coordinates, and linear RGBA colour. Fill one
    /// complete row before starting the next.
    pub(super) fn push(&mut self, point: Vec3, uv: [f32; 2], color: Vec4) {
        self.points.push(point);
        self.uvs.push(uv);
        self.colors.push(color);
    }

    /// Appends the surface and calls `wind` with each vertex’s row number.
    /// Supply complete rows and a unit-length `fallback` lighting direction for
    /// points with no surrounding area. Triangles with zero area are omitted.
    pub(super) fn emit(
        &self,
        out: &mut MeshBuffers,
        fallback: Vec3,
        wind: impl Fn(usize) -> [f32; 4],
    ) {
        let rows = self.points.len() / self.columns;
        let at = |row: usize, column: usize| row * self.columns + column;

        // Join neighbouring rows and columns, then discard triangles with zero area.
        let mut triangles = Vec::new();
        for i in 0..rows.saturating_sub(1) {
            for j in 0..self.columns - 1 {
                let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j), at(i + 1, j + 1));
                triangles.push([a, c, b]);
                triangles.push([b, c, d]);
            }
        }
        triangles.retain(|t| self.face(t).length_squared() > 0.0);

        // Blend neighbouring triangle directions for smooth lighting; larger triangles count more.
        let mut normals = vec![Vec3::ZERO; self.points.len()];
        for t in &triangles {
            let face = self.face(t);
            for &v in t {
                normals[v] += face;
            }
        }

        // Give coincident points at a tip or texture seam the same lighting direction.
        for i in 0..rows {
            let row = at(i, 0)..at(i + 1, 0);
            let first = self.points[row.start];
            if self.points[row.clone()].iter().all(|&p| p == first) {
                let sum: Vec3 = normals[row.clone()].iter().sum();
                normals[row].fill(sum);
            }
        }
        if self.wrap {
            for i in 0..rows {
                let sum = normals[at(i, 0)] + normals[at(i, self.columns - 1)];
                normals[at(i, 0)] = sum;
                normals[at(i, self.columns - 1)] = sum;
            }
        }

        // Offset triangle indices to account for vertices already in the output.
        let base = out.next_index();
        for (k, &point) in self.points.iter().enumerate() {
            let normal = normals[k].normalize_or(fallback);
            let color = self.colors[k].to_array();
            out.push_colored(point, normal, self.uvs[k], wind(k / self.columns), color);
        }
        for t in triangles {
            out.indices.extend(t.map(|v| base + v as u32));
        }
    }

    /// Returns a vector perpendicular to the triangle at these point indices.
    /// Its length is twice the triangle’s area; its direction identifies the front.
    fn face(&self, [a, b, c]: &[usize; 3]) -> Vec3 {
        let (a, b, c) = (self.points[*a], self.points[*b], self.points[*c]);
        (b - a).cross(c - a)
    }
}

/// Appends a coloured triangle with its front facing `normal`, swapping
/// corners if necessary. A zero normal uses the triangle’s own direction.
/// Collinear or coincident corners add nothing.
pub(super) fn push_triangle(
    out: &mut MeshBuffers,
    corners: [Vec3; 3],
    normal: Vec3,
    color: Vec4,
    wind: [f32; 4],
) {
    let [a, mut b, mut c] = corners;
    let face = (b - a).cross(c - a);
    if face.length_squared() == 0.0 {
        return;
    }
    if face.dot(normal) < 0.0 {
        std::mem::swap(&mut b, &mut c);
    }
    let normal = normal.normalize_or(face.normalize());
    let base = out.next_index();
    for (point, uv) in [a, b, c]
        .into_iter()
        .zip([[0.5, 0.0], [0.0, 1.0], [1.0, 1.0]])
    {
        out.push_colored(point, normal, uv, wind, color.to_array());
    }
    out.indices.extend([base, base + 1, base + 2]);
}

/// Returns a unit direction for the coloured patch on fruit at `origin`: upward
/// and away from the trunk. On the trunk’s axis, the outward component is +X.
pub(super) fn blush_direction(origin: Vec3) -> Vec3 {
    let out = Vec3::new(origin.x, 0.0, origin.z).normalize_or(Vec3::X);
    (out + Vec3::Y).normalize()
}

/// Converts a `0xRRGGBB` display colour to linear RGBA for lighting, with
/// alpha 1 (fully opaque). Bits above the lowest 24 are ignored.
pub(super) fn rgba(hex: u32) -> Vec4 {
    Vec3::from_array(srgb_hex_to_linear(hex)).extend(1.0)
}

/// Returns an angle in radians for position `j` around `count` equal steps.
/// Positions wrap around: `j == count` returns exactly zero to close a seam.
/// Panics if `count` is zero.
pub(super) fn loop_angle(j: usize, count: usize) -> f32 {
    TAU * (j % count) as f32 / count as f32
}

/// Blends smoothly from 0 at `edge0` to 1 at `edge1`, clamping values outside
/// that interval. Pass finite values with `edge0 < edge1`.
pub(super) fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Adds the distances between successive points. Returns zero for fewer
/// than two points.
fn polyline_length(points: &[Vec3]) -> f32 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Returns `start..end` as buffer offsets, including the start and excluding
/// the end. Panics if either offset exceeds `u32::MAX`.
fn index_range(start: usize, end: usize) -> std::ops::Range<u32> {
    let convert = |i: usize| u32::try_from(i).expect("index buffer longer than u32::MAX");
    convert(start)..convert(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tube_faces_outward_and_closes_its_seam() {
        let points = [Vec3::ZERO, Vec3::Y, Vec3::new(0.0, 2.0, 0.5)];
        let grid = tube(&points, 0.1, STALK_SIDES, Vec4::ONE).unwrap();
        let mut out = MeshBuffers::default();
        grid.emit(&mut out, Vec3::Y, |row| [0.0, 0.0, 0.0, row as f32]);

        assert_eq!(out.vertex_count(), 3 * (STALK_SIDES + 1));
        assert_eq!(out.colors.len(), out.vertex_count());
        // The two edges of the texture seam must meet with matching lighting.
        let last = STALK_SIDES;
        assert_eq!(out.positions[0], out.positions[last]);
        assert_eq!(out.normals[0], out.normals[last]);
        // Lighting directions must point outward from the stalk.
        for (k, n) in out.normals.iter().enumerate().take(STALK_SIDES) {
            let p = Vec3::from_array(out.positions[k]);
            assert!(Vec3::from_array(*n).dot(p) > 0.0, "vertex {k}");
        }
        assert_eq!(out.wind[out.vertex_count() - 1][3], 2.0);
    }

    #[test]
    fn tube_without_area_is_skipped() {
        assert!(tube(&[Vec3::ZERO, Vec3::Y], 0.0, 3, Vec4::ONE).is_none());
        assert!(tube(&[Vec3::ONE, Vec3::ONE], 0.1, 3, Vec4::ONE).is_none());
    }

    #[test]
    fn grid_pole_rows_share_one_normal() {
        // Make a cone with a pointed bottom and a triangular rim.
        let mut grid = Grid::new(4, true);
        for _ in 0..4 {
            grid.push(Vec3::NEG_Y, [0.0; 2], Vec4::ONE);
        }
        for j in 0..4 {
            let angle = TAU * j as f32 / 3.0;
            grid.push(
                Vec3::new(angle.cos(), 0.0, angle.sin()),
                [0.0; 2],
                Vec4::ONE,
            );
        }
        let mut out = MeshBuffers::default();
        grid.emit(&mut out, Vec3::Y, |_| [0.0; 4]);
        // Only the three sides remain; triangles with two corners at the tip are skipped.
        assert_eq!(out.triangle_count(), 3);
        let pole = Vec3::from_array(out.normals[0]);
        assert!(pole.abs_diff_eq(Vec3::NEG_Y, 1e-5), "{pole}");
        assert!(out.normals[..4].iter().all(|n| *n == out.normals[0]));
    }

    #[test]
    fn push_triangle_turns_to_face_the_normal() {
        let mut out = MeshBuffers::default();
        let corners = [Vec3::ZERO, Vec3::X, Vec3::Z];
        push_triangle(&mut out, corners, Vec3::Y, Vec4::ONE, [0.0; 4]);
        let p = |i: u32| Vec3::from_array(out.positions[i as usize]);
        let [a, b, c] = [out.indices[0], out.indices[1], out.indices[2]];
        assert!((p(b) - p(a)).cross(p(c) - p(a)).dot(Vec3::Y) > 0.0);
    }

    /// Checks that retaining an item also retains every stalk supporting it.
    #[test]
    fn needed_stalks_follow_kept_items_to_the_cluster_base() {
        let stalk = |parent| ClusterStalk {
            points: vec![Vec3::ZERO, Vec3::Y],
            radius: 0.1,
            base_distance: 0.0,
            parent,
        };
        // The retained item needs the main stem and first side stem, but not the second.
        let stalks = [stalk(None), stalk(Some(0)), stalk(Some(0)), stalk(Some(1))];
        let item = |stalk| ClusterItem {
            origin: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            size: 1.0,
            stalk,
            distance: 0.0,
        };
        assert_eq!(
            needed_stalks(&stalks, &[item(Some(3))]),
            [true, true, false, true]
        );
        assert_eq!(needed_stalks(&stalks, &[item(None)]), [false; 4]);
    }

    /// Checks distances before, within, and beyond a path with a right-angle bend.
    #[test]
    fn strip_is_flat_and_as_wide_as_the_stalk() {
        // A path that rises, then runs straight along the facing direction, then turns down.
        let points = [
            Vec3::ZERO,
            Vec3::Y,
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
        ];
        let grid = strip(&points, 0.1, Vec3::X, Vec4::ONE).unwrap();
        let mut out = MeshBuffers::default();
        grid.emit(&mut out, Vec3::X, |_| [0.0; 4]);

        // Two triangles per segment, two edge points per path point, each 0.2 apart.
        assert_eq!(out.triangle_count(), 2 * 3);
        for (k, pair) in out.positions.chunks(2).enumerate() {
            let (a, b) = (Vec3::from_array(pair[0]), Vec3::from_array(pair[1]));
            assert!((a.distance(b) - 0.2).abs() < 1e-5, "point {k}");
            assert!(a.midpoint(b).abs_diff_eq(points[k], 1e-5), "point {k}");
        }
        // The strip's width always runs along Z, so it never twists, even where
        // the path runs along the facing direction; on the rising part it faces X.
        for (k, pair) in out.positions.chunks(2).enumerate() {
            let across = Vec3::from_array(pair[0]) - Vec3::from_array(pair[1]);
            assert!(
                across.normalize().abs().abs_diff_eq(Vec3::Z, 1e-5),
                "point {k}"
            );
        }
        let first = Vec3::from_array(out.normals[0]);
        assert!(first.abs().abs_diff_eq(Vec3::X, 1e-4), "{first}");
        assert!(strip(&points, 0.0, Vec3::X, Vec4::ONE).is_none());
    }

    #[test]
    fn cluster_flutter_grows_to_one_at_the_farthest_point() {
        let cluster = SkeletonCluster {
            parent: crate::Attachment {
                branch: 0,
                section: 0,
                t: 0.0,
            },
            origin: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            stalks: vec![ClusterStalk {
                points: vec![Vec3::ZERO, Vec3::new(0.0, -2.0, 0.0)],
                radius: 0.1,
                base_distance: 0.0,
                parent: None,
            }],
            items: vec![],
        };
        let wind = ClusterWind::new(&cluster, [0.5, 1.0, 0.25]);
        assert_eq!(wind.at(0.0), [0.5, 1.0, 0.25, 0.0]);
        assert_eq!(wind.at(1.0)[3], 0.5);
        assert_eq!(wind.at(5.0)[3], 1.0);
    }
}
