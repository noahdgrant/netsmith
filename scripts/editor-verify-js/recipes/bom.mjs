// The bill of materials, grouped as the fab BOM is, with the LCSC number and library of each part. The standard catalogs ship no price or stock, so
// the lines have none.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  step('open the BOM tab');
  await page.getByText('BOM', { exact: true }).click();
  await page.waitForSelector('table.grid');
  const text = await page.locator('table.grid').innerText();
  expect(text.includes('C2286') && text.includes('C25076'), 'the LCSC numbers of the LED and the resistor');
  expect(text.includes('Basic'), 'the library of each part');
  // The header is soldered by hand and has an LCSC number (the `header` catalog), so it is listed
  // under its own heading (the page shows it in capitals) with the number.
  expect(/soldered by hand/i.test(text) && text.includes('C42431841'), 'the header listed as soldered by hand');
  expect(!/\$0\.0/.test(await page.locator('.bom').innerText()), 'no price: the standard catalogs ship none');
  await shot('bom');
}
