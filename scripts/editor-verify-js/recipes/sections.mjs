// The sections fill the window one at a time: the number keys switch them, nothing overflows at a
// laptop's 1366 x 768 or a large screen's 2000 x 1000, and the layout is kept across a reload.
export const fixture = 'two_boards';

const KEYS = [['1', 'schematic'], ['2', 'power'], ['3', 'system'], ['4', 'bom'], ['5', 'pcb'], ['6', 'build'], ['7', 'source']];

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
      expect(box.width >= w - 2, `${id} fills the window width (${Math.round(box.width)} of ${w})`);
      expect(!(await overflows()), `${id}: nothing overflows the page`);
      await shot(`${id}-${w}`);
    }
  }

  step('Alt+number works in a text field, plain numbers do not');
  await page.keyboard.press('7');
  await page.locator('.tree-filter').click();
  await page.keyboard.press('3');
  expect((await active()).join() === 'source', 'the 3 was typed into the filter');
  await page.keyboard.press('Alt+1');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  expect(true, 'Alt+1 opens the schematic');

  step('the section is kept across a reload');
  await page.keyboard.press('4');
  await page.reload();
  await page.waitForSelector('.section.sec-bom:not(.away)');
  expect((await active()).join() === 'bom', 'the BOM again');
}
