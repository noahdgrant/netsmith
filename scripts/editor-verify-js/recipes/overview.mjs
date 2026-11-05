// The page opens on the schematic, with the sections along the top, and the source one click
// (or the key 1) away.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  step('open the editor');
  await page.waitForSelector('.schematic');
  const tabs = await page.locator('.pane-strip .tab').allInnerTexts();
  expect(tabs.join(',') === 'Source,Schematic,Power tree,System,BOM,PCB,Build', `the sections in a row (${tabs.join(', ')})`);
  expect((await page.locator('.statusbar .counts').innerText()).includes('0'), 'a status bar with the error and warning counts');
  expect((await page.locator('.topbar').innerText()).includes('Write KiCad files'), 'the actions are in the top bar, apart from the sections');
  await shot('first-look');

  step('the Source section: the file tree and the code');
  await page.keyboard.press('1');
  await page.waitForSelector('.cm-content');
  await page.waitForSelector('.sidebar .ft-file');
  await shot('source');
}
