// Keepouts on the PCB canvas: the outline, cutout and keepouts are drawn; a keepout is drawn
// with the rectangle tool, moved, resized, changed in the inspector and deleted, and a keepout
// that the source sizes but does not place is moved. A drawn keepout is written to the board's
// physical file (issue #809), and the place of the one the source sizes to the layout file.
import fs from 'node:fs';
import path from 'node:path';
import { drag, fresh, openPcb, scene, sceneWhere, screenAt, settled } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

export async function run(tools) {
  const { page, shot, step, expect, project } = tools;
  const file = path.join(project, 'src/main.pcb.toml');
  const text = () => fs.readFileSync(file, 'utf8');
  const physical = () => fs.readFileSync(path.join(project, 'src/main.physical.nsm'), 'utf8');
  await fresh(tools);
  await openPcb(page);
  const find = (s, name) => s.keepouts.find((k) => k.name === name);
  const rel = (s, k) => ({ x: Math.round((k.area.x - s.originX) * 100) / 100, y: Math.round((k.area.y - s.originY) * 100) / 100 });

  step('the outline, the cutout and the keepout the source declares are drawn');
  let s = await scene(page);
  expect((await page.locator('.pcb-host svg [data-keepout="antenna"]').count()) === 1, 'the antenna keepout is drawn');
  expect((await page.locator('.pcb-host svg [data-cutout="slot"]').count()) === 1, 'the slot cutout is drawn');
  expect(s.outline.width === 50 && s.outline.height === 36, 'a 50 by 36 mm outline');
  await shot('declared');

  step('K, the keepout tool: drag a rectangle on the board (top layer only, no vias)');
  await page.keyboard.press('k');
  await page.waitForSelector('[data-testid=keepout-options]');
  await page.selectOption('[data-testid=keepout-options] select', 'top');
  await page.locator('[data-testid=keepout-options] label', { hasText: 'vias' }).locator('input').uncheck();
  const a = await screenAt(page, s.originX + 8, s.originY + 22);
  const b = await screenAt(page, s.originX + 20, s.originY + 28);
  await drag(page, a, b);
  s = await sceneWhere(page, 'keepout1 to be drawn', (s) => find(s, 'keepout1'));
  const k = find(s, 'keepout1');
  expect(k && k.origin === 'drawn', 'a drawn keepout1');
  expect(k.width === 12 && k.height === 6, `12 by 6 mm, on the 0.5 mm grid (${k.width} by ${k.height})`);
  expect(k.layers.join() === 'top' && !k.vias && k.tracks && k.copper && k.parts, 'top layer; it forbids tracks, copper and parts, not vias');
  const p = physical();
  expect(p.includes('.keepout1: Keepout {') && p.includes('.width &= 12mm') && p.includes('.height &= 6mm') && p.includes('.layers &= "top"') && p.includes('.vias &= "allow"'), 'the physical file has it');
  const t = text();
  expect(!t.includes('keepout1') && t.includes('[OutlineAndPours.keepouts.antenna]'), 'and the layout file only the antenna entry that was there');
  expect((await page.locator('[data-testid=keepout-inspector]').count()) === 1, 'the inspector shows it');
  expect((await page.locator('.pcb-overlay .k-picked .handle').count()) === 8, 'eight handles to resize it by');
  await shot('drawn');

  step('drag it by its border: it moves, on the grid');
  const was = rel(s, k);
  const edge = await screenAt(page, k.area.x + k.area.width / 2, k.area.y);
  await drag(page, { x: edge.x + 30, y: edge.y }, { x: edge.x + 30 + 60, y: edge.y + 24 });
  s = await sceneWhere(page, 'keepout1 to move', (s) => rel(s, find(s, 'keepout1')).x !== was.x);
  const now = rel(s, find(s, 'keepout1'));
  expect(now.x > was.x + 3 && now.y > was.y + 1, `moved from (${was.x}, ${was.y}) to (${now.x}, ${now.y})`);
  expect(now.x % 0.5 === 0 && now.y % 0.5 === 0, 'on the grid');

  step('drag its south-east handle: it is bigger');
  const k2 = find(s, 'keepout1');
  const corner = await screenAt(page, k2.area.x + k2.area.width, k2.area.y + k2.area.height);
  await drag(page, corner, { x: corner.x + 36, y: corner.y + 24 });
  s = await sceneWhere(page, 'keepout1 to be resized', (s) => find(s, 'keepout1').width !== k2.width);
  const k3 = find(s, 'keepout1');
  expect(k3.width > k2.width && k3.height > k2.height && rel(s, k3).x === rel(s, k2).x, `${k2.width} by ${k2.height} became ${k3.width} by ${k3.height}, the corner opposite stayed`);
  await shot('resized');

  step('the inspector changes what it forbids and the layers');
  await page.locator('[data-testid=keepout-inspector] .checks label', { hasText: 'parts' }).locator('input').uncheck();
  s = await sceneWhere(page, 'keepout1 to allow parts', (s) => !find(s, 'keepout1').parts);
  expect(physical().includes('.parts &= "allow"'), 'parts are allowed now, and the file says so');
  await page.locator('[data-testid=keepout-inspector] .checks label', { hasText: 'bottom' }).locator('input').check();
  s = await sceneWhere(page, 'keepout1 to be on the bottom too', (s) => find(s, 'keepout1').layers.join() === 'top,bottom');
  expect(physical().includes('.layers &= "top,bottom"'), 'on the top and bottom layers');

  step('a keepout the source sizes is moved by its border; the source ones that state a place are not');
  const antenna = find(s, 'antenna');
  const border = await screenAt(page, antenna.area.x + antenna.area.width / 2, antenna.area.y + antenna.area.height);
  const stamp = s.layoutStamp;
  await drag(page, border, { x: border.x - 48, y: border.y + 36 });
  s = await settled(page, stamp);
  const moved = rel(s, find(s, 'antenna'));
  expect(moved.x < 18 && moved.y > 2, `the antenna moved from (18, 2) to (${moved.x}, ${moved.y})`);
  const table = text().split('[OutlineAndPours.keepouts.antenna]')[1].split('\n[')[0];
  expect(table.includes('at = [') && !table.includes('size'), 'and only its place is in the file, not its size');
  await shot('antenna-moved');

  step('Delete removes a drawn keepout; undo brings it back');
  await page.locator('.keepout-row', { hasText: 'keepout1' }).click();
  await page.keyboard.press('Delete');
  s = await sceneWhere(page, 'keepout1 to be deleted', (s) => !find(s, 'keepout1'));
  expect(!physical().includes('keepout1'), 'it is gone from the board and the file');
  await page.keyboard.press('Control+z');
  s = await sceneWhere(page, 'keepout1 to come back', (s) => find(s, 'keepout1'));
  const back = find(s, 'keepout1');
  expect(back && back.layers.join() === 'top,bottom' && !back.parts, 'undo gives it back as it was, with its layers and what it forbids');
  await shot('undone');
}
