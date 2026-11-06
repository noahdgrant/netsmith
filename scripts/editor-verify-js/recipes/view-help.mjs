// The pages that are a column in the middle of a wide window (the bill of materials, Build, and Compare in compare.mjs): a table's frame is as tall as its
// content (the header, the rows, a counting foot right under the last row) and only past the height the pane has does it scroll inside, with a sticky header and
// the foot kept; the summary above stays put. On a window with room a column of help ("What does this view show?") stands in the margin on the right:
// folded with one click, remembered, and not in the page at all where there is no margin (a narrow window, a split).
import { expectFrameFits } from '../flow-helpers.mjs';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await page.evaluate(() => localStorage.removeItem('netsmith.viewhelp.v1'));
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  const settle = (ms = 300) => page.waitForTimeout(ms);

  step('the bill of materials: its frame is as tall as its rows, with the foot right under the last row; shortened, the table scrolls inside with a sticky header and the foot kept, and the summary tiles stay outside');
  await section('bom');
  await page.waitForSelector('table.bom-table');
  await settle(500);
  const bomSel = { frame: '.bom-frame', scroller: '.bom-scroll', foot: '.bom-foot', section: '.section.sec-bom' };
  const geo = await expectFrameFits(page, expect, 'the bill of materials at 1920 x 1080', bomSel);
  expect(!geo.scrolls && /\d+ lines?/.test(geo.footText) && /\d+ parts?/.test(geo.footText), `a foot counts what is shown (${geo.footText}), and four rows do not scroll`);
  const flags = await page.evaluate(() => ({
    sticky: getComputedStyle(document.querySelector('.bom-table thead th')).position,
    statsInScroller: document.querySelector('.bom-scroll').contains(document.querySelector('.bom-stats')),
    scrollOverflow: getComputedStyle(document.querySelector('.bom-scroll')).overflowY,
  }));
  expect(flags.sticky === 'sticky' && !flags.statsInScroller && flags.scrollOverflow === 'auto', 'the header is sticky, the table scrolls inside its own frame, and the summary tiles are outside it');
  for (const mode of ['By value', 'By module']) {
    await page.locator('.seg button', { hasText: mode }).first().click();
    await settle(300);
    await expectFrameFits(page, expect, `the bill of materials, ${mode.toLowerCase()}`, bomSel);
  }
  await page.locator('.seg button', { hasText: 'Flat' }).first().click();
  await settle(300);
  await page.setViewportSize({ width: 1920, height: 400 });
  await settle(400);
  const cut = await expectFrameFits(page, expect, 'the bill of materials at 1920 x 400', bomSel);
  expect(cut.scrolls, 'a short window makes the rows scroll inside the frame (the frame does not grow past the page)');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await settle(400);
  await page.locator('.bom-filter input').fill('C2');
  await settle(250);
  expect(/of \d+/.test(await page.locator('.bom-foot').innerText()), `filtered, the foot says of how many (${(await page.locator('.bom-foot').innerText()).replace(/\s+/g, ' ')})`);
  await page.locator('.bom-filter input').fill('');

  step('with room (1920 px) a column of help stands in the right margin, beside the page and never over it; one click folds it to its title, and that is remembered');
  const place = () =>
    page.evaluate(() => {
      const vh = document.querySelector('.section.sec-bom .vh');
      if (!vh) return null;
      const r = vh.getBoundingClientRect();
      const page = document.querySelector('.bom-frame').getBoundingClientRect();
      const sec = document.querySelector('.section.sec-bom').getBoundingClientRect();
      return { left: r.left, right: r.right, top: r.top, pageRight: page.right, secRight: sec.right, text: vh.innerText.replace(/\s+/g, ' '), expanded: vh.querySelector('.vh-toggle').getAttribute('aria-expanded'), label: vh.getAttribute('aria-label') };
    });
  const open = await place();
  expect(open && open.left >= open.pageRight + 8 && open.right <= open.secRight, `beside the page, inside the window (${open?.left.toFixed(0)} to ${open?.right.toFixed(0)} px, the table ends at ${open?.pageRight.toFixed(0)})`);
  expect(open.label === 'About this view' && /What does this view show\?/.test(open.text) && /Library/.test(open.text) && open.expanded === 'true', `a landmark that is labelled, with its title and its words (${open.text.slice(0, 60)}...)`);
  await shot('bom-wide');
  await page.locator('.section.sec-bom [data-testid="view-help-toggle"]').click();
  await settle(200);
  const folded = await place();
  expect(folded.expanded === 'false' && !/Library/.test(folded.text), 'folded: only its title');
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('netsmith.viewhelp.v1')).bom === true), 'the choice is kept');
  await page.reload();
  await page.waitForSelector('.app');
  await section('bom');
  await page.waitForSelector('.section.sec-bom .vh');
  expect((await place()).expanded === 'false', 'and it is still folded after a reload');
  await page.locator('.section.sec-bom [data-testid="view-help-toggle"]').focus();
  await page.keyboard.press('Enter');
  await settle(200);
  expect((await place()).expanded === 'true', 'the toggle works from the keyboard');

  step('another view keeps its own choice, and Build has the column too');
  await section('build');
  await page.waitForSelector('.section.sec-build .vh');
  const build = await page.evaluate(() => {
    const vh = document.querySelector('.section.sec-build .vh');
    const cards = [...document.querySelectorAll('.section.sec-build .acard')].map((c) => c.getBoundingClientRect().right);
    return { left: vh.getBoundingClientRect().left, cardsRight: Math.max(...cards), expanded: vh.querySelector('.vh-toggle').getAttribute('aria-expanded'), text: vh.innerText.replace(/\s+/g, ' ') };
  });
  expect(build.expanded === 'true' && build.left >= build.cardsRight + 8 && /Build schematic/.test(build.text), 'Build: open, beside its cards');
  await shot('build-wide');

  step('without the margin (1280 px, or a split) the column is not in the page: nothing to Tab to, nothing over the table');
  await page.setViewportSize({ width: 1280, height: 800 });
  await settle(500);
  expect((await page.locator('.section:not(.away) .vh').count()) === 0, 'no help column at 1280 px');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await settle(400);
  expect((await page.locator('.section:not(.away) .vh').count()) === 1, 'back at 1920 px');
  await section('bom');
  await page.getByRole('button', { name: /Split right/ }).click();
  await page.waitForSelector('.divider');
  await settle(500);
  expect((await page.locator('.section:not(.away) .vh').count()) === 0, 'in a split the panes are half a window: no help column');
  await page.getByRole('button', { name: /Close pane/ }).click();

  step('at 125 % (the root is 20 px) a 1920 px window has no room for it, and nothing overflows');
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 125, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');
  await section('bom');
  await settle(600);
  expect((await page.locator('.section:not(.away) .vh').count()) === 0 && !(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)), 'no column, no overflow');
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 100, motion: 'system' })));
}
