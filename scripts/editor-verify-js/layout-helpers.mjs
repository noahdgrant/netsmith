// What the layout recipes share (ADR 0061, stage 4). This file is not a recipe: recipes are in recipes/.
import fs from 'node:fs';
import path from 'node:path';

/** The boards of the `two_boards` fixture. `power` has a module (`stage`: the regulator and two capacitors, which can be arranged) and parts of its
 * own (two headers); `led` is three parts that hang from one another, so no part of it can be moved apart from the others (the block of each holds
 * the sheet's reference part). */
export const POWER = { board: 'TwoBoards/power', definition: 'PowerBoard' };
export const LED = { board: 'TwoBoards/led', definition: 'LedBoard' };

/** The layout file of the scratch project, and a way to read, write and remove it as another program would. */
export function layoutFile(project, name = 'src/main.sch.toml') {
  const file = path.join(project, name);
  return {
    file,
    exists: () => fs.existsSync(file),
    text: () => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : null),
    remove: () => fs.rmSync(file, { force: true }),
    write: (text) => fs.writeFileSync(file, text),
  };
}

/** Pick a board and wait for its drawing; returns the part list (`path` to designator) of the board. */
export async function openBoard(page, { board, definition }) {
  await page.waitForSelector('.schematic');
  await page.selectOption('.boardpick select', board);
  await page.waitForSelector(`.schematic[data-board="${definition}"]`);
  await page.waitForSelector('svg.schematic .part[data-ref]');
  return partsOf(page, board);
}

/** The parts of a board by path: `{ r1: 'R1', ... }`. Recipes name a part by its path and read its designator from here. */
export async function partsOf(page, board) {
  const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
  return Object.fromEntries(detail.parts.map((p) => [p.path, p.refdes]));
}

/** The drawn part on show with a designator. */
export const partSel = (ref) => `svg.schematic > g.sheet-slot:not([style*="none"]) .part[data-ref="${ref}"], svg.schematic .part[data-ref="${ref}"]`;

/** The state of each part on show: the designator, whether it is the sheet's reference part, movable, wears the hand-arranged outline, and
 * where its middle is on the screen. */
export function partStates(page) {
  return page.evaluate(() => {
    const out = {};
    for (const g of document.querySelectorAll('svg.schematic g.part[data-ref]')) {
      if (!g.getClientRects().length) continue;
      const ref = g.getAttribute('data-ref');
      if (out[ref]) continue;
      const hint = g.previousElementSibling?.previousElementSibling?.previousElementSibling;
      const b = g.getBoundingClientRect();
      out[ref] = {
        ref,
        reference: g.hasAttribute('data-reference'),
        movable: g.hasAttribute('data-movable'),
        hinted: !!hint?.classList.contains('part-hint') && hint.classList.contains('hinted'),
        refused: !!hint?.classList.contains('refused'),
        title: g.querySelector(':scope > title')?.textContent ?? '',
        x: b.left + b.width / 2,
        y: b.top + b.height / 2,
        origin: g.getAttribute('data-origin'),
        orient: g.getAttribute('data-orient'),
      };
    }
    return out;
  });
}

/** Screen pixels in one millimetre of the drawing, and in one grid step (1.27 mm). */
export function pixelsPerStep(page) {
  return page.evaluate(() => {
    const svg = document.querySelector('svg.schematic');
    const m = svg.getScreenCTM();
    return m.a * 1.27;
  });
}

/** Where the middle of the part's body is on the screen, for a press that lands on it (not on its text). */
export function bodyPoint(page, ref) {
  return page.evaluate((r) => {
    for (const g of document.querySelectorAll(`svg.schematic g.part[data-ref="${r}"]`)) {
      if (!g.getClientRects().length) continue;
      const lines = [...g.querySelectorAll(':scope > :is(rect, polyline, polygon, circle)')];
      const el = lines[0] ?? g;
      const b = el.getBoundingClientRect();
      const x = b.left + b.width / 2;
      const y = b.top + b.height / 2;
      return { x, y, hit: document.elementFromPoint(x, y)?.closest('g.part')?.getAttribute('data-ref') ?? null };
    }
    return null;
  }, ref);
}

/** The zoom bar's reading and the canvas transform: what a redraw must leave alone. */
export function viewOf(page) {
  return page.evaluate(() => ({
    zoom: document.querySelector('.zoombar span:not(.zsep)')?.textContent ?? '',
    transform: document.querySelector('.schematic-host .canvas')?.style.transform ?? '',
  }));
}

/** The first `[table]` header and its `key = value` lines of the file's text, as `{ header: { key: value } }`: enough to read what a drop wrote. */
export function entries(text) {
  const out = {};
  let at = null;
  for (const line of (text ?? '').split('\n')) {
    const header = /^\[(.+)\]$/.exec(line.trim());
    if (header) out[(at = header[1])] = {};
    else if (at && /=/.test(line)) {
      const [k, ...v] = line.split('=');
      out[at][k.trim()] = v.join('=').trim();
    }
  }
  return out;
}

/** True when `n` is a whole number of grid steps (1.27 mm) to the file's two decimals. */
export const onGrid = (n) => Math.abs(Math.round(n / 1.27) * 1.27 - n) < 0.006;
