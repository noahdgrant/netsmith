// A requirement on where a part goes (ADR 0045), broken by the layout file: the findings tab lists
// the constraint finding with its numbers, a click zooms to it, and handing the part back to
// the placer, which keeps to the requirement, clears it. (The windows and the drag clamp are
// not drawn yet; this checks the findings only.)
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, labelOf, openPcb, partAt, partOf, scene, settled } from '../pcb-helpers.mjs';

export const fixture = 'constrained';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Cased';
const HEADER = 'power';

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/constrained.pcb.toml'));
  await openPcb(page, BOARD);
  const s0 = await scene(page, BOARD);
  const name = labelOf(partOf(s0, HEADER));

  step('the layout file breaks the requirement: the chip counts one finding');
  await page.waitForSelector('[data-testid=findings-chip].bad');
  expect((await page.locator('[data-testid=findings-chip]').innerText()) === '1 finding', 'the chip says 1 finding');
  expect(partOf(s0, HEADER).placement === 'hand', 'the header is where the layout file put it');

  step('the findings tab lists it with the line, the part, the measured value and the window');
  await page.locator('[data-testid=findings-chip]').click();
  await page.waitForSelector('[data-testid=finding]');
  expect((await page.locator('[data-testid=finding]').count()) === 1, 'one finding');
  const row = (await page.locator('[data-testid=finding]').innerText()).replace(/\n/g, ' | ');
  expect(/constraint/.test(row) && row.includes('power.place.left'), `it names the rule and the requirement (${row})`);
  expect(row.includes(name) && row.includes('(F.Fab)'), `it names the part ${name} and the drawing measured`);
  expect(row.includes('28.73 mm') && row.includes('1.4 to 1.6 mm') && row.includes('27.13 mm over'), 'it has the measured value, the window and how far over');
  // The numbers are at the end of the message: the list must not cut them off.
  const shown = () => page.locator('.finding-msg').evaluate((el) => el.scrollHeight <= el.clientHeight + 1);
  expect(await shown(), 'the whole message is on screen, not cut short');
  expect((await page.locator('[data-testid=finding].error').count()) === 1, 'it is an error, as fab makes it');
  expect((await page.locator('.pcb-overlay .mark').count()) >= 1, 'it is marked on the board');
  await shot('constraint-finding');

  step('click it: the canvas zooms to it, marks it and selects the part');
  await page.getByRole('button', { name: 'Fit' }).click();
  await page.waitForTimeout(200);
  const zoomBefore = await page.locator('.zoombar span').innerText();
  await page.locator('[data-testid=finding]').click();
  await page.waitForTimeout(300);
  expect((await page.locator('.zoombar span').innerText()) !== zoomBefore, 'the zoom changed');
  expect((await page.locator('.pcb-overlay .mark.on').count()) === 1, 'the finding is highlighted');
  expect((await page.locator('[data-testid=part-inspector] h3').innerText()).startsWith(name), `${name} is selected`);
  expect(await shown(), 'the picked finding shows all of its message');
  await shot('constraint-clicked');

  step('hand the part back to the placer: it keeps to the requirement and the finding goes');
  await page.getByRole('button', { name: 'Fit' }).click();
  const before = (await scene(page, BOARD)).layoutStamp;
  await page.getByRole('button', { name: 'Release' }).click();
  const s = await settled(page, before, BOARD);
  expect(partOf(s, HEADER).placement === 'derived', 'the header is derived again');
  await page.waitForSelector('[data-testid=findings-chip].ok');
  expect((await page.locator('[data-testid=findings-chip]').innerText()) === 'no findings', 'no findings');
  expect((await page.locator('[data-testid=finding]').count()) === 0, 'none listed');
  await shot('placer-keeps-it');

  step('undo: the part is back where the file had it, and so is the finding with the same numbers');
  await page.keyboard.press('Control+z');
  await page.waitForSelector('[data-testid=findings-chip].bad');
  await page.waitForSelector('[data-testid=finding]');
  const again = await page.locator('[data-testid=finding]').innerText();
  expect(again.includes('28.73 mm') && again.includes('27.13 mm over'), 'the same numbers');
  expect((await partAt(page, HEADER, BOARD)).part.placement === 'hand', 'placed by hand again');
}
