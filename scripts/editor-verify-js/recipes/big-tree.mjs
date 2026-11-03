// The project tree of a big project: modules grouped by folder and a filter. (A folder of
// only parts is closed until asked for; the tree's unit tests cover that.)
export const fixture = 'big';

export async function run({ page, shot, step, expect }) {
  step('modules are grouped by folder');
  const tree = await page.locator('.sidebar').innerText();
  expect(/power/.test(tree) && /io/.test(tree), 'the folders power and io under src/sheets');
  expect((await page.locator('.row.folder').count()) >= 2, 'a folder row for each of them');
  await shot('tree');

  step('the filter finds a file or a definition and opens the folders it is in');
  await page.fill('.tree-filter', 'PullBank');
  await page.waitForSelector('.row.file:has-text("pull_bank.nsm")');
  expect((await page.locator('.row.file').count()) === 1, 'only the file with the definition');
  await page.fill('.tree-filter', 'zzz');
  expect((await page.locator('.row.file').count()) === 0, 'nothing for what is not there');
  await page.fill('.tree-filter', '');
}
