// A board's case model in the 3D view (features/pcb-3d.md, "A case model"; ADR 0109): the case's
// solids are drawn see-through round the board, a toggle hides and shows them, the problems dock
// lists the fit check's findings, and picking one flies the camera to it and lights the case solid
// it is about (showing the case if it was hidden). The fixture's case is two STEP files written by
// hand: a box with a boss under one hole, and a lid 8 mm over the board that a 9 mm header goes
// into.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, openPcb, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'case_fit';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Cased';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

/** Let the view draw what changed (a frame, and a camera move of 180 ms). */
const settle = (page) => page.waitForTimeout(350);

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/case_fit.pcb.toml'));
  await openPcb(page, BOARD);
  const s = await scene(page, BOARD);
  const tall = partOf(s, 'tall');
  const hole = partOf(s, 'hole2');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`, { timeout: 30000 });
  await settle(page);

  step('the case is drawn see-through round the board: the box (six solids) and the lid, with a toggle and a legend entry');
  const cases = await ask(page, 'cases');
  const names = cases.map((c) => `${c.case}:${c.solid}`);
  expect(cases.length === 7 && names.includes('base:boss') && names.includes('lid:lid'), `the case's solids: ${names.join(', ')}`);
  expect(await ask(page, 'caseShown'), 'the case is shown');
  const toggle = page.getByTestId('tool-case');
  expect((await toggle.getAttribute('aria-pressed')) === 'true', 'the Case tool is on');
  expect((await page.getByTestId('case-legend').count()) === 1, 'the legend names the case');
  await shot('case-shown');

  step('the toggle hides the case, and the legend drops it');
  await toggle.click();
  await settle(page);
  expect(!(await ask(page, 'caseShown')), 'the case is hidden');
  expect((await toggle.getAttribute('aria-pressed')) === 'false', 'the Case tool is off');
  expect((await page.getByTestId('case-legend').count()) === 0, 'no legend entry');
  await shot('case-hidden');

  step('the problems dock lists the fit; picking the collision flies to the header, lights the lid and shows the case again');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('[data-testid=dock]');
  const collision = page.locator('[data-testid=problem]', { hasText: 'is inside the solid `lid`' }).first();
  await collision.waitFor({ timeout: 20000 });
  const text = await collision.innerText();
  expect(text.includes(tall.refdes) && text.includes('the solid `lid` of the case model `lid` by 1.00 mm'), `the collision: ${text.replace(/\n/g, ' | ')}`);
  // case_clearance, case_reach, case_unchecked and case_boss, by what each says.
  for (const words of ['from the solid `wall_right`', "a connector at the board's right edge", 'has no height and stands under', 'has no case solid']) {
    expect((await page.locator('[data-testid=problem]', { hasText: words }).count()) > 0, `the dock lists: ${words}`);
  }
  const before = await ask(page, 'project', tall.path);
  await collision.click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.focusedSolids().includes('case:lid:lid'), HOST);
  await page.waitForTimeout(1000);
  expect(await ask(page, 'caseShown'), 'a finding about the case shows it');
  expect((await toggle.getAttribute('aria-pressed')) === 'true', 'and the toggle says so');
  const after = await ask(page, 'project', tall.path);
  expect(before && after && Math.hypot(after.x - before.x, after.y - before.y) > 10, `the camera moved to the finding (${JSON.stringify(before)} to ${JSON.stringify(after)})`);
  await shot('collision-focused');

  step('picking the hole with no boss flies to the hole; no solid is lit');
  const boss = page.locator('[data-testid=problem]', { hasText: 'has no case solid' }).first();
  await boss.click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.focusedSolids().length === 0, HOST);
  await page.waitForTimeout(1000);
  const text2 = await boss.innerText();
  expect(text2.includes(hole.refdes), `the boss finding names the hole: ${text2.replace(/\n/g, ' | ')}`);
  await shot('boss-focused');
}
