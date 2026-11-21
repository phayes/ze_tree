//! Verify fruit and flower surfaces, colours, and wind data for each cluster arrangement.

use std::ops::Range;

use glam::Vec3;
use ze_tree::{
    ClusterKind, ClusterSpan, ItemDetail, LodDetail, MeshBuffers, MeshPart, Skeleton, TreeMesh,
    TreeOptions, TreePreset, default_lod_levels,
};

const KINDS: [ClusterKind; 5] = [
    ClusterKind::Single,
    ClusterKind::Umbel,
    ClusterKind::Raceme,
    ClusterKind::Panicle,
    ClusterKind::Catkin,
];

/// Prepares a small tree with two fruit and two flower clusters per eligible
/// branch. Both use `kind`; branches must be at least two levels above the trunk.
fn decorated(kind: ClusterKind) -> TreeOptions {
    let mut options = TreePreset::AshSmall.options();
    for (placement, cluster) in [
        (&mut options.fruit.placement, &mut options.fruit.cluster),
        (&mut options.flowers.placement, &mut options.flowers.cluster),
    ] {
        placement.count = 2;
        placement.min_level = 2;
        cluster.kind = kind;
        cluster.items = 5;
    }
    options
}

/// Prepares single-item clusters with no supporting cluster stalk.
/// A fruit’s own short stem is still controlled by its shape settings.
fn bare_singles() -> TreeOptions {
    let mut options = decorated(ClusterKind::Single);
    options.fruit.cluster.stalk_length = 0.0;
    options.flowers.cluster.stalk_length = 0.0;
    options
}

/// Builds a skeleton and its full-detail mesh from valid test options.
/// Panics if skeleton generation rejects the options.
#[track_caller]
fn build(options: &TreeOptions) -> (Skeleton, TreeMesh) {
    let skeleton = Skeleton::generate(options).unwrap();
    let mesh = skeleton.mesh(options, &LodDetail::default());
    (skeleton, mesh)
}

/// Converts a cluster’s vertex range to offsets usable for slicing buffers.
fn vertices(span: &ClusterSpan) -> Range<usize> {
    span.vertices.start as usize..span.vertices.end as usize
}

/// Measures a nonempty cluster’s extent as the diagonal of its bounding box.
fn extent(buffers: &MeshBuffers, span: &ClusterSpan) -> f32 {
    let points = buffers.positions[vertices(span)]
        .iter()
        .map(|p| Vec3::from_array(*p));
    let (min, max) = points.fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
    min.distance(max)
}

/// Checks nonempty mesh buffers for finite positions, unit lighting normals,
/// valid indices, nonzero triangle areas, and colours and flutter within 0–1.
/// Also checks that cluster spans cover all output and identify the original
/// clusters selected by `cluster_stride`.
#[track_caller]
fn assert_well_formed(
    name: &str,
    buffers: &MeshBuffers,
    spans: &[ClusterSpan],
    cluster_stride: usize,
) {
    let n = buffers.vertex_count();
    assert!(n > 0, "{name}: no vertices");
    assert_eq!(buffers.normals.len(), n, "{name}");
    assert_eq!(buffers.uvs.len(), n, "{name}");
    assert_eq!(buffers.wind.len(), n, "{name}");
    assert_eq!(buffers.colors.len(), n, "{name}");
    assert!(buffers.tangents.is_empty(), "{name}");
    assert_eq!(buffers.indices.len() % 3, 0, "{name}");

    for (p, normal) in buffers.positions.iter().zip(&buffers.normals) {
        assert!(Vec3::from_array(*p).is_finite(), "{name}: {p:?}");
        let length = Vec3::from_array(*normal).length();
        assert!(
            (length - 1.0).abs() < 1e-4,
            "{name}: normal length {length}"
        );
    }
    assert!(buffers.indices.iter().all(|&i| (i as usize) < n), "{name}");
    assert!(
        buffers.wind.iter().all(|w| (0.0..=1.0).contains(&w[3])),
        "{name}"
    );
    assert!(
        buffers
            .colors
            .iter()
            .flatten()
            .all(|c| (0.0..=1.0).contains(c)),
        "{name}"
    );

    // Each triangle needs nonzero area and normals pointing toward its front.
    let p = |i: u32| Vec3::from_array(buffers.positions[i as usize]);
    let normal = |i: u32| Vec3::from_array(buffers.normals[i as usize]);
    for t in buffers.indices.chunks(3) {
        let face = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
        assert!(face.length_squared() > 0.0, "{name}: triangle without area");
        let smooth = normal(t[0]) + normal(t[1]) + normal(t[2]);
        assert!(
            face.dot(smooth) > 0.0,
            "{name}: triangle faces away from its normals"
        );
    }

    // Cluster spans must cover the buffers exactly once, without gaps.
    let mut next = (0, 0);
    for (k, span) in spans.iter().enumerate() {
        assert_eq!(span.cluster, k * cluster_stride, "{name}");
        assert_eq!((span.vertices.start, span.indices.start), next, "{name}");
        next = (span.vertices.end, span.indices.end);
    }
    assert_eq!(next, (n as u32, buffers.indices.len() as u32), "{name}");
}

