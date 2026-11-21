# ze_tree — billboard LOD plan

## Bottom line

**Goal: far trees keep their silhouette across LOD switches.**

At LOD3, branch ribbons, fruit and leaves become flat shapes tagged for
billboarding:

- **Ribbons and fruit become crossed pairs.** One shape in each pair is tagged
  `billboard`, the other `billboard_cull`.
- **Leaves stay single cards, tagged `billboard`, with no partner.** They are single
  cards today, so a renderer that ignores the tags still draws one card per leaf.

How each kind of renderer handles this:

- **Renderers that ignore the tags** draw everything as stored.
- **Renderers that read the tags** skip every `billboard_cull` shape and turn each
  `billboard` shape toward the camera. A turned shape never goes edge-on, so it
  covers its full area from every view.
- **`ze_tree_bevy` reads the tags.** It never builds the cull half, and its shaders
  do the turning in every render pass.

**What fixes the silhouette.** Measured against LOD0 (see *Silhouette
measurements*), today's outline holds through LOD2 and breaks at LOD3. The causes:

- leaves enlarged 1.8× outward from their base puff the outline out;
- the every-4th-leaf choice often throws away the outermost leaf on each branch.

The fix:

- **At every level, `leaf_stride` counts back from each branch tip**, so the
  outermost leaf always survives. This replaces today's selection outright, with
  no setting.
- **At every level, an enlarged card grows around the original leaf's centre**, not
  outward from its base. This also replaces today's behaviour.
- **At LOD3, store one card size and turn another.** A turned card shows its whole area, but
  a fixed card at a random angle shows only part of it. So the stored card keeps
  ×1.8 for renderers that ignore the tags, and the turning data describes a ×1.25
  card. Fruit already works this way: its stored disc and its turned shape differ.

| | LOD1 vs LOD0 | LOD2 vs LOD0 | LOD3 vs LOD0 | LOD2 → LOD3 switch |
|---|---|---|---|---|
| Today | 0.87 | 0.84 | 0.77 | 0.80 |
| After, tag-aware renderer | 0.90 | 0.86 | 0.86 | 0.86 |
| After, renderer that ignores the tags | 0.90 | 0.86 | 0.82 | 0.84 |

All scores are outline match (IoU, where 1.0 means identical). LOD1 → LOD2 stays at
0.90.

**ez-tree compatibility is dropped.** Leaf meshes at LOD1 and above no longer match
ez-tree's. LOD0 is unaffected: it keeps every leaf at its original size, so neither
change applies.

**LOD2 → LOD3 is still the weakest switch** (0.86, against 0.90 for LOD1 → LOD2),
and the first thing to check in the viewer. Turning LOD2's cards too is a follow-up
to try there, not part of this plan's defaults.

**Triangle cost.**

- **Leaves:** more than today at LOD1–LOD3, because picking per branch rounds up
  once per branch:
  - about 6% more cards on average, and 0% on presets whose branch leaf counts
    divide evenly;
  - at most 33% more, on Bush 3;
  - Apple goes from 990 to 1,080 cards at LOD1 and LOD2, and from 495 to 540 at
    LOD3.
- **Fruiting trees:** more than today, because every fruit is kept. Apple's LOD3
  goes from 1,962 triangles to about 3,400 stored, or about 2,250 drawn by a
  tag-aware renderer.
- **The existing under-half test** still passes: LOD3 stays under half of LOD2.

**Fruit.** Every fruit is kept at LOD3 as a pair of small hexagons, catkin items
included. The single-body catkin replacement is removed, and fruit stalks are dropped
at LOD3.

**How the data is split.** The tag is an object, not just `true`. It names the
billboard mode, the vertex attributes that hold the turning data, and the partner
part if there is one. The numbers themselves (a centre point per vertex, the axis
to turn around, and the offset from that point) have to stay in **vertex
attributes**. They are per-vertex data that a vertex shader reads directly, so they
cannot live in node JSON.

**Key architectural changes**

- **New per-vertex billboard data:**
  - `MeshBuffers` gains `billboard: Vec<BillboardVertex>`, empty except on billboard
    parts;
  - a `BillboardVertex` holds a `pivot` (the point the vertex turns around), an
    `axis` (a unit vector to turn around, or zero to face the camera fully) and an
    `offset` `[across, along]`;
  - one formula covers both modes:
    `position = pivot + across × right + along × up`, where:
    - for axis mode, `right = normalize(cross(axis, to_camera))` and `up = axis`;
    - for camera mode, `right = normalize(cross(tree_up, to_camera))` and
      `up = cross(to_camera, right)`;
  - stored positions are fallback geometry for renderers that ignore the tags. They
    need not match the turned shape's size.
