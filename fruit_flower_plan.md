# ze_tree — fruit and flowers plan

## Bottom line

Add fruit and flowers as two new, optional parts of a tree. Each is built from three
shared pieces:

1. **Placement**: where clusters attach on the tree, how many there are, and which
   way they face.
2. **Cluster**: how the items are arranged on their stalks: single, umbel, raceme,
   panicle or catkin.
3. **Item shape**: a fruit body (a revolved profile with a stem) or a flower head
   (rings of curved petals with a centre).

Items are real geometry coloured with vertex colours, so they need no new textures.
A single `maturity` value from 0 to 1 moves flowers from bud to open to fading, and
fruit from small and green to full-size and ripe.

**Key architectural changes**

- **`TreeOptions` gains two fields:**
  - `fruit: FruitOptions` and `flowers: FlowerOptions`;
  - both are off by default (cluster count 0);
  - JSON loads them with the existing partial-merge `from_json`. Old ez-tree JSON
    still loads, and `to_json` writes the new keys.
- **Shared option types:**
  - `ClusterPlacement`: levels, tips or along the branch, density, start, chance,
    facing, size and size variance;
  - `ClusterShape`: kind, item count, spread, stalk length and droop, axis length,
    cone taper;
  - `FruitShape` and `FlowerShape` hold the item settings.
- **Skeleton:**
  - new fields `Skeleton::fruit` and `Skeleton::flowers`, each a `Vec<SkeletonCluster>`;
  - a `SkeletonCluster` has an attachment, an origin, an orientation, its stalk
    polylines, and per-item origin, orientation and size;
  - the clusters are placed after branches and leaves, using **separate RNG
    streams** derived from `seed`;
  - so the branch and leaf output does not change at all, and every ez-tree
    fixture still passes unchanged.
- **Meshes:**
  - new buffers `TreeMesh::fruit` and `TreeMesh::flowers`, with span lists like
    `leaf_spans`;
  - `MeshBuffers` gains a `colors: Vec<[f32; 4]>` field, left empty for bark and
    leaves;
  - stems, petals, centres and fruit all share one buffer per part, and vertex
    colours tell them apart.
- **Materials:**
  - new `FruitMaterial` (glossier, single-sided) and `FlowerMaterial` (double-sided,
    with optional light transmission);
  - both multiply their base colour by the vertex colour.
- **Wind:**
  - clusters copy trunk bend, branch weight and phase from their attachment, as
    leaves already do;
  - the `flutter` channel grows along the stalk, so hanging fruit and racemes swing;
  - the existing shader channels are reused, with no new vertex attribute. This
    still needs to be checked visually; see Open items.
- **Built on top, extended rather than redesigned:**
  - LOD: `LodDetail` gains a cluster stride and simpler item shapes;
  - parts: new `MeshPart::Fruit` and `MeshPart::Flowers`;
  - rig: `MeshSkin` gains `fruit` and `flowers`, which use the leaf rule (the
    attachment joint);
  - export: OBJ and glTF write the new meshes and materials, with `COLOR_0` in glTF;
  - jiggle: varies shape numbers and counts, but not colours, kinds or maturity;
  - Bevy: two new `TreePartKind`s, with viewer controls.
- **Presets:**
  - new presets: apple, cherry blossom, magnolia, laburnum, dogwood, oceanspray,
    pussy willow and grape vine (on the trellis);
  - `TreePreset::ALL` lists every preset;
  - a new `TreePreset::EZ_TREE` holds the original 16, for fixture tests and
    ez-tree compatibility.

**Trade-offs**

- **Geometry instead of textures:** there's no art pipeline, and the look stays
  consistent close up. The cost is more triangles: a laburnum with petal curvature
  can be heavy. Item detail settings and LOD strides handle this, and the budget is
  checked per preset.
- **One `fruit` and one `flowers` per tree, not a list:** this covers flowers and
  fruit on the same tree, shown at different maturities. Several fruit types on one
  tree are out of scope.
- **Placement ignores collisions:** clusters can overlap leaves or branches, the same
  as ez-tree leaves do.

---

## Design details

### Placement (`ClusterPlacement`)