/// Counts groups of triangles connected by shared vertex indices. Supply
/// complete triangles with indices inside `range`; unused vertices do not
/// count as pieces, and coincident positions alone do not connect triangles.
fn connected_pieces(indices: &[u32], range: Range<usize>) -> usize {
    let mut parent: Vec<usize> = (0..range.len()).collect();
    let mut used = vec![false; range.len()];
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for t in indices.chunks(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize - range.start);
        for v in [a, b, c] {
            used[v] = true;
        }
        let ra = root(&mut parent, a);
        for other in [b, c] {
            let ro = root(&mut parent, other);
            parent[ro] = ra;
        }
    }
    (0..range.len())
        .filter(|&i| used[i] && root(&mut parent, i) == i)
        .count()
}

/// Checks that the size threshold removes an entire fruit category, including
/// stalks and catkins, while flowers above the threshold remain.
#[test]
fn min_item_size_leaves_out_small_fruit_and_flowers() {
    for kind in [ClusterKind::Umbel, ClusterKind::Catkin] {
        let mut options = decorated(kind);
        options.fruit.placement.size = 0.4;
        options.flowers.placement.size = 0.6;
        let skeleton = Skeleton::generate(&options).unwrap();
        let detail = LodDetail {
            min_item_size: 0.5,
            ..LodDetail::default()
        };
        let mesh = skeleton.mesh(&options, &detail);
        assert_eq!(mesh.fruit, MeshBuffers::default(), "{kind:?}");
        assert!(mesh.fruit_spans.is_empty(), "{kind:?}");
        assert_eq!(mesh.flower_spans.len(), skeleton.flowers.len(), "{kind:?}");
        assert!(mesh.flowers.triangle_count() > 0, "{kind:?}");
    }
}

#[test]
fn presets_have_empty_fruit_and_flower_meshes() {
    let options = TreePreset::OakMedium.options();
    let (_, mesh) = build(&options);
    assert_eq!(mesh.fruit, MeshBuffers::default());
    assert_eq!(mesh.flowers, MeshBuffers::default());
    assert!(mesh.fruit_spans.is_empty() && mesh.flower_spans.is_empty());
    assert!(mesh.branches.colors.is_empty() && mesh.leaves.colors.is_empty());
}

#[test]
fn every_cluster_kind_builds_well_formed_meshes() {
    for kind in KINDS {
        for maturity in [0.0, 0.5, 1.0] {
            let mut options = decorated(kind);
            options.fruit.maturity = maturity;
            options.flowers.maturity = maturity;
            options.fruit.shape.lobes = 5;
            options.fruit.shape.lobe_depth = 0.3;
            options.fruit.shape.top_dimple = 0.5;
            options.fruit.shape.bottom_taper = 0.5;
            options.fruit.shape.calyx = true;
            options.flowers.shape.rings = 2;
            options.flowers.shape.notch = 0.5;
            options.flowers.shape.curl = 0.5;
            let (skeleton, mesh) = build(&options);

            let name = format!("{kind:?} at maturity {maturity}");
            assert_eq!(mesh.fruit_spans.len(), skeleton.fruit.len(), "{name}");
            assert_eq!(mesh.flower_spans.len(), skeleton.flowers.len(), "{name}");
            assert_well_formed(&format!("fruit, {name}"), &mesh.fruit, &mesh.fruit_spans, 1);
            let flowers = format!("flowers, {name}");
            assert_well_formed(&flowers, &mesh.flowers, &mesh.flower_spans, 1);
        }
    }
}

#[test]
fn fruit_and_flowers_leave_bark_and_leaves_unchanged() {
    let (_, plain) = build(&TreePreset::AshSmall.options());
    let (_, decorated) = build(&decorated(ClusterKind::Raceme));
    assert_eq!(decorated.branches, plain.branches);
    assert_eq!(decorated.leaves, plain.leaves);
}

#[test]
fn unripe_fruit_is_smaller_than_ripe_fruit() {
    let mut options = bare_singles();
    options.fruit.maturity = 0.0;
    let (_, unripe) = build(&options);
    options.fruit.maturity = 1.0;
    let (_, ripe) = build(&options);

    assert_eq!(unripe.fruit_spans.len(), ripe.fruit_spans.len());
    for (small, large) in unripe.fruit_spans.iter().zip(&ripe.fruit_spans) {
        let (small, large) = (extent(&unripe.fruit, small), extent(&ripe.fruit, large));
        // At maturity zero, every fruit dimension should be 30% of its ripe value.
        assert!((small / large - 0.3).abs() < 1e-3, "{small} vs {large}");
    }
}

