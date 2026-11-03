// The power tree: rails as cards with voltage, loads and the budget against the supply.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  step('open the Power tree tab');
  await page.getByText('Power tree', { exact: true }).click();
  await page.waitForSelector('.card.power');
  const cards = await page.locator('.card.power').count();
  expect(cards >= 3, `a card for each rail net of the system (${cards})`);
  const text = await page.locator('.graph-host').innerText();
  expect(text.includes('3V3') && text.includes('VIN'), 'the VIN and 3V3 nets');
  expect(/of 100mA/.test(text), 'the regulator limit of 100mA as the capacity');
  await shot('power-tree');

  step('click a load line: the editor goes to its source');
  await page.locator('.card.power .line.link', { hasText: 'stage.vin' }).first().click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.vin'));
  expect(true, 'the cursor on the rail declaration');
}
