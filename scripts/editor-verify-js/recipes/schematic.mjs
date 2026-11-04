// The schematic section: click a part in the drawing and the source opens at its declaration
// (Back returns); put the cursor on a declaration and the part is marked in the drawing.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await page.waitForSelector('.schematic');
  step('pick the LED board, whose parts are plain');
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await page.waitForSelector('.schematic[data-board="LedBoard"]');
  await shot('led-board');

  step('click the resistor in the drawing: the Source section opens at its declaration');
  await page.locator('.schematic .part[data-ref="R1"]').click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.r1'));
  const line = await page.locator('.cm-activeLine').innerText();
  expect(line.includes('.r1: Resistor'), `the cursor on the line that declares R1 (${line.trim()})`);
  expect((await page.locator('.statusbar').innerText()).match(/Ln (\d+)/) !== null, 'the status bar shows the line');
  expect((await page.locator('.back-btn').innerText()).includes('Schematic'), 'a Back button that names the schematic');
  await shot('clicked-r1');

  step('Escape goes back to the schematic');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  expect((await page.locator('.back-btn').count()) === 0, 'the Back button is gone');

  step('put the cursor on the LED declaration in the source');
  await section('source');
  await page.locator('.cm-line', { hasText: '.led: Led' }).first().click();
  await section('schematic');
  await page.waitForSelector('.schematic .part.mark[data-ref="D1"]');
  expect(true, 'D1 marked in the drawing');
  await shot('cursor-marks-d1');

  step('open the details and pick a part in the list: the inspector shows its pins');
  if (!(await page.locator('table.parts').count())) await page.getByRole('button', { name: 'Details' }).click();
  await page.locator('table.parts tr', { has: page.locator('td.ref', { hasText: /^D1$/ }) }).click();
  // Picking a part from the list goes to its source too; Back returns, with D1 still picked.
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.inspector dd:has-text("Led")');
  expect(true, 'the inspector shows D1 as a Led');
}