#[test]
fn fruit_colour_ripens_from_unripe_to_ripe() {
    let mut options = bare_singles();
    options.fruit.shape.unripe_color = 0x00ff00;
    options.fruit.shape.color = 0xff0000;
    options.fruit.shape.blush_amount = 0.0;
    // Remove stems and flower remains to measure only the fruit skin.
    options.fruit.shape.stem_length = 0.0;
    options.fruit.shape.calyx = false;
    for (maturity, expected) in [(0.0, [0.0, 1.0, 0.0, 1.0]), (1.0, [1.0, 0.0, 0.0, 1.0])] {
        options.fruit.maturity = maturity;
        let (_, mesh) = build(&options);
        assert!(!mesh.fruit.colors.is_empty());
        assert!(
            mesh.fruit.colors.iter().all(|c| *c == expected),
            "{maturity}"
        );
    }
}

#[test]
fn blush_colours_the_upper_side_of_ripe_fruit() {
    let mut options = bare_singles();
    options.fruit.shape.color = 0x00ff00;
    options.fruit.shape.blush_color = 0xff0000;
    options.fruit.shape.blush_amount = 1.0;
    // Remove stems and flower remains to measure only the fruit skin.
    options.fruit.shape.stem_length = 0.0;
    options.fruit.shape.calyx = false;
    let (_, mesh) = build(&options);

    for span in &mesh.fruit_spans {
        // Compare the skin colours at the most upward- and downward-facing vertices.
        let range = vertices(span);
        let up = |i: &usize| mesh.fruit.normals[*i][1];
        let top = range
            .clone()
            .max_by(|a, b| up(a).total_cmp(&up(b)))
            .unwrap();
        let bottom = range.min_by(|a, b| up(a).total_cmp(&up(b))).unwrap();
        assert!(
            mesh.fruit.colors[top][0] > 0.2,
            "{:?}",
            mesh.fruit.colors[top]
        );
        assert_eq!(mesh.fruit.colors[bottom], [0.0, 1.0, 0.0, 1.0]);
    }
}

#[test]
fn lobes_change_the_outline() {
    // Measure the minimum and maximum radius around the fruit’s widest ring.
    let ring_radii = |lobes: u32| {
        let mut options = bare_singles();
        options.fruit.shape.stem_length = 0.0;
        options.fruit.shape.segments = 24;
        options.fruit.shape.rings = 8;
        options.fruit.shape.lobes = lobes;
        options.fruit.shape.lobe_depth = 0.4;
        let (skeleton, mesh) = build(&options);

        let item = skeleton.fruit[0].items[0];
        let to_local = item.orientation.inverse();
        let ring = vertices(&mesh.fruit_spans[0]).start + 4 * 25;
        let radii: Vec<f32> = mesh.fruit.positions[ring..ring + 24]
            .iter()
            .map(|p| {
                let local = to_local * (Vec3::from_array(*p) - item.origin);
                local.with_y(0.0).length()
            })
            .collect();
        let min = radii.iter().copied().fold(f32::MAX, f32::min);
        let max = radii.iter().copied().fold(0.0, f32::max);
        (min, max)
    };

    let (min, max) = ring_radii(0);
    assert!(max - min < 1e-4 * max, "smooth fruit: {min} to {max}");
    let (min, max) = ring_radii(6);
    assert!(
        (min / max - 0.6).abs() < 1e-3,
        "ribbed fruit: {min} to {max}"
    );
}

#[test]
fn calyx_adds_one_triangle_per_sepal() {
    let mut options = decorated(ClusterKind::Umbel);
    options.fruit.shape.calyx = false;
    let (skeleton, without) = build(&options);
    options.fruit.shape.calyx = true;
    let (_, with) = build(&options);

    let fruit: usize = skeleton.fruit.iter().map(|c| c.items.len()).sum();
    assert!(fruit > 0);
    let added = with.fruit.triangle_count() - without.fruit.triangle_count();
    assert_eq!(added, 5 * fruit);
}

#[test]
fn cluster_wind_follows_the_attachment_and_grows_along_stalks() {
    let (_, mesh) = build(&decorated(ClusterKind::Raceme));
    for (buffers, spans) in [
        (&mesh.fruit, &mesh.fruit_spans),
        (&mesh.flowers, &mesh.flower_spans),
    ] {
        for span in spans {
            let wind = &buffers.wind[vertices(span)];
            // All cluster vertices inherit their branch attachment’s bending and timing.
            assert!(wind.iter().all(|w| w[..3] == wind[0][..3]));
            // Extra flutter is zero at the stem base and reaches one at the farthest point.
            assert_eq!(wind[0][3], 0.0);
            let most = wind.iter().map(|w| w[3]).fold(0.0, f32::max);
            assert_eq!(most, 1.0);
        }
    }
}

