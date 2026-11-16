// Moving a picked part in the 3D view (issue 772, features/pcb-3d.md, "Moving a part"): a drag on
// a picked part moves it in the board's plane and writes the layout file as a 2D drag does, on the
// grid; R, F, L and the arrows rotate, flip, lock and nudge it; Ctrl+Z undoes; a locked part does
// not move; a part too tall for a height limit stops at its edge and the tip says which limit held
// it; a drag on nothing (or on a part not picked) still orbits and writes nothing.
import { waitFor, whenIdle } from '../settle.mjs';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, labelOf, openPcb, partOf, scene, settled } from '../pcb-helpers.mjs';

export const fixture = 'height_limits';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lidded';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

/** A point of the window where the view picks the part, and where a point `dx`, `dy` millimetres
 * from it in the board's plane (the scene's frame: y up the board) is on screen. */
async function along(page, p, dx, dy) {
  const at = await ask(page, 'project', p);
  if (!at) throw new Error(`the 3D view finds no point of the window that picks the part ${p}`);
  const b = await ask(page, 'box', p);
  const c = [(b.min[0] + b.max[0]) / 2, (b.min[1] + b.max[1]) / 2, b.max[2]];
  const a = await ask(page, 'toScreen', c);
  const z = await ask(page, 'toScreen', [c[0] + dx, c[1] + dy, c[2]]);
  return { from: at, to: { x: at.x + (z.x - a.x), y: at.y + (z.y - a.y) } };
}

/** The 3D view has drawn the scene of the layout stamp `stamp`, and nothing is drawn moved. */
async function redrawn(page, stamp) {
  await page.waitForFunction(([sel, st]) => document.querySelector(sel).dataset.stamp === st, [HOST, stamp]);
  await whenIdle(page);
}

