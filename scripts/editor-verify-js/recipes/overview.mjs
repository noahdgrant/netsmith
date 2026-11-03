// The page opens with the project tree, the first top-level file and the schematic.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  step('open the editor');
  await page.waitForSelector('.cm-content');
  await page.waitForSelector('.schematic');
  expect((await page.locator('.statusbar .counts').innerText()).includes('0'), 'a status bar with the error and warning counts');
  await shot('first-look');
}
