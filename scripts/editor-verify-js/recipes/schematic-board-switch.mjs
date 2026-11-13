// After a board is switched the schematic still has the previous board's drawing for a moment
// (the new one is on its way). A reveal asked in that moment waits for the drawing of the board in
// front, and is dropped when the board changes: it never lands on the previous board's drawing.
// The drawing's answer is held back (a gate the recipe opens) so the moment lasts as long as the steps need.
import { gate, whenIdle } from '../settle.mjs';
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.waitForSelector('svg.schematic');
  const boards = await page.locator('.boardpick select option').allInnerTexts();
  const from = 'TwoBoards/led';
  const to = 'TwoBoards/power';
  expect(boards.includes(from) && boards.includes(to), `two boards (${boards.join(', ')})`);
  await page.selectOption('.boardpick select', from);
  await page.waitForSelector('.schematic[data-board="LedBoard"]');
  const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), to);
  const part = detail.parts.find((p) => p.path.includes('.')) ?? detail.parts[0];
  const board = await page.evaluate(async () => (await (await fetch('/api/analysis')).json()).boards.find((b) => b.name === 'TwoBoards/power')?.definition);
  expect(!!board, `the other board is drawn as ${board}`);

  const goToAnything = async (text) => {
    await page.keyboard.press('Control+p');
    await page.waitForSelector('.qp[role=dialog]');
    await page.keyboard.press('Control+a');
    await page.keyboard.type(text);
    await page.waitForSelector('.qp-row[role=option]');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.qp[role=dialog]', { state: 'detached' });
  };
  let held = gate();
  await page.route('**/api/schematic.svg*', async (route) => {
    await held.opened;
    await route.continue();
  });

  step('a part of the new board is asked for while its drawing is on the way: it is shown on the new drawing only');
  const detailIn = page.waitForResponse((r) => r.url().includes('/api/board?'));
  await page.selectOption('.boardpick select', to);
  // The page knows the board now and its detail is in; the drawing is held.
  await page.waitForFunction((n) => document.querySelector('.boardpick select')?.value === n, to);
  await detailIn;
  await goToAnything(part.refdes.toLowerCase());
  const stale = await page.evaluate(() => ({
    drawing: document.querySelector('svg.schematic')?.getAttribute('data-board') ?? null,
    ring: document.querySelectorAll('.pulse-ring').length,
  }));
  expect(stale.drawing !== 'LedBoard', `the previous board's drawing is not on screen while the new one loads (${stale.drawing})`);
  expect(stale.ring === 0, 'no ring is drawn on it');
  held.open();
  await page.waitForSelector(`.schematic[data-board="${board}"]`);
  await page.waitForSelector('.schematic-host .pulse-ring');
  expect(true, 'once the drawing is the board\'s the ring is on the part');
  expect(await page.evaluate((r) => !!document.querySelector(`svg.schematic .part.mark:not(.linked)[data-ref="${r}"]`), part.refdes), `${part.refdes} is marked on the new drawing`);
  expect((await page.locator('.selchip').innerText()).includes(part.refdes), 'the chip names it');
  await shot('shown-on-new-board');
  await page.waitForSelector('.schematic-host .pulse-ring', { state: 'detached' });

  step('a part asked for, then the board switched: the reveal is dropped, nothing is shown on the new board');
  held = gate();
  await goToAnything(part.refdes.toLowerCase());
  await page.selectOption('.boardpick select', from);
  held.open();
  await page.waitForSelector('.schematic[data-board="LedBoard"]');
  await whenIdle(page);
  expect((await page.locator('.schematic-host .pulse-ring').count()) === 0, 'no ring on the board that was switched to');
  expect((await page.locator('.selchip').count()) === 0, 'and nothing is selected there');
  await page.unroute('**/api/schematic.svg*');
}
