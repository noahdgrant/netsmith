// Placement requirements drawn and held on the PCB canvas (ADR 0045 C4 and C5, issue #815): each
// part's window is drawn over the board, a part outside its window has a ghost at the nearest place
// it allows, the hover and the inspector list the requirements, a drag stops at the window's edge
// and says which requirement holds it, and "Require here" writes the part's place to the physical
// file (and undo takes it out), while a part whose place the file states as a window offers nothing.
import { waitFor, whenIdle } from '../settle.mjs';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, openPcb, partAt, partOf, scene, sceneWhere, screenAt, settled } from '../pcb-helpers.mjs';

export const fixture = 'constrained';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Cased';
const HEADER = 'power';
const OTHER = 'aux';
const PHYSICAL = fs.readFileSync(path.join(here, '../fixtures/constrained.physical.nsm'), 'utf8');

export async function run(tools) {
  const { page, project, shot, step, expect } = tools;
  // The physical file as the fixture has it: this recipe writes to it.
  const physical = path.join(project, 'src/main.physical.nsm');
  if (fs.readFileSync(physical, 'utf8') !== PHYSICAL) fs.writeFileSync(physical, PHYSICAL);
  await fresh(tools, path.join(here, '../fixtures/constrained.pcb.toml'));
  await openPcb(page, BOARD);
  const fit = async () => {
    await page.getByRole('button', { name: 'Fit' }).click();
    await whenIdle(page);
  };
  await fit();
  const tipText = async () => (await page.locator('[data-testid=tip]').innerText()).replace(/\n/g, ' | ');
  const hover = async (p) => {
    await page.mouse.move(p.x + 40, p.y + 40);
    await page.mouse.move(p.x, p.y);
    await page.waitForSelector('[data-testid=tip]');
    return tipText();
  };

  step("the header's window is drawn, and its ghost at the nearest place the window allows (the layout file puts it 28 mm away)");
  const s0 = await scene(page, BOARD);
  const header = partOf(s0, HEADER);
  expect(header.requirements.map((r) => r.field).sort().join() === 'left,y', `the scene has its two requirements (${header.requirements.map((r) => r.id).join(', ')})`);
  expect((await page.locator(`[data-req-part="${HEADER}"] .req-window`).count()) === 1, 'its window is drawn');
  expect((await page.locator(`[data-req-part="${OTHER}"]`).count()) === 0, 'the part with no requirement has none');
  const ghost = page.locator(`[data-req-part="${HEADER}"] [data-testid=req-ghost]`);
  expect((await ghost.count()) === 1, 'the header, outside its window, has a ghost');
  const gx = Number(await ghost.getAttribute('x')) - s0.originX;
  expect(Math.abs(gx - 1.6) < 1e-6, `the ghost's left edge is at the window's nearest end, 1.6 mm (${gx})`);
  await shot('window-and-ghost');

  step('the hover lists each requirement with its window');
  const h = await hover((await partAt(page, HEADER, BOARD)).centre);
  expect(h.includes('power.place.left allows 1.4 to 1.6 mm') && h.includes('power.place.y allows 11.9 to 12.1 mm'), `the hover: ${h}`);

  step('picked, its window is lit, the inspector lists the requirements, and "Require here" is not offered: the file states a window');
  let c = await partAt(page, HEADER, BOARD);
  await page.mouse.click(c.centre.x, c.centre.y);
  await page.waitForSelector(`[data-req-part="${HEADER}"].on`);
  const list = await page.locator('[data-testid=requirement-list]').innerText();
  expect(list.includes('power.place.left') && list.includes('1.4 to 1.6 mm'), `the inspector's list: ${list.replace(/\n/g, ' | ')}`);
  const button = page.locator('[data-testid=require-here]');
  expect(await button.isDisabled(), '"Require here" is disabled');
  expect(((await button.getAttribute('title')) ?? '').includes('power.place.left &= 1.5mm +/- 0.1mm'), 'its title names the line to change');
  await shot('picked');

  step('handed back to the placer, the header lands in its window and the ghost goes');
  let stamp = (await scene(page, BOARD)).layoutStamp;
  await page.getByRole('button', { name: 'Release' }).click();
  let s = await settled(page, stamp, BOARD);
  await page.waitForSelector('[data-testid=findings-chip].ok');
  expect((await page.locator(`[data-req-part="${HEADER}"] [data-testid=req-ghost]`).count()) === 0, 'no ghost');

  step('dragged right, it stops at the edge of its window and says which requirement holds it');
  await fit();
  c = await partAt(page, HEADER, BOARD);
  stamp = c.scene.layoutStamp;
  const before = c.part;
  await page.mouse.move(c.centre.x, c.centre.y);
  await page.mouse.down();
  const to = await screenAt(page, c.part.x + 10, c.part.y);
  for (let i = 1; i <= 10; i++) await page.mouse.move(c.centre.x + ((to.x - c.centre.x) * i) / 10, c.centre.y);
  const held = await tipText();
  expect(held.includes('limited by power.place.left') && held.includes('allows 1.4 to 1.6 mm'), `the note while held: ${held}`);
  await shot('held-at-the-edge');
  await page.mouse.up();
  s = await settled(page, stamp, BOARD);
  const after = partOf(s, HEADER);
  const left = after.body.x - s.originX;
  expect(after.x >= before.x && left <= 1.6 + 5e-4, `the body's left edge is still in the window (${left})`);
  expect((await page.locator('[data-testid=tip]').count()) === 0, 'the note goes when the drag ends');
  await page.waitForSelector('[data-testid=findings-chip].ok');
  expect((await page.locator('[data-testid=findings-chip]').innerText()) === 'no findings', 'and no finding');

  step('"Require here" on the other part writes its place, turn and face to the physical file');
  await fit();
  c = await partAt(page, OTHER, BOARD);
  await page.mouse.click(c.centre.x, c.centre.y);
  await page.waitForSelector('[data-testid=require-here]:not([disabled])');
  expect((await button.innerText()).includes('Require here'), 'it is offered');
  await button.click();
  s = await sceneWhere(page, 'the part to be required where it is', (x) => partOf(x, OTHER).required.x != null, BOARD);
  const req = partOf(s, OTHER).required;
  const text = fs.readFileSync(physical, 'utf8');
  expect(text.startsWith(PHYSICAL.replace(/}\n$/, '')), 'the rest of the file stays as it was');
  expect(text.includes(`.aux.place.x &= ${req.x}mm`) && text.includes(`.aux.place.y &= ${req.y}mm`) && text.includes('.aux.place.side &= "top"'), `the file has its place: ${text.split('\n').filter((l) => l.includes('.aux.')).join(' | ')}`);
  const moved = partOf(s, OTHER);
  expect(Math.abs(moved.body.x + moved.body.width / 2 - s.originX - req.x) < 5e-4, 'the part stays where it was');
  await page.waitForSelector(`[data-req-part="${OTHER}"].on`);
  expect((await button.innerText()).includes('Drop requirement'), 'the button now takes it out');
  await shot('required-here');

  step('a drag cannot move it now: its requirements fix it');
  c = await partAt(page, OTHER, BOARD);
  await page.mouse.move(c.centre.x, c.centre.y);
  await page.mouse.down();
  const far = await screenAt(page, c.part.x + 6, c.part.y + 3);
  for (let i = 1; i <= 6; i++) await page.mouse.move(c.centre.x + ((far.x - c.centre.x) * i) / 6, c.centre.y + ((far.y - c.centre.y) * i) / 6);
  const fixed = await tipText();
  expect(fixed.includes('limited by aux.place.'), `the note: ${fixed}`);
  await page.mouse.up();
  await whenIdle(page);
  const still = partOf(await scene(page, BOARD), OTHER);
  expect(still.x === c.part.x && still.y === c.part.y, 'it did not move');

  step('undo takes the requirement out of the physical file again');
  await page.keyboard.press('Control+z');
  await sceneWhere(page, 'the requirement to be taken out', (x) => partOf(x, OTHER).required.x == null, BOARD);
  await waitFor('the physical file to be as the fixture has it', () => fs.readFileSync(physical, 'utf8') === PHYSICAL);
  expect((await page.locator(`[data-req-part="${OTHER}"]`).count()) === 0, 'its window is gone');

  step('the Requirements layer hides the windows');
  const layer = async (name, on) => {
    const box = page.locator('.layer-list label', { hasText: name }).locator('input');
    if (!(await page.locator('.layer-list').isVisible())) await page.locator('.layers summary').click();
    await (on ? box.check() : box.uncheck());
    await whenIdle(page);
  };
  await layer('Requirements', false);
  expect(!(await page.locator(`[data-req-part="${HEADER}"] .req-window`).isVisible()), 'hidden');
  await layer('Requirements', true);
  expect(await page.locator(`[data-req-part="${HEADER}"] .req-window`).isVisible(), 'shown again');
  await page.locator('.layers summary').click();
}
