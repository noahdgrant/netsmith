#!/usr/bin/env node
// Deals the editor's browser recipes out to the slices of the `recipes` job (.github/workflows/ci.yml),
// balanced by how long each recipe took on a CI run. Run through scripts/ci-recipe-slices, which
// feeds it `editor-verify groups`; it needs Node and the recipe list, no browser and no build.
//
//   slices.mjs N         the proof: every recipe is in exactly one of the N slices, no slice is empty
//   slices.mjs N K       the recipes of slice K (1..N), one line, space-separated: the arguments of
//                        scripts/editor-verify-all
//   slices.mjs weights   (log of a recipes job on stdin) the table scripts/editor-verify-weights
//
// Slicing is by recipe, not by session: the `two_boards` session alone is 43% of the sweep, more
// than one slice. A session split over slices is launched once in each (a few seconds), and a
// slice still launches each session it needs once.
//
// The table has a line `recipe seconds` per recipe, and `* seconds` for the weight of a recipe the
// table does not know, so a new recipe still runs, in the slice that has the least to do.
// The slicing is a pure function of the recipe list and the table (heaviest recipe first, each to
// the lightest slice so far, ties to the lowest slice), so a slice is the same on every run and
// only changes when the table or the recipe list does.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const weightsPath = process.env.NETSMITH_RECIPE_WEIGHTS ?? path.join(here, '..', 'editor-verify-weights');
const [first, second] = process.argv.slice(2);

const die = (message) => {
  console.error(`::error::${message}`);
  process.exit(1);
};

/** `<fixture> <git|-> <recipe>...` per line (`editor-verify groups`) to the recipes in that order. */
function recipesOf(groupsText) {
  const recipes = [];
  for (const line of groupsText.split('\n')) {
    const words = line.trim().split(/\s+/).filter(Boolean);
    if (words.length === 0) continue;
    if (words.length < 3) die(`not a line of \`editor-verify groups\`: ${line}`);
    recipes.push(...words.slice(2));
  }
  return recipes;
}

