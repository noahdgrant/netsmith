// What the PCB recipes share (this file is not a recipe: recipes are in recipes/).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
export const BOARD = 'OutlineAndPours';

/** Put the layout file back as the fixture has it (`outline_and_pours` unless `original` says
 * otherwise), and load the page again, so every recipe starts from the same board whatever ran
 * before it. */
export async function fresh({ page, url, project }, original = path.join(here, 'fixtures/outline_and_pours.pcb.toml')) {
  const layout = path.join(project, 'src/main.pcb.toml');
  fs.copyFileSync(original, layout);
  await page.goto(url);
  await page.waitForSelector('.app');
  // Give the server's watcher the moment it needs to notice the file.
  await page.waitForTimeout(700);
  await page.reload();
  await page.waitForSelector('.app');
}

/** Open the PCB section (it fills the window). */
export async function openPcb(page, board = BOARD) {
  await page.selectOption('.boardpick select', board);
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-host svg[data-ready], .pcb-host svg');
  await page.waitForSelector('.pcb-overlay');
}

/** The board's scene, as the server has it now. */
export async function scene(page, board = BOARD) {
  return page.evaluate(async (b) => (await fetch(`/api/pcb/scene?board=${encodeURIComponent(b)}`)).json(), board);
}

/** A point of the board with nothing under it: the point of the outline (3 mm in from the edge)
 * that is farthest from every courtyard, keepout and cutout of the scene. A recipe that must click
 * "empty board" asks the scene for it, because where the parts are is the placer's and the layout
 * file's to choose, not a fixed spot of the outline. */
export function emptySpot(s) {
  const boxes = [...s.parts.map((p) => p.courtyard), ...s.keepouts.map((k) => k.area), ...s.cutouts.map((c) => c.area)];
  const gap = (x, y, b) => Math.hypot(Math.max(b.x - x, 0, x - (b.x + b.width)), Math.max(b.y - y, 0, y - (b.y + b.height)));
  const o = s.outline;
  let best = null;
  for (let x = o.x + 3; x <= o.x + o.width - 3; x += 0.5) {
    for (let y = o.y + 3; y <= o.y + o.height - 3; y += 0.5) {
      const room = Math.min(...boxes.map((b) => gap(x, y, b)));
      if (!best || room > best.room) best = { x, y, room };
    }
  }
  return best;
}

/** Where a point of the board (mm) is in the browser window. */
export async function screenAt(page, x, y) {
  return page.evaluate(
    ([px, py]) => {
      const svg = document.querySelector('.pcb-host svg:not(.pcb-overlay)');
      const m = svg.getScreenCTM();
      return { x: m.a * px + m.c * py + m.e, y: m.b * px + m.d * py + m.f };
    },
    [x, y],
  );
}

/** The parts of the fixture, by instance path. A recipe names a part by what it is (the path in
 * the source), never by its designator: the letters and numbers of designators are the placer's
 * and the catalog's to choose and have changed before. */
export const PART = {
  inputCap: 'stage.input_cap',
  outputCap: 'stage.output_cap',
  regulator: 'stage.regulator',
  powerIn: 'power_in',
};

/** The part at `path` in a scene. */
export function partOf(s, path) {
  const p = s.parts.find((q) => q.path === path);
  if (!p) throw new Error(`the board has no part ${path} (it has ${s.parts.map((q) => q.path).join(', ')})`);
  return p;
}

/** What the page calls a part: its designator, however the scene spells that field. */
export const labelOf = (p) => p.designator ?? p.refdes;

/** A part by path with where its middle is in the window. */
export async function partAt(page, path, board = BOARD) {
  const s = await scene(page, board);
  const p = partOf(s, path);
  const c = p.courtyard;
  return { part: p, scene: s, centre: await screenAt(page, c.x + c.width / 2, c.y + c.height / 2) };
}

/** Drag from one window position to another in a few steps. */
export async function drag(page, from, to, options = {}) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down(options);
  const steps = 8;
  for (let i = 1; i <= steps; i++) {
    await page.mouse.move(from.x + ((to.x - from.x) * i) / steps, from.y + ((to.y - from.y) * i) / steps);
  }
  await page.mouse.up(options);
}

/** Wait until the board has been placed again after an edit: the scene's stamp moved on. */
export async function settled(page, before, board = BOARD) {
  for (let i = 0; i < 50; i++) {
    const s = await scene(page, board);
    if (s.layoutStamp !== before) return s;
    await page.waitForTimeout(100);
  }
  throw new Error('the layout file was not written');
}