- **New cull halves on `TreeMesh`:**
  - a new field `billboard_cull: BillboardCull` holding `branches`, `leaves` and
    `fruit` `MeshBuffers`;
  - these hold the second, perpendicular shape of each pair. `leaves` stays empty
    unless leaves use `Billboard::Double`;
  - `BranchSpan`, `LeafSpan` and `ClusterSpan` gain ranges into these buffers.
- **Ribbons are split, not changed:**
  - strip A goes in `branches`, with axis-mode billboard data around the branch
    centreline at each ring;
  - strip B goes in `billboard_cull.branches`.
- **Fruit gets a billboard mode:**
  - a new `LodDetail::billboard_fruit: bool`;
  - when set, every fruit becomes a camera-facing hexagon plus a perpendicular cull
    hexagon, and stalks are omitted.
- **Leaves get a billboard mode:**
  - a new `LodDetail::leaf_billboard: Option<BillboardMode>`, where `BillboardMode`
    is `Axis` or `Camera`;
  - a new `LodDetail::leaf_billboard_scale: f32` sets the turned card's size,
    separately from `leaf_scale`, which keeps setting the stored card's size;
  - `LodDetail::billboard` still chooses one or two cards. `Double` puts the second
    card in `billboard_cull.leaves`;
- **Leaf selection and sizing change at every level:**
  - `leaf_stride` counts back from each branch's last leaf;
  - `leaf_scale` grows each card around the original leaf's centre;
  - both replace today's behaviour, with no setting.
- **Catkin body removed:** `catkin_body` and its `ItemDetail::Minimal` special case
  are deleted, and catkins draw individual items at every level.
- **No stored normals on billboard geometry:**
  - billboard and cull parts have no normals or tangents;
  - the shader works out a normal. For leaves it tilts the normal outward from the
    trunk, so the canopy keeps some light and dark variety.
- **New `TreeMesh::billboard_cull_part(MeshPart)`** to extract the cull half of a
  part, next to `TreeMesh::part`.
- **glTF:**
  - billboard nodes get extras like:
    ```json
    {"billboard": {"mode": "axis", "pivot": "_BILLBOARD_PIVOT", "axis": "_BILLBOARD_AXIS", "partner": "branches_LOD3_billboard_cull"}}
    ```
  - `partner` is left out when there is no cull half;
  - cull nodes `<part>_LOD<n>_billboard_cull` get
    `{"billboard_cull": {"partner": "branches_LOD3"}}`.
- **Bevy:**
  - two new vertex attributes (shader locations 9 and 10);
  - a `BILLBOARD` shader def set by `TreeMaterial::specialize` when they are
    present;
  - a shared `tree_billboard.wesl` module;
  - a padded bounding box so the turning geometry is not culled at the edges.
- **New silhouette regression test** that scores stored and turned geometry against
  LOD0 and against the neighbouring level.

**Main trade-offs**

- **One vertex layout for both modes** means one shader path. It costs 8 floats per
  billboard vertex, which is small at LOD3.
- **Fruit faces the camera fully.** At 500+ units, fruit is a few pixels across and
  roughly round. Full facing gives the most coverage and never collapses to a line.
- **"Toward the camera" means toward its position**, with the turn taken from
  tree-up. That needs only one camera vector per pass. Orthographic views, including
  directional-light shadow maps, use the view's backward direction instead.
- **Leaf count stays about the same.** Where cards sit and how big they are matters
  more for the outline than how many there are. Keeping every 2nd leaf instead of
  every 4th scores only slightly better and doubles leaf triangles.

## Silhouette measurements

**Method.** A throwaway program, not checked in:

- projected each preset's mesh into a 96×96 mask from 8 directions slightly above
  the horizon;
- scored each level by intersection over union (IoU) against LOD0 and against the
  neighbouring level, recording missing and extra area separately;
- built turned and re-centred cards on the CPU from `skeleton.leaves`.

