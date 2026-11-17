// Dragging a sheet in the overview (ADR 0139): a card dropped on another takes its place on the page, the place is written to
// `src/main.sch.toml` as the sheet's `at`, the sheets never moved are set out again round it by how they connect, a press that does not
// move still opens the sheet, Esc cancels a drag, and Delete on a placed card puts it back.
import fs from 'node:fs';
import path from 'node:path';

import { analysedSince, pageVersion, waitFor, whenIdle } from '../settle.mjs';
import { activeSheet, openSheet } from '../schematic-helpers.mjs';

export const fixture = 'big';

const card = (page, name) => page.locator(`[data-sheet-card="${name}"] .ov-card`);
const order = (page) => page.locator('.ov-card .ov-name').allInnerTexts();

/** Press on the card `from`, move onto the card `to` in a few steps, and release (or press Esc before releasing). */
async function drag(page, from, to, { cancel = false, during } = {}) {
  const a = await card(page, from).boundingBox();
  const b = await card(page, to).boundingBox();
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 8 });
  if (during) await during();
  if (cancel) await page.keyboard.press('Escape');
  await page.mouse.up();
}

export async function run({ page, shot, step, expect, project }) {
  const file = path.join(project, 'src/main.sch.toml');
  fs.rmSync(file, { force: true });
  const text = () => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : '');

  await page.waitForSelector('.schematic');
  await openSheet(page, 'All modules');
  await page.waitForSelector('.ov-card');
  const before = await order(page);
  const [first, , , moved] = before;
  expect(before.length >= 24 && first && moved, `the overview has its cards (${before.slice(0, 4).join(', ')}, ...)`);
  expect((await page.locator('.ov-placed').count()) === 0, 'with no file, no card is placed');

  step(`drag ${moved} onto ${first}: it takes the first place, and the file has it`);
  let seen = await pageVersion(page);
  await drag(page, moved, first);
  await analysedSince(page, seen);
  const written = await waitFor('the place in the layout file', () => {
    const t = text();
    return new RegExp(`\\[\\w+\\.sheets\\.${moved}\\]\\nat = \\[[\\d.]+, [\\d.]+\\]\\n`).test(t) && t;
  });
  expect(written.startsWith('version = 1\n'), `the file is written as the editor writes it:\n${written}`);
  const after = await waitFor(`${moved} first in the overview`, async () => {
    const o = await order(page);
    return o[0] === moved && o;
  });
  expect(after[0] === moved, `the dragged card is first (${after.slice(0, 4).join(', ')}, ...)`);
  const rest = (o) => o.filter((n) => n !== moved).join(',');
  expect(rest(after) === rest(before), 'the sheets never moved keep the order of their connections round it');
  expect((await card(page, moved).locator('.ov-placed').count()) === 1, 'the card wears a "placed" mark');
  expect((await activeSheet(page)) === 'All modules', 'the drop opened no sheet');
  await shot('placed');

  step('Esc cancels a drag: nothing is written');
  await drag(page, after[5], after[1], {
    cancel: true,
    during: async () => {
      expect((await page.locator('.ov-item.ov-dragging').count()) === 1, 'while it is dragged the card follows the pointer');
      expect((await page.locator(`[data-sheet-card="${after[1]}"].ov-target`).count()) === 1, `and the card under it, ${after[1]}, is marked`);
      await shot('dragging');
    },
  });
  await whenIdle(page);
  expect(text() === written, 'the file is as it was');
  expect((await order(page)).join(',') === after.join(','), 'and so are the cards');

  step('Delete on the placed card puts it back where its connections set it');
  seen = await pageVersion(page);
  await card(page, moved).focus();
  await page.keyboard.press('Delete');
  await analysedSince(page, seen);
  await waitFor('the place gone from the file', () => text() === 'version = 1\n');
  const back = await waitFor('the cards in their first order', async () => {
    const o = await order(page);
    return o.join(',') === before.join(',') && o;
  });
  expect(back.join(',') === before.join(','), `the cards are as they were (${back.slice(0, 4).join(', ')}, ...)`);
  expect((await page.locator('.ov-placed').count()) === 0, 'and no card is placed');

  step('a press that does not move is still a click: it opens the sheet');
  await card(page, moved).click();
  expect((await activeSheet(page)) === moved, `a click on the card of ${moved} opened it`);
}