#[test]
fn a_bud_is_smaller_than_an_open_flower() {
    let mut options = bare_singles();
    options.flowers.maturity = 0.0;
    let (_, bud) = build(&options);
    options.flowers.maturity = 0.5;
    let (_, open) = build(&options);

    assert!(!bud.flower_spans.is_empty());
    for (closed, opened) in bud.flower_spans.iter().zip(&open.flower_spans) {
        let (closed, opened) = (extent(&bud.flowers, closed), extent(&open.flowers, opened));
        assert!(closed < 0.8 * opened, "bud {closed}, open {opened}");
    }
}

#[test]
fn each_petal_is_a_separate_piece() {
    let mut options = bare_singles();
    options.flowers.shape.petal_count = 5;
    options.flowers.shape.rings = 2;
    options.flowers.shape.center_size = 0.0;
    let (_, mesh) = build(&options);

    assert!(!mesh.flower_spans.is_empty());
    for span in &mesh.flower_spans {
        let indices = &mesh.flowers.indices[span.indices.start as usize..span.indices.end as usize];
        assert_eq!(connected_pieces(indices, vertices(span)), 10);
    }
}

#[test]
fn flower_colour_follows_bud_open_and_faded_stages() {
    let mut options = bare_singles();
    options.flowers.shape.center_size = 0.0;
    options.flowers.shape.bud_color = 0xff0000;
    options.flowers.shape.petal_color = 0x00ff00;
    options.flowers.shape.base_color = 0x00ff00;
    options.flowers.shape.fade_color = 0x0000ff;
    for (maturity, expected) in [
        (0.0, [1.0, 0.0, 0.0, 1.0]),
        (0.5, [0.0, 1.0, 0.0, 1.0]),
        (1.0, [0.0, 0.0, 1.0, 1.0]),
    ] {
        options.flowers.maturity = maturity;
        let (_, mesh) = build(&options);
        assert!(!mesh.flowers.colors.is_empty());
        assert!(
            mesh.flowers.colors.iter().all(|c| *c == expected),
            "{maturity}"
        );
    }
}

#[test]
fn faded_petals_droop_below_open_petals() {
    // Measure average petal height from the flower base along its opening direction.
    let mean_rise = |maturity: f32| {
        let mut options = bare_singles();
        options.flowers.placement.facing = ze_tree::Facing::Up;
        options.flowers.placement.facing_jitter = 0.0;
        options.flowers.shape.center_size = 0.0;
        options.flowers.maturity = maturity;
        let (skeleton, mesh) = build(&options);
        let mut sum = 0.0;
        let mut count = 0;
        for (cluster, span) in skeleton.flowers.iter().zip(&mesh.flower_spans) {
            let item = cluster.items[0];
            let facing = item.orientation * Vec3::Y;
            for p in &mesh.flowers.positions[vertices(span)] {
                sum += (Vec3::from_array(*p) - item.origin).dot(facing);
                count += 1;
            }
        }
        sum / count as f32
    };
    assert!(mean_rise(1.0) < mean_rise(0.5));
}

#[test]
fn catkins_ignore_petal_settings() {
    let options = decorated(ClusterKind::Catkin);
    let mut changed = options.clone();
    let shape = &mut changed.flowers.shape;
    shape.petal_count = 9;
    shape.rings = 3;
    shape.petal_length = 0.9;
    shape.petal_width = 0.1;
    shape.cup = 0.9;
    shape.curl = 0.7;
    shape.notch = 0.6;
    shape.petal_segments = 7;
    shape.center_size = 0.4;
    shape.center_color = 0x123456;
    shape.base_color = 0x654321;

    let (_, original) = build(&options);
    let (_, varied) = build(&changed);
    assert!(original.flowers.vertex_count() > 0);
    assert_eq!(original.flowers, varied.flowers);
}

#[test]
fn single_item_follows_its_skeleton_orientation() {
    // Upward-facing flowers should place their petals above their attachment.
    let mut options = bare_singles();
    options.flowers.placement.facing = ze_tree::Facing::Up;
    options.flowers.placement.facing_jitter = 0.0;
    options.flowers.maturity = 0.5;
    let (skeleton, mesh) = build(&options);
    for (cluster, span) in skeleton.flowers.iter().zip(&mesh.flower_spans) {
        let item = cluster.items[0];
        let up = (item.orientation * Vec3::Y).dot(Vec3::Y);
        assert!((up - 1.0).abs() < 1e-5);
        let lowest = mesh.flowers.positions[vertices(span)]
            .iter()
            .map(|p| p[1])
            .fold(f32::MAX, f32::min);
        assert!(
            lowest >= item.origin.y - 1e-4,
            "{lowest} below {}",
            item.origin.y
        );
    }
}

/// Fruit and flower detail levels exercised by the mesh tests.
const DETAILS: [ItemDetail; 3] = [ItemDetail::Full, ItemDetail::Simple, ItemDetail::Minimal];

