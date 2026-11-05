// The schematic section: click a part in the drawing and the source opens at its declaration
// (Back returns); put the cursor on a declaration and the part is marked in the drawing.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await page.waitForSelector('.schematic');
  step('pick the LED board, whose parts are plain');
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await page.waitForSelector('.schematic[data-board="LedBoard"]');
  await shot('led-board');

  step('click the resistor: it is selected and the nets dim; a double-click opens the Source at its declaration');
  await page.locator('.schematic .part[data-ref="R1"]').click();
  await page.waitForSelector('.sel-chip');
  expect((await page.locator('.sel-chip').innerText()).includes('R1'), 'the selection chip names R1');
  await page.locator('.schematic .part[data-ref="R1"]').dblclick();
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.r1'));
  const line = await page.locator('.cm-activeLine').innerText();
  expect(line.includes('.r1: Resistor'), `the cursor on the line that declares R1 (${line.trim()})`);
  const marks = await page.locator('.cm-selectionMatch').count();
  expect(marks < 5, `a click-through does not mark every occurrence of what it selected (${marks} marks)`);
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

  step('open the details and pick a part in the list: it is selected, the inspector shows it');
  if (!(await page.locator('table.parts').count())) await page.getByRole('button', { name: 'Details' }).click();
  await page.locator('table.parts tr', { has: page.locator('td.ref', { hasText: /^D1$/ }) }).click();
  await page.waitForSelector('.sinspector .title:has-text("D1")');
  expect((await page.locator('.sinspector .title').innerText()).includes('Led'), 'the inspector shows D1 as a Led');
  expect((await page.locator('.sinspector table.parts tr.sel').count()) === 1, 'the part list highlights it');
  await page.keyboard.press('Escape');
}
