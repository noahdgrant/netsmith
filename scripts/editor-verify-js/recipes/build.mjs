// The build panel: the schematic, then the fab package with the design rules check.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  step('click Write KiCad files in the top bar: the Build section opens and shows what was written');
  await page.locator('.topbar button', { hasText: 'Write KiCad files' }).click();
  await page.waitForSelector('.topbar button.running');
  expect(true, 'the button shows that it is running');
  await page.waitForSelector('.build .chip:has-text("built")');
  expect((await page.locator('.build').innerText()).includes('wrote'), 'the schematic directory written');
  await shot('built');

  step('click Make fab files in the top bar');
  await page.locator('.topbar button', { hasText: 'Make fab files' }).click();
  await page.waitForSelector('.build .chip:has-text("package made"), .build .chip:has-text("stopped")');
  const text = await page.locator('.build').innerText();
  expect(text.includes('design rules check: passed'), 'the design rules check passed');
  expect(text.includes('fab-notes.txt'), 'fab-notes.txt shown');
  expect((await page.locator('.downloads a').count()) >= 3, 'download links for the package');
  await shot('fab');
  expect((await page.locator('.topbar button.running').count()) === 0, 'the buttons are idle again');
}
