// The sections fill the window one at a time: the number keys switch them, nothing overflows at a
// laptop's 1366 x 768 or a large screen's 2000 x 1000, and the layout is kept across a reload.
export const fixture = 'two_boards';

const KEYS = [['1', 'source'], ['2', 'schematic'], ['3', 'power'], ['4', 'system'], ['5', 'bom'], ['6', 'pcb'], ['7', 'build'], ['8', 'compare']];

export async function run({ page, shot, step, expect }) {
  const active = () => page.evaluate(() => [...document.querySelectorAll('.pane-strip .tab.active')].map((t) => t.dataset.section));
  const overflows = () => page.evaluate(() => document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight);
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  for (const [w, h] of [[1366, 768], [2000, 1000]]) {
    await page.setViewportSize({ width: w, height: h });
    step(`every section at ${w} x ${h}`);
    for (const [key, id] of KEYS) {
      await page.keyboard.press(key);
      await page.waitForSelector(`.section.sec-${id}:not(.away)`);
      await page.waitForTimeout(400);
      expect((await active()).join() === id, `key ${key} opens ${id}`);
      const box = await page.locator(`.section.sec-${id}`).boundingBox();
      // The sheet is set into the frame by its gap, 8 px on each side.
      expect(box.width >= w - 16 - 2, `${id} fills the window width inside the frame (${Math.round(box.width)} of ${w})`);
      expect(!(await overflows()), `${id}: nothing overflows the page`);
      await shot(`${id}-${w}`);
    }
  }

  step('a section key moves the focus with the choice when it was on a tab, and leaves no ring on a tab that is not on show');
  await page.keyboard.press('2');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  await page.locator('.pane-strip .tab[data-section="schematic"]').focus();
  await page.keyboard.press('8');
  await page.waitForSelector('.section.sec-compare:not(.away)');
  await page.waitForTimeout(250);
  const where = await page.evaluate(() => ({ section: document.activeElement?.getAttribute('data-section'), active: document.activeElement?.classList.contains('active'), ring: [...document.querySelectorAll('.pane-strip .tab:focus-visible')].map((t) => t.dataset.section) }));
  expect(where.section === 'compare' && where.active === true, `the focus is on the Compare tab, the one on show (${JSON.stringify(where)})`);
  expect(where.ring.every((s) => s === 'compare'), 'no other tab has a focus ring');
  await page.evaluate(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
  await page.keyboard.press('2');
  await page.waitForSelector('.section.sec-schematic:not(.away)');

  step('Alt+number works in a text field, plain numbers do not');
  await page.keyboard.press('1');
  await page.locator('.tree-filter').click();
  await page.keyboard.press('3');
  expect((await active()).join() === 'source', 'the 3 was typed into the filter');
  await page.keyboard.press('Alt+2');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  expect(true, 'Alt+2 opens the schematic');

  step('the section is kept across a reload');
  // The focus is still in the source's filter field (a plain number is a character there): leave it.
  await page.evaluate(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
  await page.keyboard.press('5');
  await page.reload();
  await page.waitForSelector('.section.sec-bom:not(.away)');
  expect((await active()).join() === 'bom', 'the BOM again');
}
