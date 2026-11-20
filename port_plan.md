# ze_tree — port plan for ez-tree

A Rust port of [ez-tree](https://github.com/dgreenheck/ez-tree), a procedural tree
generator. This crate is the **core**. It covers options, presets, the seeded RNG,
skeleton growth, meshing and LOD meshing, and it has no engine dependency. A
`ze_tree_bevy` crate comes later for materials, the wind shader, textures, LOD
switching and trellis meshes.

Reference source: `prior_art/ez-tree`, a shallow clone of commit
`dcf309bd86bd521083d9c70f01f2de45fdc7c457` (v2.0.0, 2026-07-16). The library lives in
`src/lib/` (about 1,660 lines; nearly all the logic is in `tree.js`). The demo app in
`src/app/` is out of scope.

---

## Status (2026-11-19)

| Phase | State |
|---|---|
| 0 — Scaffolding | ✅ Done |
| 1 — Foundations (golden harness, `rng`, `options` + `presets`) | ✅ Done |
| 2 — Skeleton growth | ✅ Done (uncommitted) |
| 3 — Meshing and LOD | ⏭ Next |
| 4 — Tree wrapper, trellis layout, material params, WESL wind | Not started |
| 5 — Exports (glTF, OBJ, rig) | Not started |
| 6 — Polish | Not started |

Where things stand:
- `cargo build`, `cargo clippy --all-targets`, `cargo test` (14 unit, 12 options,
  1 RNG, 5 skeleton tests), `cargo doc --all-features` and the doctests all pass with
  no warnings. Phases 0–1 are committed; Phase 2 is not.
- All fixtures from the verification section are generated (7.8 MB). RNG, options and
  all 24 skeleton fixtures are used by tests; the 9 mesh fixtures are for Phase 3.
- `skeleton` and the trellis pull (`TrellisOptions::nearest_point` / `pull`) are
  implemented. `mesh`, `tree` and `material` are still empty modules.

Open items carried forward:
- **D9:** fractional seeds act like their truncated value. Accepted as documented;
  revisit only if someone needs JS-identical fractional seeds.
- **f32 options vs f64 preset literals:** preset files contain values such as
  `0.6199999999999999` that f32 stores as `0.62`. The JSON-format test compares
  fractional values at f32 precision for this reason (D4).
- **Thin evergreen branches are chaotic (Phase 2 finding):** see "Phase 2 tolerance"
  below. One fixture (`evergreen_seed_42`) has a documented, larger tolerance. Doing
  the whole growth in f64 might remove this; not done (see Decision 7).
- **`cargo fmt`:** the Phase 0–1 files (`options.rs`, `tests/options.rs`,
  `tests/rng.rs`) are not rustfmt-clean. The Phase 2 files are. Run `cargo fmt` once
  as its own commit.

---

## Bottom line up front

1. **Faithful means algorithmically equivalent, not bit-identical.** Same algorithm,
   same RNG, same draw order. For a given seed and options, the tree has the same
   structure as in JS: the same branch and leaf counts and the same placement slots.
   Vertex positions and normals match within float tolerance. Float jitter is fine.
2. **Idiomatic Rust on `glam`, in f32.** Orientations are stored as `Quat`. Euler
   angles appear only where the algorithm itself works on them: gnarliness adds random
   offsets to the Euler x and z angles. No three.js math port, no JS-rounding helpers,
   and no copying of JS operation order. glam also has f64 types (`DVec3`, `DQuat`),
   but f32 matches bevy and the output buffers. Switch to f64 only if a Phase 2 test
   shows f32 drift beyond ε on the largest presets.
3. **The RNG is ported exactly**, including JS int32 semantics. It is integer-only, so
   exact matching is cheap, and it is what ties a seed to a tree shape.
4. **Two-pass architecture, as in JS v2.0.** `Skeleton::generate(&TreeOptions)`
   consumes all the randomness. `Skeleton::mesh(&options, &LodDetail)` is pure and can
   be called any number of times.
5. **Engine-specific parts stay out of core.** Materials, textures, `THREE.LOD`
   and `CylinderGeometry` are not ported here. Core provides only the *data* they need:
   material parameters, wind constants, LOD level descriptors and trellis line
   placements. The one exception is the **wind shader, ported from GLSL to a portable
   WESL module** shipped by core, so any wgpu or bevy renderer can import it.
6. **JS behaviour that decides what a seed produces is kept.** That includes the
   effect of the `'Deciduous'` typo: the code says plainly what it does, and a TODO
   records what upstream intended (K1). Quirks that don't affect shape are
   dropped. See "JS behaviour: kept vs dropped".
7. **Verification uses JS-generated fixtures compared at tolerance.** A small Node
   harness runs the real ez-tree and dumps its output. Rust tests compare structure
   exactly and floats within ε.
8. **The crate becomes a Cargo workspace now**, so the bevy crate can be added later
   (Decision 1).
9. **Exports are optional features.** Each file format sits behind its own cargo
   feature. Exports keep trunk, branches and leaves as separate named parts, and
   formats that support skinning (glTF) carry a rig built from the skeleton. Core
   records the branch hierarchy and per-branch mesh ranges so exporters need no
   re-meshing.

---

## Target public API (sketch)

```rust
// options.rs — serde-compatible with ez-tree preset/export JSON   (implemented)
pub struct TreeOptions { seed: i32 /*ToInt32 on JSON load*/, tree_type: TreeType, bark: BarkOptions,
                         branch: BranchOptions, leaves: LeafOptions, trellis: TrellisOptions }
pub enum TreeType { Deciduous, Evergreen }       // "deciduous" | "evergreen"
pub enum Billboard { Single, Double }            // "single" | "double"
pub struct PerLevel<T>([T; 4]);                  // JSON: {"0":..,"1":..,"2":..,"3":..}

pub const MAX_LEVELS: u32 = 3;
pub enum OptionsError { TooManyLevels { levels: u32 } }

// presets.rs — the 16 JSON presets, embedded with include_str!   (implemented)
pub enum TreePreset { AshSmall, ..., PineLarge, Trellis }
impl TreePreset { pub const ALL: [TreePreset; 16]; pub fn name(self) -> &'static str;
                  pub fn from_name(&str) -> Option<Self>; pub fn json(self) -> &'static str;
                  pub fn options(self) -> TreeOptions; }
impl TreeOptions { pub fn from_json(&str) -> Result<Self, serde_json::Error>;  // merges onto defaults, like JS copy()
                   pub fn to_json(&self) -> String;
                   pub fn validate(&self) -> Result<(), OptionsError>; }

// rng.rs   (implemented)
pub struct Rng;  impl Rng { pub fn new(seed: i32); pub fn next_u32(&mut self);
                            pub fn next_f64(&mut self) /*[0,1), exact JS value*/;
                            pub fn next_f32(&mut self) /*[0,1)*/;
                            pub fn range(&mut self, min: f32, max: f32) -> f32; }
// JS rng.random(max, min) == range(min, max). Note the swapped argument order.
// Use next_f64 where the result is floored into an index (Fisher–Yates), so f32
// rounding cannot produce k+1.

// skeleton.rs   (implemented)
pub struct Skeleton { pub branches: Vec<SkeletonBranch>, pub leaves: Vec<SkeletonLeaf> }
pub struct SkeletonBranch { pub level: u32, pub parent: Option<Attachment>,
                            pub sections: Vec<Section>, pub segment_count: u32, pub base_radius: f32 }
pub struct Section { pub origin: Vec3, pub orientation: Quat, pub radius: f32 }
pub struct SkeletonLeaf { pub parent: Attachment, pub origin: Vec3, pub orientation: Quat, pub size: f32 }
pub struct Attachment { pub branch: usize, pub section: usize, pub t: f32 }  // t: 0..1 toward section+1
impl Skeleton { pub fn generate(opts: &TreeOptions) -> Result<Skeleton, OptionsError>; }
pub enum OptionsError { TooManyLevels { levels }, ZeroSections { level } }

// trellis.rs   (pull implemented; layout is Phase 4)
impl TrellisOptions { pub fn nearest_point(&self, Vec3) -> Vec3;
                      pub fn pull(&self, position: Vec3, radius: f32) -> Option<TrellisPull>; }
pub struct TrellisPull { pub direction: Vec3, pub strength: f32 }

// mesh.rs
pub struct LodDetail { pub section_stride: u32, pub segment_factor: f32,
                       pub leaf_stride: u32, pub leaf_scale: f32, pub billboard: Option<Billboard> }
pub struct LodLevel { pub distance: f32, pub hysteresis: f32, pub detail: LodDetail }
pub fn default_lod_levels() -> [LodLevel; 3];
pub struct MeshBuffers { pub positions: Vec<[f32;3]>, pub normals: Vec<[f32;3]>,
                         pub uvs: Vec<[f32;2]>, pub indices: Vec<u32> }
pub struct TreeMesh { pub branches: MeshBuffers, pub leaves: MeshBuffers }
impl Skeleton { pub fn mesh(&self, opts: &TreeOptions, detail: &LodDetail) -> TreeMesh; }

// tree.rs — a convenience wrapper that mirrors the JS `Tree` class
pub struct Tree { options, skeleton, mesh }
impl Tree { new, from_preset, generate, create_geometry(&LodDetail), generate_lods(&[LodLevel]),
            vertex_count, triangle_count }

// trellis.rs (Phase 4)
pub fn trellis_layout(&TrellisOptions) -> TrellisLayout  // line positions, lengths, radius, colour

// material.rs — engine-agnostic material and wind parameters
pub struct BarkMaterialParams { tint, flat_shading, textured, uv_repeat_y }
pub struct LeafMaterialParams { tint, alpha_test, rounded_normals, double_sided }
pub struct WindParams { strength: Vec3 = (0.5,0,0.5), frequency: 0.5, scale: 70 }
```

These names are a starting point and get settled while implementing.

---

## JS behaviour: kept vs dropped

Every kept quirk gets a short code comment explaining that it matches ez-tree, plus a
test.

**Kept, because they decide what a seed or preset produces:**

| # | JS behaviour | Where | Port |
|---|---|---|---|
| K1 | `this.options.type === 'Deciduous'` (capital D) is never true, so JS never divides deciduous section length by `levels - 1`. Upstream meant to divide. | `#growBranch` | **Fix the typo, keep the behaviour.** Don't port a dead comparison. Section length is `branch.length / section_count`, with a `TODO` above it. The TODO says that upstream intended to divide deciduous section length by `levels - 1`; that turning it on needs (a) a rule for `levels ≤ 1`, where the divisor is 0 or negative, such as `max(1, levels - 1)`, and (b) retuning the deciduous presets, which would grow much shorter. Fixtures come from unmodified JS and match. |
| K2 | The RNG uses JS int32 semantics: `&` gives signed i32, `>>` is an arithmetic shift, `<<16` wraps, `>>> 0` gives u32. | `rng.js` | Exact port using `i32` wrapping ops. The f64 result `/ 2^32` is then cast to f32, and the cast must not round up to 1.0 (clamp, or compute the division in f64). |
| K3 | Branches grow breadth-first from a FIFO queue, and that order decides RNG use. | `#generateSkeleton` | `VecDeque`. |
| K4 | `radialOffset` is drawn even when `count == 0`, and the shuffle draws `count-1` values. | `generateChildBranches`, `generateLeaves`, `shuffledIndices` | Same draws in the same order. |
| K5 | RNG draw order inside a section: gnarliness x, then z. Per child or leaf: start jitter, then radial jitter. Leaf size is drawn at record time. | throughout | Same order, with a comment wherever the order isn't obvious. |
| K6 | Gnarliness adds random offsets to the **Euler XYZ** x and z angles of the section orientation. | `#growBranch` | Decompose with a private `euler_xyz` that copies three's `setFromRotationMatrix` XYZ case (`asin` middle angle, 0.9999999 gimbal threshold, z = 0 at lock), offset x and z, then `Quat::from_euler(EulerRot::XYZ)`. glam's `to_euler` uses a different method that splits x/z differently at gimbal lock, and that split changes the result of the offsets. Tested: glam's `from_euler(XYZ)` equals three's `qx·qy·qz`. |
| K7 | The evergreen tip has radius 0, so gnarliness `1/sqrt(0)` is ∞ and the force step `strength/0` is ±∞ (then clamped). | `#growBranch` | The orientation computed after the last section is never used. Skip that work explicitly instead of producing NaN or ∞. The result is the same, and the code is clearer. |
| K8 | Child and leaf placement uses `floor(t * (n-1))` section lookup with lerp/slerp between sections. Note that `qB.slerp(qA, alpha)` slerps *from B toward A* by alpha, while position lerps A→B: at alpha 0 the position is A's but the orientation is B's. | placement | Keep exactly, including the B→A slerp direction. This is an algorithmic detail, not jitter. |
| K9 | The trellis snaps to the nearest multiple of `spacing` from the grid's min corner, then clamps to the grid edge. When `width` or `height` is not a multiple of `spacing`, a point near the far edge can be pulled toward the edge itself, where there is no wire. | `getNearestTrellisPoint` | Kept; it decides the trellis preset's shape. |
| K10 | `rotateTowards` returns unchanged only when the angle is exactly 0, and `setFromUnitVectors(+Y, −Y)` is a half turn about +Z. | growth force, trellis | Private `rotate_towards` and `rotation_from_y` follow three. glam's `rotate_towards` snaps to the target within 1e-4 rad and clamps negative steps, and `from_rotation_arc` picks another axis at 180°. |

**Dropped or changed:**

| # | JS behaviour | Port |
|---|---|---|
| D1 | Indices go into a `Uint16Array`, so large trees silently wrap. | `u32` indices. |
| D2 | A `windFactor` buffer is declared and never filled. | Omitted. |
| D3 | Per-level maps indexed past level 3 give `undefined`, which becomes NaN. | `levels > 3` returns an error. |
| D4 | Euler round-trips after every rotation, f64 math, `Math.round` half-up. | Quaternions, f32, `f32::round`. Inputs are positive, so the rounding agrees. |
| D5 | `options.copy` silently ignores unknown keys. | Same behaviour in `TreeOptions::from_json` (merge onto defaults, no `deny_unknown_fields`). Listed here only to record that it was a deliberate choice. |
| D6 | The trellis nearest-point code is duplicated in `tree.js` and `trellis.js`. | One function. |
| D7 | `bark.maps` and `leaves.map` hold THREE textures. | Omitted from `TreeOptions`; texture choice belongs to the engine crate. `bark.type` / `leaves.type` are kept as `kind` strings. |
| D8 | Per-level maps only have the keys that are used: `angle` and `start` have `1..3`, `children` has `0..2`. | `PerLevel<T>([T; 4])` for every field; unused slots default to 0. JSON output therefore has all four keys; ez-tree ignores the extra ones on load. |
| D10 | A child or leaf `start` outside 0..1 indexes past the section array and crashes with `undefined`. | The section index is clamped to the array, so the point extrapolates below the base or stays at the tip. `validate()` does not check `start`. |
| D11 | `sections == 0` divides by zero. | `validate()` rejects it for levels 0..=`levels` (`OptionsError::ZeroSections`). |
| D9 | A fractional seed in JS is added to the RNG constants *before* the int32 wrap, so `seed: 0.5` and `seed: 0` give different streams. | `seed` is `ToInt32`-converted once on load (Decision 3), so fractional seeds behave like their truncated value. Integer seeds, which is all the JS app produces, match exactly. |

---

## Verification: JS fixtures at tolerance

`tools/golden/` is a small Node project, not part of the Rust build.

- `package.json` pins `three@0.167.1` (matching `prior_art`'s lockfile) and `esbuild`.
  ez-tree uses import paths without extensions and imports JSON, so esbuild bundles
  it unmodified. The files in `prior_art` are never edited.
- `gen.mjs` bundles the prior_art sources with esbuild (`three` external) and writes
  these to `crates/ze_tree/tests/fixtures/` (about 8 MB in total). Run it with
  `npm install && npm run gen` in `tools/golden`.
  1. **`rng.json`**: the raw u32 behind 256 draws for 12 seeds, including negative
     seeds and seeds above 2³¹. Compared **exactly** via `Rng::next_u32`.
  2. **`skeletons/<case>.json`**: all 16 presets, `default`, `deciduous_seed_{1,-7}`,
     `evergreen_seed_{1,42}`, `deciduous_levels_{0,1}` and `evergreen_levels_2`. Each
     holds the merged options plus branches (`segmentCount`, `baseRadius`, sections as
     `[ox,oy,oz,qx,qy,qz,qw,radius]`) and leaves as `[ox,oy,oz,qx,qy,qz,qw,size]`.
     Quaternions come from three's `setFromEuler`. Floats keep 9 significant digits.
  3. **`meshes/<case>__lod{0,1,2}.json`**: raw buffers (before the Float32/Uint16
     conversion) for `aspen_small`, `bush_2` and `evergreen_levels_2` at each
     `Tree.defaultLODLevels` detail.
  4. **`options/default.json`** and **`options/presets_merged.json`** (each preset
     after `TreeOptions.copy`), plus **`lod_levels.json`**.
- Rust tests:
  - **Structure is exact:** branch count, sections per branch, leaf count, segment
    counts, and the mesh's vertex, index and triangle counts.
  - **Floats are within ε:** compare positions relative to tree size, and compare
    orientations by quaternion angle (allowing for q ≡ −q). Report the worst
    deviation, and on failure report where it happened. ε is set once from evidence
    in Phase 2, documented, and never loosened just to make a test pass.
  - **Meshing is tested on its own:** load a JS skeleton fixture into `Skeleton`, mesh
    it in Rust, and compare against the JS mesh. This keeps growth error separate from
    meshing error.
- Known points where float jitter can change the outcome, since these are the only
  ways it can change structure:
  - The trellis `distance > maxDistance` cutoff, which only matters for the Trellis
    preset.
  - The growth force's `sinFull > 1e-6` cutoff.
  - The Euler decomposition near gimbal lock (|y| ≈ 90°).

  If a fixture differs at one of these, explain and document the cause before
  adjusting anything.

---

## Implementation phases

Each phase ends with `cargo test`, `cargo clippy --all-targets` and
`cargo doc --all-features` passing, and with the comment-rewrite pass described in
the user's CLAUDE.md.

### Phase 0 — Scaffolding ✅ Done
- Convert to a workspace with `crates/ze_tree`. Remove the `add()` placeholder.
- Copy the 16 preset JSONs into `crates/ze_tree/presets/` unchanged, and add the ez-tree
  MIT licence notice.
- Add `glam`, `serde` and `serde_json` as required dependencies, as listed in
  Decisions.
- `prior_art/` stays git-ignored, for reference only.
- Create empty modules: `rng`, `options`, `presets`, `skeleton`, `mesh`, `trellis`,
  `material`, `tree`.

### Phase 1 — Foundations ✅ Done
- **1a. Golden harness** (`tools/golden`). ✅ All fixtures generated: RNG, options,
  LOD levels, skeletons and meshes.
- **1b. `rng`.** ✅ Exact port; every draw matches the RNG fixture.
- **1c. `options` + `presets`.** ✅ Types, defaults matching `options.js`, `PerLevel`
  serde, JSON merge semantics, validation (D3) and `TreePreset`. Tests: every preset
  parses, defaults match the JS dump, and options survive a JSON round trip.
  - Merge semantics live in `TreeOptions::from_json`: the input is read as a
    `serde_json::Value`, merged onto `TreeOptions::default()` with the same rules as
    JS `copy()`, then read with the derived `Deserialize`. The derived impls are
    strict, so deserializing `TreeOptions` through serde directly (for example when
    embedded in another type) needs complete options, as written by `to_json`.

### Phase 2 — Skeleton growth ✅ Done
All 24 skeleton fixtures match: structure exactly (branch, section, leaf and segment
counts), floats within the tolerance below. Also tested: the hierarchy (parents come
first, levels step by one, every branch and leaf starts exactly at its `Attachment`),
repeatability, tip behaviour and validation.

**Phase 2 tolerance (set from evidence, `tests/skeleton.rs`).** Floats are compared by
how far they would move a mesh vertex, relative to the tree's extent: origins by
their distance; branch rings by orientation angle × radius; leaf quads by angle ×
leaf size. The limit is **1e-4**. The worst seen is 4.8e-5 (a leaf on a thin bush_3
branch), and most cases are below 1e-5. Radius and leaf size: **1e-6** relative
(worst about 3e-7).
- Raw angles are not compared. On branches thinner than about 1/1000 of the trunk,
  bending is scaled by `1/sqrt(radius)` (×170 at r = 3.5e-5), the growth force
  (`strength / radius`) turns each section fully upright, and the XYZ Euler angles sit
  near gimbal lock. Tiny float differences then change the branch's roll by up to
  0.05 rad. This is jitter point 3 below, made chaotic by thin radii.
- **Known exception:** `evergreen_seed_42` gets 1e-3. Branch 274 (level 3, radius
  3.5e-5) rolls up to 0.05 rad differently, and its leaves inherit that roll; the worst
  leaf moves 4.2e-4 of the extent.
- Doing only the placement maths (`start`, `alpha`, interpolated radius) in f64 was
  tried. It improved bush_3 (4.8e-5 → 2.2e-6) but worsened evergreen_levels_2
  (9.5e-6 → 5.1e-5), so it was reverted. Full-f64 growth is the remaining option.
  Decision 7's trigger ("drift beyond ε on the largest presets") is not met: the
  largest presets stay at or below 1e-6.

Original task list:
- Extend `validate()` to reject `branch.sections == 0` (see Status).
- Record the hierarchy that Phase 5 needs: each branch's `level` and `parent`, and
  each leaf's parent (see Phase 5 prerequisites).
- Port `grow_branch`: taper, Euler gnarliness (K6), the growth force rotating toward
  or away from `force.direction` with the clamped angle step, twist, and the trellis
  force (`rotate_towards`).
- Port `generate_child_branches`, `generate_leaves`, `shuffled_indices` and
  `record_leaf`, plus the terminal branch for deciduous trees and the BFS queue.
- Port `trellis::nearest_point` and `trellis::force`.
- Tests: skeleton fixtures match for every preset and seed, structure exactly and
  floats within ε. Set and record ε in this phase.

### Phase 3 — Meshing and LOD ⏭ Next
To mesh a fixture skeleton on its own, build a `Skeleton` from the fixture in the
test. The fixtures have no `level` / `parent` / leaf `parent`. Fill them from a Rust
`Skeleton::generate` of the same options (the structure matches exactly) rather than
regenerating the fixtures.
- `mesh_branch`: ring sampling with stride that always keeps the first and last rings,
  `segments = max(3, round(n*factor))`, `wraps_x`, the duplicated seam vertex, and the
  index winding `v1,v3,v2,v2,v3,v4`.
- `mesh_leaf`: single and double billboards, rounded normals, UVs and indices.
- `LodDetail` defaults and normalisation (`max(1, floor(stride))`), and
  `default_lod_levels()` with the same values, sorted by distance like `generateLODs`.
- Tests: mesh fixtures (counts exact, floats within ε), and meshing of fixture
  skeletons on its own.

### Phase 4 — Tree wrapper, trellis layout, material params
- A `Tree` wrapper with `generate`, `create_geometry`, `generate_lods`, returning
  `Vec<(LodLevel, TreeMesh)>`, `vertex_count`, `triangle_count` and preset/JSON loading.
- `trellis_layout`: the number of horizontal and vertical lines, their positions and
  lengths, the same as `Trellis.generate`.
- Material parameter structs and `WindParams`.
- **Wind shader in WESL.** Port the leaf wind from `#createLeafMaterial`
  (`onBeforeCompile`) to `crates/ze_tree/shaders/wind.wesl`:
  - `simplex3` (the ashima simplex noise, ported as is) and
    `wind_sway(position, uv_y, time, params) -> vec3<f32>`, which is the three-sine sway
    weighted by `uv.y`. Keep JS's `2.0 * 3.14`, not π, because it changes the pattern.
  - A `WindParams` uniform struct whose layout matches the Rust `WindParams`
    (`strength: vec3`, `frequency`, `scale`), with defaults `(0.5,0,0.5)`, `0.5`, `70`.
  - Exposed from Rust as `pub const WIND_WESL: &str` (`include_str!`). Shader text needs
    no runtime dependency, so it is always available.
  - The rounded-normals flag (skip the backface normal flip) is a fragment concern for
    the engine material. Core only documents it.
  - Tests: compile the module with `wesl` (dev-dependency, `0.6`, the version bevy 0.20
    uses) and validate the output with naga. A CPU port of `wind_sway` in the test
    checks a few hand-computed values against the GLSL maths.

### Phase 5 — Exports
Each format is an optional cargo feature, off by default. All of them use the
segmentation and rig data below.

**Prerequisites in earlier phases** (structure only; they draw no RNG):
- Phase 2: `SkeletonBranch` records `level` and
  `parent: Option<BranchAttachment { branch: usize, section: usize, t: f32 }>`, the parent
  branch and where along it the child starts. `SkeletonLeaf` records its parent branch
  and section the same way.
- Phase 3: `TreeMesh` records a `BranchSpan { branch, level, vertices: Range<u32>,
  indices: Range<u32>, sections: Vec<usize> }` per branch, where `sections` lists the
  skeleton sections kept by `section_stride`, and a vertex range per leaf. Exporters
  split and skin meshes from these ranges.

**Segmentation.** `MeshPart` is `Trunk` (level 0), `Branches(level)` for levels 1–3,
and `Leaves`. `TreeMesh::part(MeshPart) -> MeshBuffers` extracts one part with
re-based indices. Exporters write each part as its own named mesh or group: `trunk`,
`branches_1` … `branches_3` (or a single merged `branches`, as an option) and
`leaves`, with the bark material on wood and the leaf material on leaves.

**Rig** (`rig.rs`, always compiled, no dependencies):
- `Rig::build(&Skeleton, &TreeMesh, &RigOptions) -> Rig`. It produces joints, each
  with a parent, a local transform and an inverse bind matrix, plus per-vertex
  `joints: Vec<[u16; 4]>` and `weights: Vec<[f32; 4]>` for each part.
- Joints: one per kept section of each branch up to `RigOptions::max_level`. A
  branch's first joint is parented to its parent branch's joint at the attachment
  section, so the joint tree follows the branch tree.
- Weights: each ring of branch vertices binds to its section's joint, optionally
  blended with the next joint along the branch. Branches deeper than `max_level`, and
  all leaves, bind rigidly to the nearest ancestor joint.
- Joint budget (Decision 10): `RigOptions::max_joints` defaults to **64** and may be
  set up to **256**; anything above 256 is an error. Large trees reach thousands of
  sections, and bevy caps a skinned mesh at 256 joints.
  - The fitting algorithm starts at the trunk and works outward. It gives the trunk
    its joints first, then each level in turn, coarsening the section stride of the
    outer levels (and finally dropping whole levels) until the rig fits the budget.
  - `max_level` and an explicit stride remain available as overrides. `Rig` reports
    its joint count and the stride chosen for each level.
- Tests: the joint tree is acyclic with one root, weights sum to 1, every vertex has a
  joint, and at bind pose the skinned positions equal the unskinned ones.

**Formats:**
- `export-obj`: OBJ plus MTL with no dependencies. One `g`/`o` group per part and
  `usemtl bark` / `usemtl leaves`. OBJ has no skinning, so no rig.
- `export-gltf`: glTF 2.0 as `.gltf` plus `.bin`, or as a single `.glb`, written with
  `gltf-json` 1.4 (Decision 11).
  - One mesh node per part, with PBR materials from the material params: bark tint;
    leaves get tint, `alphaMode: MASK`, `alphaCutoff` and `doubleSided`. Texture names
    (`bark.kind`, `leaves.kind`) are written as image URIs only when the caller
    supplies a texture path resolver.
  - An optional skin: the rig's joints as a node hierarchy, with `JOINTS_0`,
    `WEIGHTS_0` and inverse bind matrices.
  - Optional LODs, each as its own set of nodes. The `MSFT_lod` extension is a
    possible follow-up.
  - The options JSON goes in the asset `extras`, so a tree can be regenerated from the
    file.
  - Tests: re-import with the `gltf` crate (a dev-dependency), then check structure,
    accessor counts and bounds, and that the skin is valid.
- Out of scope for now: FBX (no maintained Rust writer) and USD (UsdSkel in hand-written
  USDA is possible later, if wanted).
- `examples/export.rs` writes every preset in every enabled format, to inspect by eye
  in Blender.

### Phase 6 — Polish
- Crate docs and a README with a feature map against ez-tree.

### Deferred: `ze_tree_bevy` (not in this plan)
- A bark `StandardMaterial`: tint, flat shading, texture maps and Y repeat.
- A leaf material: alpha mask, double-sided, a vertex shader that imports core's
  `wind.wesl` (bevy 0.20 loads WESL natively), and the custom-normals flag that skips
  the backface normal flip.
- LOD switching with bevy `VisibilityRange` or a custom distance-and-hysteresis
  system.
- Trellis cylinder meshes built from `trellis_layout`.

---

## Agent working notes

- Check behaviour against `prior_art/ez-tree/src/lib/*.js`, and against three r167 in
  `tools/golden/node_modules/three/src/math/` when the meaning of a three.js call
  matters. Examples: `slerp` direction, `premultiply` vs `multiply`, `setFromUnitVectors`,
  `rotateTowards`, and Euler order.
- Port the *algorithm*, not the JS syntax. Write idiomatic glam code. But keep RNG draw
  order and the kept quirks K1–K8 exact.
- Never widen a tolerance to make a test pass. A structural mismatch means the
  algorithm is wrong. A float mismatch beyond ε means a math mistake, or one of the
  documented points where jitter can change the outcome, which then has to be
  explained.
- Record any new quirk in the kept/dropped tables.

---

## Decisions

1. **Workspace layout:** convert now to a virtual workspace. The core crate goes in
   `crates/ze_tree`, and `crates/ze_tree_bevy` is added later.
2. **Fixture storage:** commit plain JSON in `crates/ze_tree/tests/fixtures/`. Move to
   gzip only if the total grows past about 10 MB. That would need a new decision,
   because it adds `flate2`.
3. **Seed type:** `seed: i32` in the API. The serde deserializer accepts any JSON number
   (large, negative or fractional) and converts it the way JS `ToInt32` does, so pasted
   JS presets behave identically.
4. **serde:** `serde` and `serde_json` are **required dependencies**, not a feature.
   The serde derives, `TreeOptions::from_json` / `to_json` and `TreePreset` are always
   available, and there are no `cfg` gates in the code or tests. (Originally a
   default-on feature; dropped to remove the gating and a duplicated options struct.)
5. **Output element type:** `Vec<[f32;3]>` and `Vec<[f32;2]>`, with `Vec<u32>` indices.
6. **Benchmarks:** skipped for now. No `criterion` or `divan`.
7. **Math crate:** `glam` with f32. Pin it to the version used by the targeted bevy
   release, and check that version when adding the dependency in Phase 0.
8. **The `'Deciduous'` typo:** fix the code, keep the upstream behaviour, and leave a
   TODO (K1).
9. **Wind:** a WESL module in core (`shaders/wind.wesl`, `WIND_WESL`), validated with
   `wesl` 0.6 and naga as dev-dependencies. bevy imports it instead of maintaining a
   WGSL port.
10. **Rig joint budget:** configurable. Default 64, hard maximum 256, fitted from the
    trunk outward by coarsening outer levels first.
11. **Export formats:** glTF (`export-gltf`, built on `gltf-json`, re-imported with
    `gltf` in tests) and OBJ (`export-obj`, no dependencies). USD, FBX, PLY and STL are
    out of scope for now.
