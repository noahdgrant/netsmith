// Moving, rotating, flipping and locking parts on the PCB canvas, snapped to the grid and
// written to the layout file, with undo and redo.
import fs from 'node:fs';
import path from 'node:path';
import { PART, drag, fresh, openPcb, partAt, partOf, scene, settled } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

export async function run(tools) {
  const { page, shot, step, expect, project } = tools;
  const file = path.join(project, 'src/main.pcb.toml');
  const text = () => fs.readFileSync(file, 'utf8');
  await fresh(tools);
  await openPcb(page);
  const key = async (k) => {
    const before = (await scene(page)).layoutStamp;
    await page.keyboard.press(k);
    return settled(page, before);
  };
  const entry = (s, path) => partOf(s, path);
  const CAP = PART.outputCap;

  step('drag the output capacitor: it moves on the grid and the file gets one entry');
  let c = await partAt(page, CAP);
  const was = { x: c.part.x, y: c.part.y };
  await page.mouse.click(c.centre.x, c.centre.y);
  let stamp = c.scene.layoutStamp;
  await drag(page, c.centre, { x: c.centre.x + 83, y: c.centre.y + 61 });
  let s = await settled(page, stamp);
  let moved = entry(s, CAP);
  expect(moved.placement === 'hand', 'the part is placed by hand now');
  expect(Math.abs(moved.x - was.x) > 3 && Math.abs(moved.y - was.y) > 3, `it moved (${was.x}, ${was.y}) to (${moved.x}, ${moved.y})`);
  const rel = (v, origin) => Math.round((v - origin) * 100) / 100;
  expect(rel(moved.x, s.originX) % 0.5 === 0 && rel(moved.y, s.originY) % 0.5 === 0, 'it is on the 0.5 mm grid');
  expect(text().includes('[OutlineAndPours.parts."stage.output_cap"]') && (text().match(/\[OutlineAndPours\.parts\./g) ?? []).length === 1, 'one part entry in the layout file');
  expect(text().includes('[OutlineAndPours.keepouts.antenna]'), 'the keepout entry that was there is still there');
  expect(s.parts.filter((p) => p.placement === 'derived').length === 11, 'the other parts stay derived');
  await shot('moved');

  // The placer chose the turn the part had before the drag and a drag keeps it, so a turn is
  // checked against that, not against zero.
  const turn = moved.rotation;
  const quarter = (turn + 90) % 360;
  step(`R rotates 90 degrees (${turn} to ${quarter})`);
  s = await key('r');
  expect(entry(s, CAP).rotation === quarter, `rotation ${quarter}`);
  expect(text().includes(`rotation = ${quarter}.0`), `the file says rotation ${quarter}`);
  await shot('rotated');

  step('F puts it on the other face');
  s = await key('f');
  expect(entry(s, CAP).side === 'bottom', 'on the bottom');
  expect(text().includes('side = "bottom"'), 'the file says bottom');
  expect((await page.locator('.pcb-host svg g[data-path="stage.output_cap"]').getAttribute('transform')).includes('scale(-1 1)'), 'drawn mirrored');
  await shot('flipped');

  step('L locks it: a lock mark, and a drag does not move it');
  s = await key('l');
  expect(entry(s, CAP).locked === true, 'locked');
  expect((await page.locator('.pcb-overlay .lock-mark').count()) === 1, 'one lock mark');
  expect(text().includes('lock = true'), 'the file says lock = true');
  c = await partAt(page, CAP);
  await drag(page, c.centre, { x: c.centre.x + 60, y: c.centre.y + 40 });
  await page.waitForTimeout(400);
  const still = entry(await scene(page), CAP);
  expect(still.x === entry(s, CAP).x && still.y === entry(s, CAP).y, 'a locked part does not move');
  await page.waitForSelector('.toast');
  await shot('locked');

  step('undo walks back through lock, flip, rotate and move');
  for (const [what, check] of [
    ['lock', (p) => p.locked === false],
    ['flip', (p) => p.side === 'top'],
    ['rotate', (p) => p.rotation === turn],
  ]) {
    const before = (await scene(page)).layoutStamp;
    await page.keyboard.press('Control+z');
    s = await settled(page, before);
    expect(check(entry(s, CAP)), `undo ${what}`);
  }
  const before = (await scene(page)).layoutStamp;
  await page.keyboard.press('Control+z');
  s = await settled(page, before);
  expect(entry(s, CAP).placement === 'derived' && entry(s, CAP).x === was.x && entry(s, CAP).y === was.y, 'undo move: derived again, where the placer had it');
  expect(!text().includes('stage.output_cap'), 'the file has no entry for it');
  expect(text().includes('[OutlineAndPours.keepouts.antenna]'), 'and still the keepout');
  await shot('undone');

  step('redo with Ctrl+Shift+Z');
  const b2 = (await scene(page)).layoutStamp;
  await page.keyboard.press('Control+Shift+z');
  s = await settled(page, b2);
  expect(entry(s, CAP).placement === 'hand' && entry(s, CAP).x === moved.x, 'the move is back');
  expect((await page.locator('[data-testid=undo]').isEnabled()) && (await page.locator('[data-testid=redo]').isEnabled()), 'undo and redo buttons are live');

  step('grid setting: 2.54 mm; a drag ends on it');
  await page.selectOption('[data-testid=grid]', '2.54');
  c = await partAt(page, CAP);
  await page.mouse.click(c.centre.x, c.centre.y);
  stamp = (await scene(page)).layoutStamp;
  await drag(page, c.centre, { x: c.centre.x + 47, y: c.centre.y - 33 });
  s = await settled(page, stamp);
  const e = entry(s, CAP);
  const step254 = (v) => Math.abs(Math.round(v / 2.54) * 2.54 - v) < 0.011;
  expect(step254(rel(e.x, s.originX)) && step254(rel(e.y, s.originY)), `on the 2.54 grid (${rel(e.x, s.originX)}, ${rel(e.y, s.originY)})`);
  await shot('grid');

  step('drag two selected parts together');
  const a = await partAt(page, PART.inputCap);
  const b = entry(a.scene, PART.regulator);
  await page.mouse.click(a.centre.x, a.centre.y);
  await page.keyboard.down('Shift');
  const u7 = await partAt(page, PART.regulator);
  await page.mouse.click(u7.centre.x, u7.centre.y);
  await page.keyboard.up('Shift');
  expect((await page.locator('[data-testid=part-inspector] h3').innerText()).match(/^2 PARTS/i), 'both selected');
  stamp = (await scene(page)).layoutStamp;
  await drag(page, a.centre, { x: a.centre.x - 40, y: a.centre.y + 70 });
  s = await settled(page, stamp);
  const dInput = entry(s, PART.inputCap).x - a.part.x;
  const dReg = entry(s, PART.regulator).x - b.x;
  expect(Math.abs(dInput - dReg) < 0.02 && Math.abs(dInput) > 1, `both moved by the same ${dInput.toFixed(2)} mm`);
  await shot('two-moved');
}