/** The table: { fallback, seconds: Map<recipe, number> }. */
function readWeights() {
  const seconds = new Map();
  let fallback;
  for (const [i, raw] of fs.readFileSync(weightsPath, 'utf8').split('\n').entries()) {
    const line = raw.replace(/#.*/, '').trim();
    if (!line) continue;
    const m = /^(\S+)\s+(\d+(?:\.\d+)?)$/.exec(line);
    if (!m) die(`${weightsPath}:${i + 1}: expected \`recipe seconds\`, got: ${raw}`);
    if (m[1] === '*') fallback = Number(m[2]);
    else if (seconds.has(m[1])) die(`${weightsPath}:${i + 1}: ${m[1]} is listed twice`);
    else seconds.set(m[1], Number(m[2]));
  }
  if (fallback === undefined) die(`${weightsPath} has no \`* seconds\` line (the weight of a recipe it does not list)`);
  return { fallback, seconds };
}

/** The slices: arrays of recipes (in the order of the list), and their loads in seconds. */
function deal(recipes, n, weights) {
  const weight = (r) => weights.seconds.get(r) ?? weights.fallback;
  const order = recipes.map((r, index) => ({ r, index, w: weight(r) }));
  order.sort((a, b) => b.w - a.w || (a.r < b.r ? -1 : a.r > b.r ? 1 : 0));
  const slices = Array.from({ length: n }, () => ({ items: [], load: 0 }));
  for (const item of order) {
    let lightest = slices[0];
    for (const s of slices) if (s.load < lightest.load) lightest = s;
    lightest.items.push(item);
    lightest.load += item.w;
  }
  return slices.map((s) => ({
    recipes: s.items.sort((a, b) => a.index - b.index).map((i) => i.r),
    load: s.load,
  }));
}

function slicesFor(n) {
  if (!Number.isInteger(n) || n < 1) die(`the number of slices is a positive integer, got ${n}`);
  const recipes = recipesOf(fs.readFileSync(0, 'utf8'));
  if (recipes.length === 0) die('`editor-verify groups` listed no recipes');
  const weights = readWeights();
  return { recipes, weights, slices: deal(recipes, n, weights) };
}

/** Reads the log of a recipes job: the seconds between one `PASS <recipe>` line and the one before it. */
function weightsFromLog(log) {
  const stamp = (line) => {
    const m = /^(\d{4}-\d\d-\d\dT[\d:.]+Z) (.*)$/.exec(line);
    return m ? { t: Date.parse(m[1]), text: m[2] } : null;
  };
  const seconds = new Map();
  let previous = null;
  for (const raw of log.split('\n')) {
    const line = stamp(raw.replace(/\x1b\[[0-9;]*m/g, '').replace(/\r$/, ''));
    if (!line) continue;
    // A session starts at its group line (so the first recipe carries the launch) and ends at the
    // next `endgroup`: the summary at the end of the sweep repeats every PASS line.
    if (/^##\[group\]session /.test(line.text)) previous = line.t;
    else if (/^##\[endgroup\]/.test(line.text)) previous = null;
    else if (previous !== null) {
      const m = /^PASS (\S+)$/.exec(line.text);
      if (m) {
        seconds.set(m[1], Math.max(1, Math.ceil((line.t - previous) / 1000)));
        previous = line.t;
      }
    }
  }
  if (seconds.size === 0) die('no `PASS <recipe>` line inside a `session` group: is this the log of a recipes job?');
  const fallback = Math.max(1, Math.round([...seconds.values()].sort((a, b) => a - b)[Math.floor(seconds.size / 2)]));
  const total = [...seconds.values()].reduce((a, b) => a + b, 0);
  const lines = [
    '# Seconds each editor recipe took on a CI run, for scripts/ci-recipe-slices: it deals the recipes',
    '# out to the slices of the `recipes` job by these numbers. Only the proportions matter, and a',
    '# rough table is enough: the slices only have to be about as long as each other.',
    '#',
    '# Regenerate from the logs of the recipes jobs of one CI run (the job ids are in',
    '# `gh run view <run> --json jobs`; the logs of all the slices are read as one):',
    '#   for id in <job id>...; do gh api --allow-escape-sequences repos/noahdgrant/netsmith/actions/jobs/$id/logs; done |',
    '#     scripts/ci-recipe-slices weights > scripts/editor-verify-weights',
    `# (the sum of this table: ${total} s for ${seconds.size} recipes; the first recipe of a session carries its launch.)`,
    '',
    '# A recipe that is not listed (a new one) weighs this much: the median of the table.',
    `* ${fallback}`,
    '',
    ...[...seconds.keys()].sort().map((r) => `${r} ${seconds.get(r)}`),
  ];
  return `${lines.join('\n')}\n`;
}

if (first === 'weights') {
  process.stdout.write(weightsFromLog(fs.readFileSync(0, 'utf8')));
} else if (first !== undefined && second === undefined) {
  // The proof.
  const n = Number(first);
  const { recipes, weights, slices } = slicesFor(n);
  const where = new Map();
  slices.forEach((s, i) => {
    console.log(`slice ${i + 1}/${n}: ${s.recipes.length} recipes, about ${s.load} s`);
    if (s.recipes.length === 0) die(`slice ${i + 1} is empty`);
    for (const r of s.recipes) where.set(r, [...(where.get(r) ?? []), i + 1]);
  });
  const twice = [...where].filter(([, at]) => at.length > 1);
  if (twice.length) die(`recipes that are in more than one slice: ${twice.map(([r, at]) => `${r} (${at})`).join(', ')}`);
  const unique = new Set(recipes);
  if (unique.size !== recipes.length) die('a recipe is listed in more than one session by `editor-verify groups`');
  const missing = recipes.filter((r) => !where.has(r));
  if (missing.length) die(`recipes that are in no slice: ${missing.join(', ')}`);
  const sum = slices.reduce((a, s) => a + s.recipes.length, 0);
  if (sum !== recipes.length) die(`the slices add up to ${sum} recipes, the sweep has ${recipes.length}`);
  const unknown = recipes.filter((r) => !weights.seconds.has(r));
  if (unknown.length) console.log(`note: no weight for ${unknown.join(', ')}: ${weights.fallback} s each; rebalance (see scripts/editor-verify-weights)`);
  const stale = [...weights.seconds.keys()].filter((r) => !unique.has(r));
  if (stale.length) console.log(`note: the weights name recipes that do not exist: ${stale.join(', ')}`);
  console.log(`ok: ${recipes.length} recipes, each in exactly one of ${n} slices`);
} else if (first !== undefined) {
  const n = Number(first);
  const k = Number(second);
  const { slices } = slicesFor(n);
  if (!Number.isInteger(k) || k < 1 || k > n) die(`the slice is 1..${n}, got ${second}`);
  const recipes = slices[k - 1].recipes;
  // An empty list would make scripts/editor-verify-all drive every recipe.
  if (recipes.length === 0) die(`slice ${k} is empty`);
  console.log(recipes.join(' '));
} else {
  die('usage: scripts/ci-recipe-slices N | N K | weights');
}