/// Builds a mesh with the requested fruit and flower detail, item stride,
/// and item scale. Keeps every cluster and uses full detail for bark and leaves.
fn mesh_at(
    skeleton: &Skeleton,
    options: &TreeOptions,
    item_detail: ItemDetail,
    item_stride: u32,
    item_scale: f32,
) -> TreeMesh {
    let detail = LodDetail {
        item_detail,
        item_stride,
        item_scale,
        ..LodDetail::default()
    };
    skeleton.mesh(options, &detail)
}

/// Creates a test tree with open flowers and two petal layers, allowing the
/// tests to check that lower detail removes flower geometry.
fn detailed(kind: ClusterKind) -> TreeOptions {
    let mut options = decorated(kind);
    options.flowers.shape.rings = 2;
    options.flowers.maturity = 0.5;
    options
}

#[test]
fn every_detail_level_builds_well_formed_meshes() {
    for kind in KINDS {
        let mut options = detailed(kind);
        let skeleton = Skeleton::generate(&options).unwrap();
        for maturity in [0.0, 0.5, 1.0] {
            options.fruit.maturity = maturity;
            options.flowers.maturity = maturity;
            for item_detail in DETAILS {
                for stride in [1, 2] {
                    let detail = LodDetail {
                        cluster_stride: stride,
                        item_stride: stride,
                        item_scale: 1.25,
                        item_detail,
                        ..LodDetail::default()
                    };
                    let mesh = skeleton.mesh(&options, &detail);
                    let name =
                        format!("{kind:?}, {item_detail:?}, maturity {maturity}, stride {stride}");
                    let stride = stride as usize;
                    assert_eq!(
                        mesh.fruit_spans.len(),
                        skeleton.fruit.len().div_ceil(stride)
                    );
                    assert_eq!(
                        mesh.flower_spans.len(),
                        skeleton.flowers.len().div_ceil(stride)
                    );
                    assert_well_formed(
                        &format!("fruit, {name}"),
                        &mesh.fruit,
                        &mesh.fruit_spans,
                        stride,
                    );
                    let flowers = format!("flowers, {name}");
                    assert_well_formed(&flowers, &mesh.flowers, &mesh.flower_spans, stride);
                }
            }
        }
    }
}

#[test]
fn lower_detail_uses_fewer_triangles() {
    for kind in KINDS {
        let options = detailed(kind);
        let skeleton = Skeleton::generate(&options).unwrap();
        let counts = DETAILS.map(|d| {
            let mesh = mesh_at(&skeleton, &options, d, 1, 1.0);
            (mesh.fruit.triangle_count(), mesh.flowers.triangle_count())
        });
        let [full, simple, minimal] = counts;
        assert!(
            full.0 > simple.0 && simple.0 > minimal.0,
            "{kind:?} fruit: {counts:?}"
        );
        assert!(
            full.1 > simple.1 && simple.1 > minimal.1,
            "{kind:?} flowers: {counts:?}"
        );
    }
}

#[test]
fn skipped_clusters_leave_the_kept_ones_unchanged() {
    let options = detailed(ClusterKind::Raceme);
    let skeleton = Skeleton::generate(&options).unwrap();
    let full = skeleton.mesh(&options, &LodDetail::default());
    let detail = LodDetail {
        cluster_stride: 2,
        ..LodDetail::default()
    };
    let half = skeleton.mesh(&options, &detail);

    for (buffers, spans, all, all_spans) in [
        (
            &half.fruit,
            &half.fruit_spans,
            &full.fruit,
            &full.fruit_spans,
        ),
        (
            &half.flowers,
            &half.flower_spans,
            &full.flowers,
            &full.flower_spans,
        ),
    ] {
        assert_eq!(spans.len(), all_spans.len().div_ceil(2));
        for span in spans {
            let original = &all_spans[span.cluster];
            let (kept, source) = (vertices(span), vertices(original));
            assert_eq!(
                buffers.positions[kept.clone()],
                all.positions[source.clone()]
            );
            assert_eq!(buffers.colors[kept.clone()], all.colors[source.clone()]);
            assert_eq!(buffers.wind[kept], all.wind[source]);
        }
    }
}

#[test]
fn item_stride_drops_bare_stalks_and_keeps_wind() {
    // A stride of two retains items 0, 2, and 4 and their individual stalks.
    let options = detailed(ClusterKind::Umbel);
    let skeleton = Skeleton::generate(&options).unwrap();
    let full = mesh_at(&skeleton, &options, ItemDetail::Full, 1, 1.0);
    let thinned = mesh_at(&skeleton, &options, ItemDetail::Full, 2, 1.0);
    for (all, kept) in [
        (&full.fruit_spans, &thinned.fruit_spans),
        (&full.flower_spans, &thinned.flower_spans),
    ] {
        for (a, k) in all.iter().zip(kept) {
            assert_eq!(a.vertices.len() * 3, k.vertices.len() * 5);
        }
    }

    // Thinning a cluster must preserve the wind values used by its retained geometry.
    for kind in KINDS {
        let options = detailed(kind);
        let skeleton = Skeleton::generate(&options).unwrap();
        let full = mesh_at(&skeleton, &options, ItemDetail::Full, 1, 1.0);
        let thinned = mesh_at(&skeleton, &options, ItemDetail::Simple, 2, 1.25);
        for (all, all_spans, kept, kept_spans) in [
            (
                &full.fruit,
                &full.fruit_spans,
                &thinned.fruit,
                &thinned.fruit_spans,
            ),
            (
                &full.flowers,
                &full.flower_spans,
                &thinned.flowers,
                &thinned.flower_spans,
            ),
        ] {
            for (a, k) in all_spans.iter().zip(kept_spans) {
                let before = &all.wind[vertices(a)];
                for w in &kept.wind[vertices(k)] {
                    assert!(before.contains(w), "{kind:?}: new wind value {w:?}");
                }
            }
        }
    }
}

