// A project with no board yet: every view that needs a board says "No boards yet", names the file to write it in, and the button opens it.
import { gate, whenIdle } from '../settle.mjs';
export const fixture = 'empty';

export async function run({ page, shot, step, expect, section, url }) {
  await page.setViewportSize({ width: 1280, height: 800 });
  for (const id of ['schematic', 'power', 'system', 'bom', 'pcb']) {
    step(`${id}: the empty state`);
    await section(id);
    await page.waitForSelector(`.sec-${id} [data-testid="no-board"]`);
    const text = await page.locator(`.sec-${id} [data-testid="no-board"]`).innerText();
    expect(text.includes('No boards yet') && text.includes('src/main.nsm'), `${id} says No boards yet and names src/main.nsm`);
    if (id === 'schematic') await shot('schematic');
  }
  step('the button opens src/main.nsm in the Source');
  await section('bom');
  await page.click('.sec-bom [data-testid="open-entry"]');
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForSelector('.tabbar.files .tab.active');
  expect((await page.locator('.tabbar.files .tab.active').innerText()).includes('main.nsm'), 'the file tab main.nsm is active');
  await shot('source');

  // The project has no board and is not in a git repository: the comparison has nothing to say, and says so.
  step('Compare (key 8) in a project that is not in a git repository: the reason, no picker, no chip, no switch that does anything');
  await section('bom');
  await page.keyboard.press('8');
  await page.waitForSelector('.section.sec-compare:not(.away)');
  await page.waitForSelector('[data-testid="cmp-no-git"]');
  expect(/not in a git repository|git is not installed/.test(await page.locator('[data-testid="cmp-no-git"]').innerText()), 'it says the project is not in a repository');
  expect((await page.locator('[data-testid="ref-button"]').count()) === 0 && (await page.locator('[data-testid="compare-chip"]').count()) === 0, 'no ref picker and no chip');
  await shot('compare-no-git');
  step('the palette: Compare with a ref… opens the section and leaves no picker waiting; the marks switch says there is nothing to compare with');
  await section('bom');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('compare with a ref');
  await whenIdle(page);
  await page.keyboard.press('Enter');
  await page.waitForSelector('.section.sec-compare:not(.away) [data-testid="cmp-no-git"]');
  expect((await page.locator('[data-testid="ref-popover"]').count()) === 0, 'no picker is open');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('show changes vs');
  await whenIdle(page);
  await page.keyboard.press('Enter');
  await page.waitForSelector('.toast');
  expect(/not in a git repository/.test(await page.locator('.toast').first().innerText()), 'a toast says there is nothing to compare with');
  expect(!(await page.evaluate(() => document.documentElement.classList.contains('cmp-on'))), 'and no mark is on');

  step('the first-run splash is the same in a project that is not a repository: the mark, the words, then the page');
  const p2 = await page.context().newPage();
  // The project is held back until the step has looked at the splash: the splash is there for as long as that takes, on any machine.
  const arrives = gate();
  await p2.route('**/api/project', async (r) => {
    await arrives.opened;
    await r.continue();
  });
  const nav = p2.goto(url);
  await p2.waitForSelector('.splash');
  expect(((await p2.locator('.splash').innerText()) || '').includes('Reading the project'), 'it says Reading the project');
  arrives.open();
  await nav;
  await p2.waitForSelector('.splash', { state: 'detached' });
  await p2.waitForSelector('.section:not(.away)');
  expect(true, 'the page is there');
  await p2.close();
}
