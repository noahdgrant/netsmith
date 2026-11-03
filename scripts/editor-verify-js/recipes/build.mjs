// The build panel: the schematic, then the fab package with the design rules check.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  step('click Build in the top bar');
  await page.locator('.topbar button', { hasText: /^Build$/ }).click();
  await page.waitForSelector('.build .chip:has-text("built")');
  expect((await page.locator('.build').innerText()).includes('wrote'), 'the schematic directory written');
  await shot('built');

  step('click Fab in the top bar');
  await page.locator('.topbar button', { hasText: /^Fab$/ }).click();
  await page.waitForSelector('.build .chip:has-text("package made"), .build .chip:has-text("stopped")', { timeout: 180000 });
  const text = await page.locator('.build').innerText();
  expect(text.includes('design rules check: passed'), 'the design rules check passed');
  expect(text.includes('fab-notes.txt'), 'fab-notes.txt shown');
  expect((await page.locator('.downloads a').count()) >= 3, 'download links for the package');
  await shot('fab');
}