#[test]
fn item_scale_resizes_each_item() {
    let options = bare_singles();
    let skeleton = Skeleton::generate(&options).unwrap();
    let normal = mesh_at(&skeleton, &options, ItemDetail::Full, 1, 1.0);
    let doubled = mesh_at(&skeleton, &options, ItemDetail::Full, 1, 2.0);
    for (a, b, spans_a, spans_b) in [
        (
            &normal.fruit,
            &doubled.fruit,
            &normal.fruit_spans,
            &doubled.fruit_spans,
        ),
        (
            &normal.flowers,
            &doubled.flowers,
            &normal.flower_spans,
            &doubled.flower_spans,
        ),
    ] {
        for (sa, sb) in spans_a.iter().zip(spans_b) {
            let ratio = extent(b, sb) / extent(a, sa);
            assert!((ratio - 2.0).abs() < 1e-3, "{ratio}");
        }
    }
}

#[test]
fn simpler_fruit_drop_the_calyx_then_the_stem() {
    let mut options = bare_singles();
    options.fruit.shape.calyx = true;
    options.fruit.shape.stem_length = 0.2;
    let skeleton = Skeleton::generate(&options).unwrap();
    // Full detail has a body, stem, and five flower remains; simple drops the
    // flower remains, and minimal also drops the stem.
    for (detail, pieces) in [
        (ItemDetail::Full, 7),
        (ItemDetail::Simple, 2),
        (ItemDetail::Minimal, 1),
    ] {
        let mesh = mesh_at(&skeleton, &options, detail, 1, 1.0);
        for span in &mesh.fruit_spans {
            let range = vertices(span);
            let indices =
                &mesh.fruit.indices[span.indices.start as usize..span.indices.end as usize];
            assert_eq!(connected_pieces(indices, range), pieces, "{detail:?}");
        }
    }
}

#[test]
fn simple_flowers_keep_one_layer_of_two_segment_petals() {
    let mut options = bare_singles();
    options.flowers.shape.petal_count = 5;
    options.flowers.shape.rings = 3;
    options.flowers.shape.petal_segments = 6;
    options.flowers.shape.center_size = 0.3;
    let skeleton = Skeleton::generate(&options).unwrap();
    let mesh = mesh_at(&skeleton, &options, ItemDetail::Simple, 1, 1.0);
    for span in &mesh.flower_spans {
        // Each of the five petals has two divisions of four triangles each;
        // the centre adds six triangles.
        assert_eq!(span.indices.len() / 3, 5 * 2 * 4 + 6);
        let indices = &mesh.flowers.indices[span.indices.start as usize..span.indices.end as usize];
        assert_eq!(connected_pieces(indices, vertices(span)), 5 + 1);
    }
}

#[test]
fn minimal_flowers_are_single_stars() {
    let mut options = bare_singles();
    options.flowers.shape.petal_count = 6;
    options.flowers.shape.rings = 2;
    let skeleton = Skeleton::generate(&options).unwrap();
    for maturity in [0.0, 0.5, 1.0] {
        options.flowers.maturity = maturity;
        let mesh = mesh_at(&skeleton, &options, ItemDetail::Minimal, 1, 1.0);
        for span in &mesh.flower_spans {
            // Each petal tip joins the centre and its two neighbouring notches.
            assert_eq!(span.indices.len() / 3, 2 * 6, "maturity {maturity}");
            let indices =
                &mesh.flowers.indices[span.indices.start as usize..span.indices.end as usize];
            assert_eq!(connected_pieces(indices, vertices(span)), 1);
        }
    }

    // Closed buds use only the bud colour; fully open flowers include the centre colour.
    let shape = &mut options.flowers.shape;
    shape.bud_color = 0xff0000;
    shape.center_color = 0x0000ff;
    shape.center_size = 0.3;
    let centre = [0.0, 0.0, 1.0, 1.0];
    for (maturity, shows_centre) in [(0.0, false), (0.5, true)] {
        options.flowers.maturity = maturity;
        let mesh = mesh_at(&skeleton, &options, ItemDetail::Minimal, 1, 1.0);
        let found = mesh.flowers.colors.contains(&centre);
        assert_eq!(found, shows_centre, "maturity {maturity}");
        if maturity == 0.0 {
            assert!(
                mesh.flowers
                    .colors
                    .iter()
                    .all(|c| *c == [1.0, 0.0, 0.0, 1.0])
            );
        }
    }
}