export async function run(tools) {
  const { page, project, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/height_limits.pcb.toml'));
  await openPcb(page, BOARD);
  const layout = path.join(project, 'src/main.pcb.toml');
  let s = await scene(page, BOARD);
  const flat = partOf(s, 'flat');
  const tall = partOf(s, 'tall');
  const entry = (sc, p) => sc.parts.find((x) => x.path === p);

  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`);
  // Straight down and orthographic, so a millimetre is as long everywhere on screen.
  await page.getByTestId('view-top').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'top', HOST);
  await page.keyboard.press('p');
  await whenIdle(page);
  const note = await page.locator('.v3-note').innerText();
  expect(note.includes('Drag a picked part to move it'), `the note says a picked part is dragged: ${note}`);

  step('a drag on a part that is not picked orbits the camera and writes nothing');
  {
    const { from, to } = await along(page, flat.path, 0, 6);
    const before = s.layoutStamp;
    const cam = JSON.stringify(await ask(page, 'camera'));
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 6 });
    await page.mouse.up();
    await whenIdle(page);
    expect(JSON.stringify(await ask(page, 'camera')) !== cam, 'the camera orbited');
    expect((await scene(page, BOARD)).layoutStamp === before, 'the layout file was not written');
    await page.getByTestId('view-top').click();
    await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'top', HOST);
    await whenIdle(page);
  }

  step('a click picks the part; a drag on it moves it in the board plane, on the grid, and writes the layout file');
  {
    const at = await ask(page, 'project', flat.path);
    await page.mouse.click(at.x, at.y);
    await page.waitForFunction((t) => document.querySelector('[data-testid=part-inspector] h3')?.innerText.startsWith(t), labelOf(flat));
    await whenIdle(page);
    // 4 mm toward the board's bottom edge: the sheet's y grows by 4.
    const { from, to } = await along(page, flat.path, 0, -4);
    const before = s.layoutStamp;
    const still = JSON.stringify(await ask(page, 'camera'));
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 8 });
    await page.waitForFunction((sel) => document.querySelector(sel).view3d.offset(), HOST);
    await shot('dragging');
    await page.mouse.up();
    s = await settled(page, before, BOARD);
    const now = entry(s, flat.path);
    const dy = now.y - flat.y;
    expect(Math.abs(dy - 4) <= 0.5 && Math.abs(now.x - flat.x) <= 0.5, `moved 4 mm down the board (by ${dy.toFixed(2)}, ${(now.x - flat.x).toFixed(2)})`);
    const onGrid = (v) => Math.abs(Math.round(v / 0.5) * 0.5 - v) < 0.011;
    expect(onGrid(now.x - s.originX) && onGrid(now.y - s.originY), `on the 0.5 mm grid (${now.x - s.originX}, ${now.y - s.originY})`);
    expect(JSON.stringify(await ask(page, 'camera')) === still, 'the camera did not orbit');
    const text = fs.readFileSync(layout, 'utf8');
    const section = text.split('\n[').find((x) => /^Lidded\.parts\.("flat"|flat)\]/.test(x)) ?? '';
    expect(section.includes('at = [11.0, 16.0]') && section.includes('source = "hand"'), `the layout file has the hand placement: ${section.replace(/\n/g, ' ')}`);
    await redrawn(page, s.layoutStamp);
    expect((await ask(page, 'offset')) === false, 'the scene drawn is the one placed again, with nothing drawn moved');
    await shot('moved');
  }

  /** Press a key and wait for the layout file to be written. */
  const key = async (k) => {
    const before = (await scene(page, BOARD)).layoutStamp;
    await page.keyboard.press(k);
    s = await settled(page, before, BOARD);
    await redrawn(page, s.layoutStamp);
    return entry(s, flat.path);
  };

  step('R rotates the picked part, F flips it to the other face, the arrows nudge it, Ctrl+Z undoes');
  {
    const r0 = entry(s, flat.path).rotation;
    let p = await key('r');
    expect(p.rotation === (r0 + 90) % 360, `rotated a quarter turn (${p.rotation})`);
    p = await key('f');
    expect(p.side === 'bottom', 'flipped to the bottom');
    await shot('flipped');
    p = await key('f');
    expect(p.side === 'top', 'flipped back to the top');
    const x0 = p.x;
    p = await key('ArrowRight');
    expect(Math.abs(p.x - x0 - 0.5) < 0.011, `nudged one grid step right (${(p.x - x0).toFixed(2)})`);
    p = await key('Control+z');
    expect(Math.abs(p.x - x0) < 0.011, 'Ctrl+Z took the nudge back');
  }

  step('L locks it: a drag on it moves nothing and says so; L again unlocks');
  {
    let p = await key('l');
    expect(p.locked === true, 'locked');
    const before = s.layoutStamp;
    const { from, to } = await along(page, flat.path, 3, 0);
    const cam = JSON.stringify(await ask(page, 'camera'));
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 6 });
    await page.mouse.up();
    await page.waitForSelector('.toast', { hasText: 'locked' });
    await whenIdle(page);
    expect((await scene(page, BOARD)).layoutStamp === before, 'a locked part is not written');
    expect(JSON.stringify(await ask(page, 'camera')) === cam, 'nor does the drag orbit');
    p = await key('l');
    expect(p.locked === false, 'unlocked');
  }

  step('a part too tall for the lid stops at its edge, and the tip says which limit held it');
  {
    await page.keyboard.press('Escape');
    const at = await ask(page, 'project', tall.path);
    await page.mouse.click(at.x, at.y);
    await page.waitForFunction((t) => document.querySelector('[data-testid=part-inspector] h3')?.innerText.startsWith(t), labelOf(tall));
    await whenIdle(page);
    // Out of the lid to the right (leaving a limit is free): the lid ends at 20 mm.
    let m = await along(page, tall.path, 18, 0);
    let before = s.layoutStamp;
    await page.mouse.move(m.from.x, m.from.y);
    await page.mouse.down();
    await page.mouse.move(m.to.x, m.to.y, { steps: 10 });
    await page.mouse.up();
    s = await settled(page, before, BOARD);
    await redrawn(page, s.layoutStamp);
    const out = entry(s, tall.path);
    expect(out.body.x >= s.originX + 20 - 0.011, `out of the lid (body from ${(out.body.x - s.originX).toFixed(2)} mm)`);
    // Back in: it stops at the lid's edge.
    m = await along(page, tall.path, -12, 0);
    before = s.layoutStamp;
    await page.mouse.move(m.from.x, m.from.y);
    await page.mouse.down();
    await page.mouse.move(m.to.x, m.to.y, { steps: 10 });
    const tip = await page.waitForSelector('[data-testid=tip-3d]:has-text("limited by lid")');
    expect((await tip.innerText()).includes('lid allows 3 mm'), `the tip names the limit: ${(await tip.innerText()).replace(/\n/g, ' | ')}`);
    await shot('held');
    await page.mouse.up();
    s = await waitFor('the layout file to be written, or the drag to have stopped where it was', async () => {
      const now = await scene(page, BOARD);
      return now.layoutStamp !== before ? now : null;
    });
    const held = entry(s, tall.path);
    const left = held.body.x - s.originX;
    expect(left >= 20 - 0.011 && left < 20 + 0.5 + 0.011, `stopped at the lid's edge (body from ${left.toFixed(2)} mm)`);
    await redrawn(page, s.layoutStamp);
    await shot('stopped');
  }
}
