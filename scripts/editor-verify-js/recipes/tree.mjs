// The project tree: top-level files with their boards and systems, modules, the manifest
// and the read-only standard library.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await section('source');
  step('read the tree');
  // The tree arrives a moment after the page (the driver waits for `.app` only).
  await page.waitForSelector('.sidebar .row.file');
  const tree = await page.locator('.sidebar').innerText();
  expect(tree.includes('verify'), 'the project name');
  expect(/TOP LEVEL/i.test(tree) && tree.includes('main.nsm'), 'the top-level file');
  expect(tree.includes('PowerBoard') && tree.includes('TwoBoards'), 'the boards and the system inside it');
  expect(tree.includes('power.nsm'), 'the module under src/');
  expect(tree.includes('netsmith.toml'), 'the manifest');

  step('open the standard library and a file in it');
  await page.locator('.heading', { hasText: /standard library/i }).click();
  await page.locator('.row.file', { hasText: 'passive.nsm' }).click();
  await page.waitForSelector('.tab.active:has-text("passive.nsm")');
  await page.waitForFunction(() => document.querySelector('.cm-content')?.innerText.includes('part Resistor'));
  expect(true, 'the library source in the editor');
  await page.locator('.cm-content').click();
  await page.keyboard.type('x');
  expect(!(await page.locator('.cm-content').innerText()).startsWith('x'), 'the library file refusing edits');
  await shot('std-library');

  step('click a board in the tree: the source goes to it and the views show it');
  await page.locator('.row.def', { hasText: 'LedBoard' }).click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('board LedBoard'));
  expect((await page.locator('.boardpick select').inputValue()) === 'TwoBoards/led', 'LedBoard chosen for the views');
  await shot('board-chosen');
}