#[test]
fn minimal_catkins_are_one_body_whatever_the_item_stride() {
    let mut options = decorated(ClusterKind::Catkin);
    options.fruit.cluster.items = 12;
    options.flowers.cluster.items = 12;
    let skeleton = Skeleton::generate(&options).unwrap();
    let all = mesh_at(&skeleton, &options, ItemDetail::Minimal, 1, 1.0);
    let thinned = mesh_at(&skeleton, &options, ItemDetail::Minimal, 3, 1.5);
    assert_eq!(all.fruit, thinned.fruit);
    assert_eq!(all.flowers, thinned.flowers);

    // Each catkin has a supporting stem and one body, with roughly the same
    // overall extent as its full-detail mesh.
    let full = mesh_at(&skeleton, &options, ItemDetail::Full, 1, 1.0);
    for (buffers, spans, full_buffers, full_spans) in [
        (&all.fruit, &all.fruit_spans, &full.fruit, &full.fruit_spans),
        (
            &all.flowers,
            &all.flower_spans,
            &full.flowers,
            &full.flower_spans,
        ),
    ] {
        for (span, full_span) in spans.iter().zip(full_spans) {
            let indices = &buffers.indices[span.indices.start as usize..span.indices.end as usize];
            assert_eq!(connected_pieces(indices, vertices(span)), 2);
            let ratio = extent(buffers, span) / extent(full_buffers, full_span);
            assert!((0.8..=1.05).contains(&ratio), "extent ratio {ratio}");
        }
    }
}

#[test]
fn default_levels_simplify_fruit_and_flowers_further_away() {
    let [near, middle, far, farthest] = default_lod_levels().map(|level| level.detail);
    assert_eq!(near.item_detail, ItemDetail::Full);
    assert_eq!(middle.item_detail, ItemDetail::Simple);
    assert_eq!(far.item_detail, ItemDetail::Minimal);
    assert_eq!(farthest.item_detail, ItemDetail::Minimal);
    assert_eq!((far.item_stride, far.item_scale), (2, 1.25));
    assert_eq!(
        [near, middle, far, farthest].map(|d| d.flat_item_size),
        [0.0, 0.0, 0.75, 1.5]
    );
    // The size filter is disabled for the first three levels and set to 0.5 for the fourth.
    assert_eq!(
        [near, middle, far, farthest].map(|d| d.min_item_size),
        [0.0, 0.0, 0.0, 0.5]
    );
    // Farther levels must select a subset of the items and clusters kept nearby.
    for (nearer, farther) in [(near, middle), (middle, far), (far, farthest)] {
        assert_eq!(farther.item_stride % nearer.item_stride.max(1), 0);
        assert_eq!(farther.cluster_stride % nearer.cluster_stride.max(1), 0);
    }
}

#[test]
fn fruit_and_flower_parts_copy_their_buffers() {
    let (_, mesh) = build(&decorated(ClusterKind::Umbel));
    for (part, buffers) in [
        (MeshPart::Fruit, &mesh.fruit),
        (MeshPart::Flowers, &mesh.flowers),
    ] {
        let copy = mesh.part(part);
        assert_eq!(&copy.buffers, buffers);
        assert_eq!(
            copy.source_vertices,
            (0..buffers.vertex_count() as u32).collect::<Vec<_>>()
        );
        assert!(!part.is_bark() && !part.is_leaves());
    }
    assert_eq!(MeshPart::Fruit.name(), "fruit");
    assert_eq!(MeshPart::Flowers.name(), "flowers");
    // Branch grouping omits fruit and flowers; callers request those parts separately.
    let parts = ze_tree::BranchGrouping::Merged.parts(&mesh);
    assert!(parts.iter().all(|p| p.is_bark() || p.is_leaves()));
}

/// Meshes at minimal detail with every other item kept at 1.25× and flat
/// drawing for items configured smaller than `flat_item_size`.
fn mesh_flat(skeleton: &Skeleton, options: &TreeOptions, flat_item_size: f32) -> TreeMesh {
    let detail = LodDetail {
        item_detail: ItemDetail::Minimal,
        item_stride: 2,
        item_scale: 1.25,
        flat_item_size,
        ..LodDetail::default()
    };
    skeleton.mesh(options, &detail)
}

/// Prepares small fruit and flowers in `kind` clusters, with stalks too thin
/// to draw so that only the items themselves are meshed.
fn small_items(kind: ClusterKind) -> TreeOptions {
    let mut options = decorated(kind);
    for (placement, cluster) in [
        (&mut options.fruit.placement, &mut options.fruit.cluster),
        (&mut options.flowers.placement, &mut options.flowers.cluster),
    ] {
        placement.size = 0.3;
        cluster.stalk_radius = 0.0;
    }
    options
}

