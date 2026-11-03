// Keepouts on the PCB canvas: the outline, cutout and keepouts are drawn; a keepout is drawn
// with the rectangle tool, moved, resized, changed in the inspector and deleted, and a keepout
// that the source sizes but does not place is moved. All of it is in the layout file.
import fs from 'node:fs';
import path from 'node:path';
import { drag, fresh, openPcb, scene, screenAt, settled } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

export async function run(tools) {
  const { page, shot, step, expect, project } = tools;
  const file = path.join(project, 'src/main.pcb.toml');
  const text = () => fs.readFileSync(file, 'utf8');
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
  let stamp = s.layoutStamp;
  await drag(page, a, b);
  s = await settled(page, stamp);
  const k = find(s, 'keepout1');
  expect(k && k.origin === 'drawn', 'a drawn keepout1');
  expect(k.width === 12 && k.height === 6, `12 by 6 mm, on the 0.5 mm grid (${k.width} by ${k.height})`);
  expect(k.layers.join() === 'top' && !k.vias && k.tracks && k.copper && k.parts, 'top layer; it forbids tracks, copper and parts, not vias');
  const t = text();
  expect(t.includes('[OutlineAndPours.keepouts.keepout1]') && t.includes('size = [12.0, 6.0]') && t.includes('layers = "top"') && t.includes('vias = "allow"'), 'the layout file has it');
  expect(t.includes('[OutlineAndPours.keepouts.antenna]'), 'and the antenna entry that was there');
  expect((await page.locator('[data-testid=keepout-inspector]').count()) === 1, 'the inspector shows it');
  expect((await page.locator('.pcb-overlay .k-picked .handle').count()) === 8, 'eight handles to resize it by');
  await shot('drawn');

  step('drag it by its border: it moves, on the grid');
  const was = rel(s, k);
  const edge = await screenAt(page, k.area.x + k.area.width / 2, k.area.y);
  stamp = s.layoutStamp;
  await drag(page, { x: edge.x + 30, y: edge.y }, { x: edge.x + 30 + 60, y: edge.y + 24 });
  s = await settled(page, stamp);
  const now = rel(s, find(s, 'keepout1'));
  expect(now.x > was.x + 3 && now.y > was.y + 1, `moved from (${was.x}, ${was.y}) to (${now.x}, ${now.y})`);
  expect(now.x % 0.5 === 0 && now.y % 0.5 === 0, 'on the grid');

  step('drag its south-east handle: it is bigger');
  const k2 = find(s, 'keepout1');
  const corner = await screenAt(page, k2.area.x + k2.area.width, k2.area.y + k2.area.height);
  stamp = s.layoutStamp;
  await drag(page, corner, { x: corner.x + 36, y: corner.y + 24 });
  s = await settled(page, stamp);
  const k3 = find(s, 'keepout1');
  expect(k3.width > k2.width && k3.height > k2.height && rel(s, k3).x === rel(s, k2).x, `${k2.width} by ${k2.height} became ${k3.width} by ${k3.height}, the corner opposite stayed`);
  await shot('resized');

  step('the inspector changes what it forbids and the layers');
  stamp = s.layoutStamp;
  await page.locator('[data-testid=keepout-inspector] .checks label', { hasText: 'parts' }).locator('input').uncheck();
  s = await settled(page, stamp);
  expect(!find(s, 'keepout1').parts && text().includes('parts = "allow"'), 'parts are allowed now, and the file says so');
  stamp = s.layoutStamp;
  await page.locator('[data-testid=keepout-inspector] .checks label', { hasText: 'bottom' }).locator('input').check();
  s = await settled(page, stamp);
  expect(find(s, 'keepout1').layers.join() === 'top,bottom' && text().includes('layers = "top,bottom"'), 'on the top and bottom layers');

  step('a keepout the source sizes is moved by its border; the source ones that state a place are not');
  const antenna = find(s, 'antenna');
  const border = await screenAt(page, antenna.area.x + antenna.area.width / 2, antenna.area.y + antenna.area.height);
  stamp = s.layoutStamp;
  await drag(page, border, { x: border.x - 48, y: border.y + 36 });
  s = await settled(page, stamp);
  const moved = rel(s, find(s, 'antenna'));
  expect(moved.x < 18 && moved.y > 2, `the antenna moved from (18, 2) to (${moved.x}, ${moved.y})`);
  const table = text().split('[OutlineAndPours.keepouts.antenna]')[1].split('\n[')[0];
  expect(table.includes('at = [') && !table.includes('size'), 'and only its place is in the file, not its size');
  await shot('antenna-moved');

  step('Delete removes a drawn keepout; undo brings it back');
  await page.locator('.keepout-row', { hasText: 'keepout1' }).click();
  stamp = (await scene(page)).layoutStamp;
  await page.keyboard.press('Delete');
  s = await settled(page, stamp);
  expect(!find(s, 'keepout1') && !text().includes('keepout1'), 'it is gone from the board and the file');
  stamp = s.layoutStamp;
  await page.keyboard.press('Control+z');
  s = await settled(page, stamp);
  const back = find(s, 'keepout1');
  expect(back && back.layers.join() === 'top,bottom' && !back.parts, 'undo gives it back as it was, with its layers and what it forbids');
  await shot('undone');
}