**Caveats.** Cards are treated as solid squares, ignoring image transparency, so
absolute numbers are approximate and the "extra" figures run high. Bark is drawn as
stored, with LOD3 ribbons crossed rather than turned. All figures are means over the
17 presets.

**Against LOD0**

| Level | IoU | Missing | Extra |
|---|---|---|---|
| LOD1 today (2 crossed cards per leaf, every 2nd, ×1.25) | 0.87 | 5% | 9% |
| LOD1 tip-picked | 0.87 | 4% | 10% |
| LOD1 centred | 0.89 | 5% | 6% |
| **LOD1 tip-picked and centred** | **0.90** | 4% | 7% |
| LOD2 today (1 fixed card, every 2nd, ×1.3) | 0.84 | 10% | 7% |
| LOD2 tip-picked | 0.85 | 9% | 8% |
| LOD2 centred | 0.85 | 11% | 5% |
| **LOD2 tip-picked and centred** | **0.86** | 9% | 5% |
| LOD3 today (1 fixed card, ×1.8 from the base, every 4th) | **0.77** | 9% | **19%** |
| LOD3 turned, tip-picked, ×1.2 | 0.86 | 9% | 6% |
| **LOD3 turned, tip-picked, ×1.25** | **0.86** | 8% | 7% |
| LOD3 turned, tip-picked, ×1.4 | 0.85 | 6% | 11% |
| LOD3 stored, centred, tip-picked, ×1.25 | 0.77 | **21%** | 3% |
| LOD3 stored, centred, tip-picked, ×1.5 | 0.81 | 14% | 7% |
| **LOD3 stored, centred, tip-picked, ×1.8** | **0.82** | 8% | 13% |
| LOD3 stored, from the base, tip-picked, ×1.8 | 0.77 | 8% | 21% |
| LOD2 turned, ×1.15, today's selection (follow-up idea) | 0.89 | 5% | 7% |

**Between neighbouring levels** (the first level is the reference)

| Switch | IoU | Missing | Extra |
|---|---|---|---|
| LOD1 → LOD2 today | 0.90 | 9% | 2% |
| LOD1 → LOD2, both tip-picked and centred | 0.90 | 8% | 1% |
| LOD2 → LOD3 today | 0.80 | 6% | 18% |
| Today's LOD2 → LOD3 turned ×1.25 | 0.82 | 9% | 11% |
| **New LOD2 → new LOD3 turned ×1.25 (the plan)** | **0.86** | 6% | 10% |
| New LOD2 → new LOD3 stored ×1.8 (tags ignored) | 0.84 | 5% | 14% |

**What the numbers say**

- **The stored card needs its own size.** Shrinking it to ×1.25 to match the turned
  card loses a fifth of the outline for renderers that ignore the tags. Keeping it
  at ×1.8, centred, beats today's LOD3 by 0.05.
- **Centring matters on its own.** ×1.8 grown from the base scores 0.77; centred, it
  scores 0.82.
- **Tip-picking and centring help at every level**, not just LOD3: LOD1 goes from
  0.87 to 0.90 and LOD2 from 0.84 to 0.86. The cost is at most one extra leaf per
  branch, about 6% more cards on average.
- **Today's every-4th selection drops branch tips.** In 6 of the 17 presets (Aspen
  Small and Medium, Bush 2, Pine Medium and Large, Trellis), the outermost leaf on
  every branch is never kept, because each branch's leaf count is a multiple of 4.
- **Turned scale between ×1.2 and ×1.4 scores about the same.** ×1.25 is a starting
  point, to be tuned in the viewer since the mask ignores transparency.
- **Bark alone at LOD3 matches only 0.41 of LOD0's bark outline**, because thin
  branches are dropped below radius 0.15. Leaves hide this on leafy trees. Bare or
  sparse trees would lose outline at LOD3 regardless of this plan; that is a
  possible follow-up, not part of this plan.

**Triangle budget.** The existing test requires LOD3 to have under half of LOD2's
triangles. Apple is the tightest preset:

| | Triangles |
|---|---|
| LOD2 total (1,452 bark + 2,160 leaves from 1,080 tip-picked cards + 5,440 fruit) | 9,052 |
| LOD3 limit (under half) | 4,526 |
| LOD3 bark, both strips | 156 |
| LOD3 leaves, 540 tip-picked single cards | 1,080 |
| LOD3 fruit: 272 fruit × 2 hexagons × 4 triangles | 2,176 |
| **LOD3 total, both halves counted** | **3,412** |
| LOD3 drawn by a tag-aware renderer (one strip, one hexagon) | about 2,250 |

