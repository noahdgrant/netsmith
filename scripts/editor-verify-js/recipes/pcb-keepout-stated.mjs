// A keepout the physical file states in a shape the editor does not write whole (a sum, a
// length in micrometres, a `.max_height`) is moved and resized on the canvas, and only the numbers
// of its place and size are rewritten in the file (issue #810); what it forbids and its layers
// stay the file's.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { drag, fresh, openPcb, poursPhysical, sceneWhere, screenAt } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

/** The example's antenna, with its place stated as a sum, its height in micrometres and a height limit. */
const STATED = poursPhysical().replace(
  '    .antenna: Keepout {\n',
  '    .antenna: Keepout {\n        .x &= 10mm + 8mm\n        .y &= 2mm\n',
).replace('.height &= 5mm\n', '.height &= 5000um // the antenna\'s own clearance\n')
  .replace('.vias &= "allow"\n', '.vias &= "allow"\n        .max_height &= 4mm\n');

export async function run(tools) {
  const { page, shot, step, expect, project } = tools;
  const physicalFile = path.join(project, 'src/main.physical.nsm');
  const physical = () => fs.readFileSync(physicalFile, 'utf8');
  const layoutFile = path.join(project, 'src/main.pcb.toml');
  // The physical file places the antenna, so the layout file has no entry for it.
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'keepout-stated-'));
  const empty = path.join(scratch, 'main.pcb.toml');
  fs.writeFileSync(empty, 'version = 1\n');
  fs.writeFileSync(physicalFile, STATED);
  await fresh(tools, empty);
  fs.rmSync(scratch, { recursive: true, force: true });
  await openPcb(page);
  const find = (s, name) => s.keepouts.find((k) => k.name === name);
  const rel = (s, k) => ({ x: Math.round((k.area.x - s.originX) * 100) / 100, y: Math.round((k.area.y - s.originY) * 100) / 100 });

  step('the antenna, which the physical file states in another shape, is the editor\'s to move');
  let s = await sceneWhere(page, 'the antenna to be read from the physical file', (s) => find(s, 'antenna')?.origin === 'stated');
  let antenna = find(s, 'antenna');
  expect(antenna.height === 5 && antenna.maxHeight === 4, `5000 um high with a 4 mm limit (${antenna.height}, ${antenna.maxHeight})`);
  const border = await screenAt(page, antenna.area.x + antenna.area.width / 2, antenna.area.y + antenna.area.height);
  await page.mouse.click(border.x, border.y);
  await page.waitForSelector('[data-testid=keepout-inspector]');
  expect((await page.locator('.pcb-overlay .k-picked .handle').count()) === 8, 'eight handles to resize it by');
  expect((await page.locator('[data-testid=keepout-inspector] .checks input:not([disabled])').count()) === 0, 'what it forbids and its layers are not changed here');
  expect((await page.locator('[data-testid=keepout-inspector] button', { hasText: 'Delete' }).count()) === 0, 'and it is not deleted here');
  await shot('stated');

  step('drag it by its border: only its place is rewritten, and the rest of the file stays');
  const was = rel(s, antenna);
  // Away from the handle in the middle of the edge, which would resize it.
  await drag(page, { x: border.x + 30, y: border.y }, { x: border.x + 30 + 48, y: border.y });
  s = await sceneWhere(page, 'the antenna to move', (s) => rel(s, find(s, 'antenna')).x !== was.x);
  antenna = find(s, 'antenna');
  const now = rel(s, antenna);
  expect(now.x > was.x && now.y === was.y, `moved from (${was.x}, ${was.y}) to (${now.x}, ${now.y})`);
  expect(physical() === STATED.replace('.x &= 10mm + 8mm', `.x &= ${now.x}mm`), 'the sum became the new number, and nothing else changed');
  expect(antenna.origin === 'stated' && antenna.maxHeight === 4, 'it still has its height limit');
  await shot('moved');

  step('drag its south-east handle: the height in micrometres becomes millimetres');
  const corner = await screenAt(page, antenna.area.x + antenna.area.width, antenna.area.y + antenna.area.height);
  await drag(page, corner, { x: corner.x, y: corner.y + 24 });
  s = await sceneWhere(page, 'the antenna to be resized', (s) => find(s, 'antenna').height !== antenna.height);
  const taller = find(s, 'antenna');
  expect(taller.height > antenna.height && taller.width === antenna.width, `${antenna.height} mm high became ${taller.height}`);
  const text = physical();
  expect(text.includes(`.height &= ${taller.height}mm // the antenna's own clearance\n`) && text.includes('.max_height &= 4mm'), 'the height is in millimetres, its comment and the limit stay');
  await shot('resized');
}
