// The bill of materials, grouped as the fab BOM is, with the LCSC number and library of each part. The standard catalogs ship no price or stock
// and the fixture has no [supply] table, so the price and stock are unknown (ADR 0047), not 0 and not "no date".
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
  const bom = await page.locator('.bom').innerText();
  expect(!/\$0\.0/.test(bom), 'no price: the standard catalogs ship none');
  expect(text.includes('unknown') && !/no date/i.test(bom), 'price and stock say unknown, not "no date"');
  expect(await page.locator('.stale-dot').count() === 0, 'an unknown price is not marked stale');
  await shot('bom');
}