## Steps

### 1. Core types (`crates/ze_tree/src/mesh.rs`, `options.rs`)

- Add `BillboardVertex { pivot: [f32; 3], axis: [f32; 3], offset: [f32; 2] }`. Its doc
  gives the formula, the meaning of a zero axis, that tree-up is +Y, and that stored
  positions are fallback geometry that need not match the turned shape.
- Add `BillboardMode { Axis, Camera }`.
- Add `MeshBuffers::billboard`. Update the `MeshBuffers` docs: `billboard` is either
  empty or one entry per position, and when it is populated, `normals` and
  `tangents` are empty.
- Add `BillboardCull { branches, leaves, fruit }` and `TreeMesh::billboard_cull`.
- `TreeMesh::vertex_count` and `triangle_count` count everything, cull half included
  (*settled*). `BillboardCull` gets its own counts, so a tag-aware
  renderer's cost is the total minus the cull half.
- Add three `LodDetail` fields:
  - `billboard_fruit: bool` (JSON `billboardFruit`, default off);
  - `leaf_billboard: Option<BillboardMode>` (JSON `leafBillboard`, default off);
  - `leaf_billboard_scale: f32` (JSON `leafBillboardScale`, default 1.0).
- Update the `leaf_stride` and `leaf_scale` docs: the stride counts back from each
  branch's last leaf, and scaling grows each card around the leaf's centre. Remove
  "JSON uses ez-tree's `LODDetail` field names" from the `LodDetail` doc only if a
  field is renamed; this plan renames none.

### 2. Ribbons (`mesh.rs`: `push_ribbon_ring` and the ring-connecting code)

- Write strip A's two edge vertices to `branches`. Each gets:
  - `pivot` = the section centre;
  - `axis` = the section's up direction (the branch tangent);
  - `offset` = `[±radius, 0]`.
- Write strip B's two edges to `billboard_cull.branches`.
- Connect rings with 2 triangles per buffer instead of 4 in one.
- Stop writing normals and tangents for ribbons.
- `BranchSpan`:
  - `ring_vertices()` returns 2 for ribbons;
  - add `cull_vertices` and `cull_indices` ranges, which are empty for tubes;
  - update the ring layout doc.

### 3. Fruit (`cluster_mesh/mod.rs`, `cluster_mesh/fruit.rs`)

- **Remove the catkin body:**
  - delete `catkin_body` and the two `catkin && level == ItemDetail::Minimal`
    branches;
  - fruit catkins go through `fruit::mesh_item` without stems;
  - flower catkins go through `flower::mesh_catkin_item` at the requested level;
  - update the `ItemDetail::Minimal`, `item_stride` and `item_scale` docs;
  - remove `CATKIN_BODY_SIDES` and `CATKIN_BODY_RINGS`.
- **Billboard fruit:** in `mesh_fruit`, when `detail.billboard_fruit` is set, skip
  stalks and emit `fruit::mesh_billboard` for every retained item, catkins included.
- `mesh_billboard` writes two hexagons of 6 vertices and 4 triangles each, with no
  centre vertex and no rim shading:
  - **Billboard hexagon**, into `fruit`:
    - stored positions follow today's disc placement (outward facing, with the
      per-item lean, stretched along the fruit axis);
    - billboard data describes a regular hexagon: `pivot` = the body centre,
      `axis` = zero, and a diameter equal to the average of the body's width and
      height.
  - **Cull hexagon**, into `billboard_cull.fruit`: the same shape turned 90° around
    the fruit's own axis.
- `ClusterSpan` gains the matching cull ranges.
- When `billboard_fruit` is set, `flat_item_size` and `item_detail` no longer apply
  to fruit bodies. Document this on the field.

### 4. Leaves (`mesh.rs`: leaf selection and `mesh_leaf`)

- **Selection, every level:** replace the `step_by(leaf_stride)` over the whole leaf
  list. Keep leaf `i` when `(last leaf index on its branch − i) % leaf_stride == 0`.
  Leaves are stored base to tip within each branch, so this always keeps the tip
  leaf. This relies on each branch's leaves being contiguous in `Skeleton::leaves`;
  document that ordering on `Skeleton::leaves` and cover it with a test.
