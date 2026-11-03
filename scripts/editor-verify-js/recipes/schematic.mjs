// Click a part in the drawing: the editor jumps to its declaration. Put the cursor on a
// declaration: the part is marked in the drawing.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic');
  step('pick the LED board, whose parts are plain');
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await page.waitForSelector('.schematic[data-board="LedBoard"]');
  await shot('led-board');

  step('click the resistor in the drawing');
  await page.locator('.schematic .part[data-ref="R1"]').click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.r1'));
  const line = await page.locator('.cm-activeLine').innerText();
  expect(line.includes('.r1: Resistor'), `the cursor on the line that declares R1 (${line.trim()})`);
  expect((await page.locator('.statusbar').innerText()).match(/Ln (\d+)/) !== null, 'the status bar shows the line');
  await shot('clicked-r1');

  step('put the cursor on the LED declaration in the source');
  await page.locator('.cm-line', { hasText: '.led: Led' }).first().click();
  await page.waitForSelector('.schematic .part.mark[data-ref="D1"]');
  expect(true, 'D1 marked in the drawing');
  await shot('cursor-marks-d1');

  step('pick a part in the list: the inspector shows its pins');
  await page.locator('table.parts tr', { has: page.locator('td.ref', { hasText: /^D1$/ }) }).click();
  await page.waitForSelector('.inspector dd:has-text("Led")');
  expect(true, 'the inspector shows D1 as a Led');
}