| Field | Meaning |
|---|---|
| `min_level` | Lowest branch level that can carry clusters. Old-wood flowering is `min_level` 1 or 2. |
| `tips` | Chance (0–1) that a branch tip at an eligible level gets a cluster. |
| `count` | Clusters along each eligible branch, excluding tips. 0 means tips only. |
| `start` | Earliest position along a branch, from 0 to 1, as for leaves. |
| `facing` | `Hang` (fruit and laburnum follow gravity), `Up` (dogwood and magnolia), `Outward` (cherry: away from the branch). |
| `facing_jitter` | Random tilt from `facing`, in degrees. |
| `size`, `size_variance` | Item size in tree units, and its random variation, as for leaves. |

### Cluster (`ClusterShape`)

| `kind` | Arrangement | Used by |
|---|---|---|
| `Single` | One item on one stalk | apple, magnolia, dogwood |
| `Umbel` | `items` stalks fanning from one point, by `spread` degrees | cherry fruit and blossom |
| `Raceme` | `items` on short pedicels along an axis of `axis_length` that droops by `droop` | laburnum |
| `Panicle` | A raceme whose side stalks branch again, shrinking toward the tip by `taper` | grapes, oceanspray |
| `Catkin` | One elongated body made of packed bumps; ignores petal settings | pussy willow, mulberry |