#[test]
fn small_items_drawn_flat_keep_every_item() {
    for kind in [
        ClusterKind::Single,
        ClusterKind::Umbel,
        ClusterKind::Raceme,
        ClusterKind::Panicle,
    ] {
        let options = small_items(kind);
        let skeleton = Skeleton::generate(&options).unwrap();
        let mesh = mesh_flat(&skeleton, &options, 0.75);
        assert_well_formed("fruit", &mesh.fruit, &mesh.fruit_spans, 1);
        assert_well_formed("flowers", &mesh.flowers, &mesh.flower_spans, 1);

        // Every fruit is a six-triangle disc and every flower one triangle, despite the item stride.
        for (cluster, span) in skeleton.fruit.iter().zip(&mesh.fruit_spans) {
            assert_eq!(span.indices.len() / 3, 6 * cluster.items.len(), "{kind:?}");
        }
        for (cluster, span) in skeleton.flowers.iter().zip(&mesh.flower_spans) {
            assert_eq!(span.indices.len() / 3, cluster.items.len(), "{kind:?}");
        }
    }
}

#[test]
fn flat_fruit_discs_face_out_from_the_trunk_at_varied_angles() {
    let options = small_items(ClusterKind::Raceme);
    let skeleton = Skeleton::generate(&options).unwrap();
    let mesh = mesh_flat(&skeleton, &options, 0.75);
    for (cluster, span) in skeleton.fruit.iter().zip(&mesh.fruit_spans) {
        // Each disc has one flat lighting direction; neighbouring discs should differ.
        let normals: Vec<Vec3> = mesh.fruit.normals[vertices(span)]
            .chunks(2 * 7)
            .map(|disc| Vec3::from_array(disc[0]))
            .collect();
        assert!(normals.len() > 2);
        for pair in normals.windows(2) {
            assert!(pair[0].dot(pair[1]) < 0.999, "{pair:?}");
        }

        // Every disc stays within 0.6 radians of facing straight out from the
        // trunk (or straight in, for its back), so none is seen edge-on from outside.
        let out = Vec3::new(cluster.origin.x, 0.0, cluster.origin.z).normalize();
        for n in &normals {
            assert!(n.dot(out).abs() >= 0.6_f32.cos() - 1e-4, "{n} vs {out}");
        }
    }
}

#[test]
fn items_at_or_above_the_flat_size_keep_their_shape() {
    let mut options = small_items(ClusterKind::Single);
    options.fruit.placement.size = 0.75;
    options.flowers.placement.size = 0.75;
    let skeleton = Skeleton::generate(&options).unwrap();
    assert_eq!(
        mesh_flat(&skeleton, &options, 0.75),
        mesh_flat(&skeleton, &options, 0.0)
    );
}

#[test]
fn catkins_ignore_the_flat_size() {
    let options = small_items(ClusterKind::Catkin);
    let skeleton = Skeleton::generate(&options).unwrap();
    assert_eq!(
        mesh_flat(&skeleton, &options, 0.75),
        mesh_flat(&skeleton, &options, 0.0)
    );
}

#[test]
fn minimal_stalks_are_flat_strips() {
    // Single flowers on curved stalks, each drawn as one triangle.
    let mut options = decorated(ClusterKind::Single);
    options.flowers.placement.size = 0.3;
    let skeleton = Skeleton::generate(&options).unwrap();
    let mesh = mesh_flat(&skeleton, &options, 0.75);
    for (cluster, span) in skeleton.flowers.iter().zip(&mesh.flower_spans) {
        let segments = cluster.stalks[0].points.len() - 1;
        assert_eq!(span.indices.len() / 3, 2 * segments + 1);
    }
}

#[test]
fn flat_items_still_follow_item_scale() {
    let mut options = bare_singles();
    options.fruit.placement.size = 0.3;
    options.flowers.placement.size = 0.3;
    let skeleton = Skeleton::generate(&options).unwrap();
    let at = |item_scale| {
        let detail = LodDetail {
            item_detail: ItemDetail::Minimal,
            item_scale,
            flat_item_size: 0.75,
            ..LodDetail::default()
        };
        skeleton.mesh(&options, &detail)
    };
    let (normal, doubled) = (at(1.0), at(2.0));
    for (a, b, spans_a, spans_b) in [
        (
            &normal.fruit,
            &doubled.fruit,
            &normal.fruit_spans,
            &doubled.fruit_spans,
        ),
        (
            &normal.flowers,
            &doubled.flowers,
            &normal.flower_spans,
            &doubled.flower_spans,
        ),
    ] {
        for (sa, sb) in spans_a.iter().zip(spans_b) {
            let ratio = extent(b, sb) / extent(a, sa);
            assert!((ratio - 2.0).abs() < 1e-3, "{ratio}");
        }
    }
}
