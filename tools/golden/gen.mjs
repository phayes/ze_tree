// Regenerates reference data for the Rust tests using ez-tree in prior_art/.
// Run `npm run gen` from tools/golden after installing its npm dependencies.
// This overwrites the generated fixture files; it does not modify prior_art/.
//
// Output goes to crates/ze_tree/tests/fixtures/:
//   rng.json                    random draws for selected seeds
//   lod_levels.json             default levels of detail (LOD)
//   options/default.json        default tree settings
//   options/presets_merged.json presets with all defaults filled in
//   skeletons/<case>.json       branch structure and leaf placements
//   meshes/<case>__<lod>.json   mesh data for each selected level of detail

import { build } from 'esbuild';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as THREE from 'three';

const here = path.dirname(fileURLToPath(import.meta.url));
const presetDir = path.resolve(here, '../../prior_art/ez-tree/src/lib/presets');
const outDir = path.resolve(here, '../../crates/ze_tree/tests/fixtures');

// Bundle ez-tree so Node can load its extensionless and JSON imports.
// Keep three external so both the bundle and this script use the same instance.
const bundlePath = path.join(here, '.build', 'ez-tree.mjs');
await build({
  entryPoints: [path.join(here, 'entry.js')],
  bundle: true,
  format: 'esm',
  platform: 'node',
  external: ['three'],
  outfile: bundlePath,
  logLevel: 'warning',
});
const { Tree, RNG, TreeOptions } = await import(bundlePath);

// ---------------------------------------------------------------------------
// Writing fixture files

// Keep nine significant digits in geometry files to reduce their size.
// Nine digits are sufficient to preserve values already represented as f32.
function round9(_key, value) {
  if (typeof value === 'number' && !Number.isInteger(value)) {
    return Number(value.toPrecision(9));
  }
  return value;
}

function write(rel, data, { pretty = false, round = false } = {}) {
  const file = path.join(outDir, rel);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const text = JSON.stringify(data, round ? round9 : null, pretty ? 2 : 0);
  fs.writeFileSync(file, text + '\n');
  return text.length;
}

function presetNames() {
  return fs
    .readdirSync(presetDir)
    .filter((f) => f.endsWith('.json'))
    .map((f) => path.basename(f, '.json'))
    .sort();
}

function loadPresetJson(name) {
  return JSON.parse(fs.readFileSync(path.join(presetDir, `${name}.json`), 'utf8'));
}

// Apply partial settings over defaults using ez-tree's own merge behaviour.
function optionsFrom(json) {
  const options = new TreeOptions();
  options.copy(structuredClone(json));
  return options;
}

// Convert options to plain JSON; no textures are loaded, so texture slots are null.
function optionsJson(options) {
  return JSON.parse(JSON.stringify(options));
}

const sizes = [];
function record(rel, bytes) {
  sizes.push([rel, bytes]);
}

// ---------------------------------------------------------------------------
// Reference random sequences for ordinary, negative, and large seeds.
// Multiplying each unit-interval draw by 2^32 recovers its exact integer value.

{
  const seeds = [
    0, 1, -1, 42, 12345, 26867, -987654321, 2147483647, -2147483648,
    3000000000, 4294967296 + 5, 1e10,
  ];
  const streams = seeds.map((seed) => {
    const rng = new RNG(seed);
    const values = [];
    for (let i = 0; i < 256; i++) values.push(rng.random() * 4294967296);
    return { seed, values };
  });
  record('rng.json', write('rng.json', streams));
}

// ---------------------------------------------------------------------------
// Default settings and presets with defaults filled in

record('lod_levels.json', write('lod_levels.json', Tree.defaultLODLevels, { pretty: true }));
record(
  'options/default.json',
  write('options/default.json', optionsJson(new TreeOptions()), { pretty: true }),
);
{
  const merged = {};
  for (const name of presetNames()) merged[name] = optionsJson(optionsFrom(loadPresetJson(name)));
  record('options/presets_merged.json', write('options/presets_merged.json', merged, { pretty: true }));
}

// ---------------------------------------------------------------------------
// Branch structures and leaf placements

// Each case pairs a fixture name with partial settings to apply over defaults.
const skeletonCases = [];
for (const name of presetNames()) skeletonCases.push([name, loadPresetJson(name)]);
skeletonCases.push(['default', {}]);
for (const seed of [1, -7]) {
  skeletonCases.push([`deciduous_seed_${seed}`, { seed }]);
}
for (const seed of [1, 42]) {
  skeletonCases.push([`evergreen_seed_${seed}`, { seed, type: 'evergreen' }]);
}
skeletonCases.push(['deciduous_levels_0', { seed: 3, branch: { levels: 0 } }]);
skeletonCases.push(['deciduous_levels_1', { seed: 3, branch: { levels: 1 } }]);
skeletonCases.push(['evergreen_levels_2', { seed: 3, type: 'evergreen', branch: { levels: 2 } }]);

function quat(euler) {
  const q = new THREE.Quaternion().setFromEuler(euler);
  return [q.x, q.y, q.z, q.w];
}

// Store each cross-section or leaf as a compact row. The first three values
// are position; qx, qy, qz, qw describe rotation as a quaternion:
//   section: [ox, oy, oz, qx, qy, qz, qw, radius]
//   leaf:    [ox, oy, oz, qx, qy, qz, qw, size]
function skeletonJson(skeleton) {
  return {
    branches: skeleton.branches.map((b) => ({
      segmentCount: b.segmentCount,
      baseRadius: b.baseRadius,
      sections: b.sections.map((s) => [
        s.origin.x, s.origin.y, s.origin.z, ...quat(s.orientation), s.radius,
      ]),
    })),
    leaves: skeleton.leaves.map((l) => [
      l.origin.x, l.origin.y, l.origin.z, ...quat(l.orientation), l.size,
    ]),
  };
}

for (const [name, json] of skeletonCases) {
  const options = optionsFrom(json);
  const tree = new Tree(options);
  tree.generate();
  const data = { options: optionsJson(options), ...skeletonJson(tree.skeleton) };
  record(`skeletons/${name}.json`, write(`skeletons/${name}.json`, data, { round: true }));
}

// ---------------------------------------------------------------------------
// Mesh data at each default level of detail, before conversion to typed arrays.
// Generate one level at a time so the tree exposes the buffers for that level.

// Limit file sizes with three small cases: a broadleaf tree, a conifer, and a bush.
const meshCases = ['aspen_small', 'bush_2', 'evergreen_levels_2'];
const lodDetails = Tree.defaultLODLevels.map((level, i) => [`lod${i}`, level.detail ?? {}]);
const caseOptions = new Map(skeletonCases);

function buffersJson(b) {
  return { positions: b.verts, normals: b.normals, uvs: b.uvs, indices: b.indices };
}

for (const name of meshCases) {
  for (const [lodName, detail] of lodDetails) {
    const tree = new Tree(optionsFrom(caseOptions.get(name)));
    tree.generateLODs([{ distance: 0, detail }]);
    const data = {
      skeleton: name,
      detail,
      branches: buffersJson(tree.branches),
      leaves: buffersJson(tree.leaves),
    };
    const rel = `meshes/${name}__${lodName}.json`;
    record(rel, write(rel, data, { round: true }));
  }
}

// ---------------------------------------------------------------------------

let total = 0;
for (const [rel, bytes] of sizes) {
  total += bytes;
  console.log(`${(bytes / 1024).toFixed(0).padStart(8)} KiB  ${rel}`);
}
console.log(`${(total / 1024).toFixed(0).padStart(8)} KiB  total`);