Shared fields: `items`, `items_variance`, `stalk_length`, `stalk_radius`, `stalk_color`.
Panicles also use `side_stalks` (how many side stalks share the items) and
`side_length` (the lowest side stalk's length as a fraction of `axis_length`).
For catkins, `stalk_length` is the bare stalk before the catkin body.

Stalks are sampled polylines. The skeleton keeps them so that wind, rig and meshing
all see the same curve.

### Fruit (`FruitShape`)

- **Body:** a sphere revolved around its axis, with:
  - `elongation`: below 1 is flattened, above 1 is plum or lemon shaped;
  - `top_dimple` and `bottom_taper`: the apple top and pear bottom;
  - `lobes` and `lobe_depth`: ribs, as on a pumpkin or a ribbed apple;
  - `segments` and `rings` for mesh detail.
- **Stem:** `stem_length`, `stem_radius` and `stem_curve`.
- **Calyx:** `calyx` (bool), a small star at the base.
- **Colour:** `color` (ripe), `unripe_color`, `blush_color` and `blush_amount`. The
  blush is a vertex-colour gradient on the side that faces up and out.
- **Maturity:** scales size from 30% to 100% and blends from `unripe_color` to `color`.

### Flower (`FlowerShape`)

- **Petals:**
  - `petal_count`, `rings` (each extra ring is offset and smaller), `petal_length`
    and `petal_width`;
  - `cup`: 0 is flat (dogwood), 1 is tulip-like (magnolia);
  - `curl`: the tip bends backward;
  - `notch`: cherry and dogwood tips;
  - `petal_segments`: curvature detail.
- **Centre:** `center_size` and `center_color`, as a small domed disc.
- **Colour:** `petal_color`, `base_color` (the gradient toward the petal base),
  `bud_color` and `fade_color`.
- **Maturity:**
  - 0 is a closed bud: petals folded into a cone, coloured `bud_color`;
  - 0.5 is fully open;
  - 1 is fading: petals droop and blend toward `fade_color`.

### Random streams

`Rng::new(seed ^ FRUIT_SALT)` and `Rng::new(seed ^ FLOWER_SALT)`.

- Turning flowers on never moves the fruit, and neither changes branches or leaves.
- Items are placed per branch in branch order, so the result depends only on the
  options.

---

## Phases

### F0 — Options ✅ done

Also added: `OptionsError::InvalidSetting { field, requirement }` with the JSON path
of the setting; `cluster.items` must be at least 1. Fruit stem and flower petal
lengths are fractions of the item size; stalk and axis lengths are in tree units.

- Add `ClusterPlacement`, `ClusterShape`, `ClusterKind`, `Facing`, `FruitShape`,
  `FlowerShape`, `FruitOptions` and `FlowerOptions`, with serde in camelCase, defaults
  (off), and `maturity`.
- In `validate`, reject values that would produce bad geometry:
  - `maturity` outside 0–1;
  - zero segments when enabled;
  - negative sizes.
- **Tests:**
  - JSON round-trip;
  - partial JSON;
  - old ez-tree JSON loads with fruit and flowers off;
  - every existing options test is unchanged.

### F1 — Placement and clusters in the skeleton ✅ done

Implemented in `src/cluster.rs` as `SkeletonCluster`, `ClusterStalk` and `ClusterItem`.
Stalks store `base_distance` and items store `distance` along the stalks, ready for the
F2 wind swing. Side clusters start on the branch surface; tip clusters grow only on
branch ends with no continuing branch.

- Add `SkeletonCluster` and the cluster pass that runs after the branch and leaf
  generation, with its own RNG streams.
- **Tests:**
  - every ez-tree fixture is unchanged;
  - generation is deterministic;
  - when disabled, nothing is generated;
  - clusters appear only on levels from `min_level` up, and after `start`;
  - `Hang` stalk ends point downward;
  - item counts are within the variance;
  - turning flowers on doesn't move the fruit.

### F2 — Fruit mesh ✅ done

Implemented in `src/cluster_mesh/` (`mod.rs` shared grid, stalk tubes, wind and colour;
`fruit.rs`). `TreeMesh` gains `fruit`, `flowers`, `fruit_spans` and `flower_spans`
(`ClusterSpan`, one per cluster). Flutter runs from 0 at the cluster origin to 1 at its
farthest point; each item body moves rigidly with its attachment point. Fruit in catkin
clusters have no stem. Calyx sepals use the cluster's `stalk_color`. LOD was added in F4.

- Add `MeshBuffers::colors`.
- Mesh the revolved body, the stem tube (reusing the branch tube code), the calyx
  and the blush.
- Fill the wind channels from the attachment, plus the stalk swing.
- **Tests:**
  - normals have unit length;
  - indices are in range;
  - colour and wind buffers have one entry per vertex;
  - maturity 0 gives smaller items than maturity 1;
  - lobes change the outline.

### F3 — Flower mesh and catkins ✅ done

Implemented in `src/cluster_mesh/flower.rs`. Maturity 0–0.5 opens the bud, 0.5–1 fades
and droops the petals. Catkin flowers are small rounded bumps using only `petal_color`,
`bud_color` and `fade_color`. Checkpoint renders of every kind were reviewed in a
throwaway three.js page; Bevy rendering comes in F5.

- Mesh the petal strips (cup, curl, notch), the rings, the centre disc, the
  bud-to-fading maturity curve, and catkin bodies.
- **Tests:** the same buffer checks as F2, plus:
  - a bud's extent is smaller than an open flower's;
  - petal count equals the triangle groups;
  - a catkin ignores the petal settings.
- **Checkpoint:** screenshots of each cluster kind for the user to review before
  tuning presets.

### F4 — LOD, parts and materials ✅ done

Agreed with the user before starting; this replaces the original `simple_items` idea.

- `LodDetail` gains:
  - `cluster_stride` (keep every Nth cluster; spans keep the skeleton cluster index);
  - `item_stride` (keep every Nth item in a cluster; stalks no kept item needs are
    dropped, using the new `ClusterStalk::parent`);
  - `item_scale` (enlarge kept items, like `leaf_scale`);
  - `item_detail: ItemDetail { Full, Simple, Minimal }`.
- `ItemDetail::Simple`:
  - fruit bodies with about half the segments and rings (at least 6×4), and 3-sided stems;
  - no calyx;
  - one petal ring with at most 2 segments (1 looked like thin blades in the renders);
  - a cone-shaped flower centre;
  - 4×3 catkin bumps and 3-sided stalks.
- `ItemDetail::Minimal`:
  - fruit at 5×3 with no stem;
  - each flower drawn as one flat star (`2 × petal_count` triangles), whose centre
    colour fades in as the flower opens;
  - each catkin, fruit or flower, becomes one tapered body along its stem, from the
    back of the first item to the tip of the last; it ignores `item_stride` and
    `item_scale`.
- Wind is measured over the whole cluster, so kept vertices sway identically at every level.
- Default levels:
  - level 0: Full;
  - level 1: Simple;
  - level 2: Minimal, with `item_stride` 2 and `item_scale` 1.25. Clusters are all kept.
- **Measured triangle counts per cluster, levels 0 → 1 → 2:**
  - grape panicle: 5784 → 1452 → 582;
  - oceanspray panicle: 7440 → 2160 → 720;
  - willow catkin: 1176 → 528 → 118.
- `MeshPart::Fruit` and `MeshPart::Flowers` work with `TreeMesh::part`, and
  `MeshPart::is_bark` is new. `BranchGrouping::parts` still lists only bark and
  leaves until F7, so the exporters don't treat fruit as bark.
- `FruitMaterial` (roughness 0.45, one-sided) and `FlowerMaterial` (roughness 0.8,
  two-sided) are new. Both take their colour from the vertex colours.
- Per-preset LOD budgets are deferred until after F5/F6 testing (user's call).

**F4 follow-up (flat items and stalks), done and tested:**
- At `ItemDetail::Minimal`, stalks are flat strips (2 triangles per segment) that
  face away from the trunk. Because of that, `FruitMaterial` is now two-sided.
- `LodDetail::flat_item_size` (0 = off; the default far level uses 0.75): when a
  kind's `ClusterPlacement::size` is below it, fruit become rough 6-triangle discs and
  flowers become single triangles. Flat items keep every item (they ignore
  `item_stride`) but still use `item_scale`. Catkins are unaffected.
- Each disc faces out from the trunk, leaning up to 0.6 rad in a varied direction,
  so none is seen edge-on from outside the tree.
- **Minimal triangle counts, before → after:**
  - grapes: 582 → 304, and all 30 grapes are kept instead of 15;
  - oceanspray: 720 → 260;
  - laburnum: 192 → 70.
- **Not done yet:** the Codex comment pass for this follow-up.

### F5 — Bevy plugin and viewer

**Status: done.**
- `cluster_mesh`, `fruit_material` and `flower_material` added; fruit and flowers spawn
  as `TreePartKind::Fruit` and `TreePartKind::Flowers` at every LOD level, using
  vertex colours, wind, and two-sided `StandardMaterial`s.
- The viewer steps maturity with `[` and `]`, and also accepts `--maturity <0–1>`.
- No petal translucency: `diffuse_transmission` was judged too expensive (user's call).
- `TreePreset::EZ_TREE` added and the ez-tree fixture tests use it. `TreePreset::Apple`
  is the first F6 preset, checked in the viewer at ripe and unripe maturity; its
  triangle count has not been checked against a budget.

- Add `TreePartKind::Fruit` and `TreePartKind::Flowers`; convert vertex colours;
  set up the materials and the wind shader path.
- Viewer:
  - `[` and `]` step maturity;
  - the new presets join the preset cycle.

### F6 — Presets

- Add apple, cherry blossom, magnolia, laburnum, dogwood, oceanspray, pussy willow
  and grape vine.
- For each, tune the settings in the viewer and check the triangle count.
- Add `TreePreset::EZ_TREE` and point the fixture tests at it.
- Some presets suit flowering better with few leaves or none; magnolia, for
  example, blooms before its leaves come out.

### F7 — Export, rig and jiggle

- **OBJ:** separate `fruit` and `flowers` objects, plus MTL materials. Vertex colours
  are written as the common `v x y z r g b` extension.
- **glTF:** new primitives with `COLOR_0`, and the new materials.
- **Rig:** `MeshSkin.fruit` and `MeshSkin.flowers`.
- **Jiggle:** add the new shape fields and counts, and extend its tests.

### F8 — Docs

- README sections for fruit and flowers, the crate docs, `port_plan.md` status, and
  the Codex comment pass.

---

## Open items

- **Pendulum swing:** if the `flutter` channel looks wrong on hanging fruit, add a
  fifth wind value. That would change the wind attribute and the shaders, so ask
  first.
- **Triangle budget:** set a target per preset. Proposed: under 2× the leaf triangles
  at LOD 0. Revisit after F5/F6 testing, including whether presets need their own
  LOD levels.
- **Not in this plan** (possible follow-ups):
  - falling petals or fruit;
  - seasonal blending between flower and fruit;
  - several fruit types on one tree.
