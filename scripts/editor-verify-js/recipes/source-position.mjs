// The source editor keeps, for each open file, where the cursor and the scroll position were: switching to another file and back
// finds both as they were.

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await section('source');
  await page.waitForSelector('.cm-content');

  step('scroll down in main.nsm and put the cursor on a line there');
  await page.locator('.ft-def', { hasText: 'LedBoard' }).click();
  const line = page.locator('.cm-line', { hasText: '.r1: Resistor' }).first();
  await line.click();
  await page.keyboard.press('End');
  const scroller = page.locator('.cm-scroller');
  await scroller.evaluate((e) => (e.scrollTop = e.scrollTop + 120));
  const where = () =>
    page.evaluate(() => ({
      top: Math.round(document.querySelector('.cm-scroller').scrollTop),
      line: document.querySelector('.cm-activeLine')?.textContent ?? '',
    }));
  const before = await where();
  expect(before.top > 0 && before.line.includes('r1'), `scrolled to ${before.top} with the cursor on ${before.line.trim()}`);

  step('open another file, then come back');
  await page.locator('.ft-file', { hasText: 'power.nsm' }).first().click();
  await page.waitForSelector('.tab.active:has-text("power.nsm")');
  await page.locator('.tab', { hasText: 'main.nsm' }).click();
  await page.waitForSelector('.tab.active:has-text("main.nsm")');
  await page.waitForTimeout(400);
  const after = await where();
  expect(after.line === before.line, `the cursor is on the same line (${after.line.trim()})`);
  expect(Math.abs(after.top - before.top) <= 2, `the scroll position is the same (${before.top} and ${after.top})`);
  await shot('position-kept');
}