- **Centring, every level:** cards grow around the original leaf centre
  (`origin + tip × s/2`, where `s` is the leaf's original size), not from the base.
  With `leaf_scale` 1.0, as at LOD0, the card is unchanged.
- **Billboard cards:** when `leaf_billboard` is set:
  - the first card goes to `leaves`, with stored size `leaf_scale` and billboard
    data sized by `leaf_billboard_scale` (`k` below);
  - with `Billboard::Double`, the second stored card goes to
    `billboard_cull.leaves`;
  - both drop normals and tangents, and `rounded_normals` has no effect.
- **Billboard data per mode:**
  - **`Camera`:** `pivot` = the original leaf centre, `axis` = zero, offsets
    `[±k·s/2, ±k·s/2]`.
  - **`Axis`:** `pivot` = the leaf origin, `axis` = the stem-to-tip direction,
    offsets `[±k·s/2, s/2 ± k·s/2]`.
- `LeafSpan` gains cull ranges, which are empty for `Single`.

### 5. Default levels (`default_lod_levels`)

- **LOD3:**
  - `billboard_fruit: true`;
  - `cluster_stride: 1`, `item_stride: 1`, `item_scale: 1.0`;
  - `leaf_billboard: Some(Camera)`, to match the clump image;
  - `billboard: Some(Single)`, `leaf_stride: 4` and `leaf_scale: 1.8`, all as today;
  - `leaf_billboard_scale: 1.25`.
- **LOD0–LOD2:** settings unchanged. Their leaf
  meshes still change through the new selection and centring.
- Flowers are unchanged, apart from catkins now drawing individual items. Keep
  `min_item_size: 0.5` at LOD3.
- Update the function doc.

### 6. Parts and rig (`part.rs`, `rig.rs`)

- `PartMesh` extraction copies `billboard`, and accepts buffers with empty
  `normals`.
- Add `TreeMesh::billboard_cull_part(part)`:
  - bark parts select ranges from `billboard_cull.branches`;
  - `MeshPart::Leaves` selects from `billboard_cull.leaves`;
  - `MeshPart::Fruit` selects from `billboard_cull.fruit`;
  - flowers return an empty mesh.
- `MeshSkin` gains `branches_billboard_cull` and `leaves_billboard_cull`. Fruit
  remains unskinned.
- **Documented limitation:** importers skin `POSITION` but not `_BILLBOARD_PIVOT`.

### 7. glTF (`export/gltf.rs`)

- For each part whose buffer has billboard data:
  - write `_BILLBOARD_PIVOT` as VEC4 `(pivot, across)` and `_BILLBOARD_AXIS` as
    VEC4 `(axis, along)`;
  - omit `NORMAL` and `TANGENT`;
  - put the `billboard` object in the **node's** extras. Node extras are what
    Blender, Godot and three.js expose; primitive extras are often dropped on import.
- Write each non-empty cull half as a sibling node named
  `<part>_LOD<n>_billboard_cull`, with the `billboard_cull` object. It uses the
  same material and the same wind and skin attributes as its partner.
- Document the extras schema, the attribute layout, the formula, and that stored
  positions are fallback geometry, in the `to_gltf` doc.

### 8. OBJ (`export/obj.rs`)

OBJ has no metadata. Write each non-empty cull half as its own group, named
`<part>_billboard_cull`. Faces in billboard and cull groups are written as
`f v/vt`, without normals. Billboard data is not written.

### 9. Bevy (`crates/ze_tree_bevy`)

- **`mesh.rs`:**
  - add `ATTRIBUTE_BILLBOARD_PIVOT` and `ATTRIBUTE_BILLBOARD_AXIS` (Float32x4);
  - `bark_mesh`, `leaf_mesh` and `cluster_mesh` insert them when present;
  - `base_mesh` inserts normals only when the buffer has them.
- **`material.rs`:** `specialize` adds locations 9 and 10 and the `BILLBOARD` shader
  def when the mesh layout contains the attributes.
  - The shader def comes from the mesh layout, so billboard and non-billboard
    meshes share one `TreeMaterial`; Bevy builds a separate pipeline per mesh
    layout.
  - `BarkKey` needs no billboard field.
  - **`LeafKey` gains a `billboard` flag**, and `leaf_material` leaves out the
    normal map when it is set. Billboard leaves have no tangents, and
    `leaf_billboard` is a public per-level field, so an app pairing it with
    normal-mapped single-leaf images must still work.
  - The default LOD3 clump image has no normal map, so its material doesn't change.
- **New `shaders/tree_billboard.wesl`:**
  - takes a local-space camera position (or, for orthographic views, a direction) and
    the two attributes, and returns the local position and a normal;
  - for ribbons and fruit, the normal is the flat face normal turned toward the
    camera;
  - for leaves, the normal blends the toward-camera direction with
    `normalize(vec3(pivot.x, 0, pivot.z))`, the horizontal direction from the trunk,
    set by a tunable constant. A `BILLBOARD_LEAF` shader def tells leaves apart
    without changing the material key.
- **`tree.wesl` and `tree_prepass.wesl`:**
  - under `BILLBOARD`, compute the turned position first, then add
    `tree_wind_offset` to it;
  - the motion-vector branch uses the same turned position with the previous
    frame's transform.
- **Which camera each pass uses:** every pass turns toward its own view, so shadow
  passes face the light. This needs no per-camera uniform. Bevy 0.20.1's default
  directional cascades end at 150 units, so LOD3 casts no directional shadows unless
  an app extends them.
- **`spawn.rs`:**
  - `generate` never converts `billboard_cull`;
  - `spawn_tree_parts` inserts an `Aabb` for billboard parts, padded by the largest
    billboard offset.
- **Docs:** the crate and README docs explain that LOD3 bark, leaves and fruit turn
  to face the camera.

### 10. Tests

New tests:

- **Silhouette regression** (`tests/silhouette.rs`): port the measurement scorer
  (masks, 8 directions, IoU). It scores, per preset and as a mean:
  - **stored geometry** of each default level against LOD0, which is what renderers
    that ignore the tags see;
  - **turned geometry**, built from the stored `billboard` data through the
    formula, against LOD0;
  - **each neighbouring pair** of levels, LOD1 → LOD2 and LOD2 → LOD3.

  Thresholds come from the measured values with a small margin, e.g. LOD3 turned
  ≥ 0.84 against LOD0, and LOD2 → LOD3 turned ≥ 0.84. This guards the main goal: a later
  tuning change that loses outline fails the test.
- **Ribbons:** strip A and strip B are perpendicular and share pivots. The formula
  reproduces strip A's positions when `to_camera` is strip A's own normal.
- **Fruit:** hexagons cross at 90°, camera mode has a zero axis, stalks are absent,
  and every fruit (catkin items included) is kept at LOD3.
- **Leaves:**
  - with `Single`, the cull buffer is empty; with `Double`, the cards cross at 90°;
  - the stored card uses `leaf_scale` and the billboard offsets use
    `leaf_billboard_scale`, both centred on the original leaf;
  - for any stride, every branch's last leaf is kept;
  - each branch's leaves are contiguous in `Skeleton::leaves`, base to tip;
  - a scaled card keeps its centre at the original leaf's centre;
  - with `leaf_scale` 1.0, the card matches the unscaled leaf.
- **Catkins:** at `ItemDetail::Minimal` without billboards, a catkin draws its
  individual items, and `item_stride` reduces them.
- **glTF:** extras parse, and every `partner` resolves to an existing node. Leaf
  billboard nodes have no `partner`. `NORMAL` is absent on billboard and cull nodes.
- **Bevy:** billboard meshes carry both attributes, and no entity is spawned for a
  cull half. A billboard leaf level that uses single-leaf images gets a material
  without a normal map.

Existing tests whose intent changes. Each is rewritten to the new layout without
weakening what it checks:

- `mesh.rs` `ribbons_are_two_crossed_strips`: still checks two crossed strips, now
  across `branches` and `billboard_cull.branches`.
- The `BranchSpan` ring-layout assertions (4 → 2 + 2).
- Tests that loop over `default_lod_levels()` and assert one normal and one tangent
  per position ([tests/mesh.rs](crates/ze_tree/tests/mesh.rs) around lines 269, 344,
  398). LOD3 bark and leaves now expect empty normals and tangents.
- `farthest_level_culls_thin_branches_and_keeps_leaves`: the leaf-count check
  changes from `div_ceil` over the whole list to the per-branch count. The
  under-half triangle check is **unchanged** and should pass (see the budget).
- `tests/export_gltf.rs` LOD3 material check (around line 286). It must also accept
  the `_billboard_cull` nodes.
- `tests/export_obj.rs`: `vn` lines equal the number of vertices that have normals.
- `tests/rig.rs` loop over `lod_meshes`: the skin covers the cull buffers.
- `cluster_mesh` tests that pin LOD3 fruit counts, strides or flat discs.
- `spawn.rs` `ribbon_bark_is_drawn_two_sided`: unchanged.

Changed because ez-tree compatibility is dropped (the requirement itself is
withdrawn, so these are retired rather than weakened):

- `tests/mesh.rs` `matches_ez_tree_meshes`:
  - keep comparing branches at LOD0–LOD2, and leaves at LOD0, against the ez-tree
    fixtures, since those meshes don't change;
  - stop comparing leaves at LOD1 and LOD2.
  - Rather than special-casing, it may be cleaner to keep only LOD0 there and rely
    on the new leaf tests for the rest. Decide when implementing.
- `tests/mesh.rs` `default_lod_levels_match_ez_tree`: it still passes, because the
  LOD0–LOD2 values are unchanged. But it now pins a promise we no longer make.
  Replace it with a plain test of the documented default values.

Removed with the feature (the behaviour no longer exists, so the test is deleted
rather than weakened):

- `tests/cluster_mesh.rs` `minimal_catkins_are_one_body_whatever_the_item_stride`.
  The new catkin test above replaces it.

### 11. Verify visually

In the Bevy viewer, orbit trees across the 250 → 500 switch, with both the tag-aware
path and a stored-geometry view. Check:

- the outline holds, especially on Aspen, Bush 2 and Pine;
- branches don't visibly bend, and turned strips stay connected at joints;
- the lighting doesn't pop;
- shadows look right with the cascade range extended.

Fine-tune `leaf_billboard_scale` (measured best between 1.2 and 1.4) and
`leaf_scale` here, since the mask ignores leaf image transparency.

## Things to verify before or while implementing

- **Missing normals in Bevy.** Whether Bevy 0.20's forward and prepass fragment code
  works without `Mesh::ATTRIBUTE_NORMAL` (`VERTEX_NORMALS` off) when our vertex
  shader supplies the normal. If it doesn't, I'll ask rather than insert a dummy
  attribute.
- **Camera into tree space.** Whether `mesh_functions` exposes `local_from_world` in
  0.20. The alternative is to do the billboard math in world space and transform
  the wind offset.
- **Bounds.** Whether a manually inserted `Aabb` survives Bevy's automatic bounds
  calculation in 0.20.

## Decisions

**Settled**

- Fruit faces the camera fully.
- Tags are named `billboard` / `billboard_cull`.
- The catkin body is removed; catkins become per-item billboards at LOD3.
- Billboard geometry drops stored normals; the shader works them out.
- Fruit stalks are dropped at the billboard level.
- The silhouette is the main goal.
- At every level, leaves are picked counting back from each branch tip, and enlarged
  cards are centred on the original leaf. This is standard behaviour, with no
  setting.
- ez-tree compatibility is dropped wherever it conflicts with a better result.
- LOD3 keeps every 4th leaf: stored at ×1.8, turned at ×1.25.

- Leaf billboards stay at LOD3 only (recommended by both the advisor and me):
  - with the new selection and centring, LOD2 → LOD3 already measures 0.86, up from
    0.80;
  - turning LOD2 would move the no-normals lighting change to 250 units, where trees
    are twice as large on screen, and the outline mask can't measure that cost;
  - `leaf_billboard` is per level, so turning LOD2 later is a one-line default
    change.
- `TreeMesh::vertex_count` and `triangle_count` count everything, cull half
  included (recommended by both the advisor and me). They stay true totals, match
  what exporters write, and keep the OBJ export test meaningful.

**Follow-ups, not part of this plan**

- **Turning LOD2's leaf cards.** Try it in the viewer, with `Axis` mode for LOD2's
  single-leaf images. The only measurement so far (0.89 against LOD0) used camera
  mode, today's selection and base-anchored cards, so it needs re-measuring on the
  new baseline.
- **Bark outline at LOD3 for bare or sparse trees.** Bark alone matches 0.41 of
  LOD0's bark outline, because thin branches are dropped.
