// Parts that are not fitted until they are tuned (issue 1273) are listed under their own heading of the bill of materials,
// as the DNP lines of bom.csv: the range in the value, no LCSC number, and not among the parts the board house places.
export const fixture = 'not_fitted';

export async function run({ page, shot, step, expect, section }) {
  step('open the BOM tab');
  await section('bom');
  await page.waitForSelector('[data-testid="bom-not-fitted"]');
  const rows = await page.locator('[data-testid="bom-not-fitted-row"]').allInnerTexts();
  expect(rows.length === 2, `two DNP lines, got ${rows.length}`);
  expect(rows.some((r) => r.includes('DNP (tune 1pF..10pF)')), 'the part with no value shows its range');
  expect(rows.some((r) => r.includes('DNP 100n (tune 10nF..1uF)')), 'the part with a value shows it and its range');
  const placed = await page.locator('table.grid tbody:not([data-testid="bom-not-fitted"])').innerText();
  expect(!placed.includes('DNP'), 'no DNP line among the parts the board house places');
  expect(placed.includes('C11702'), 'the fitted resistors keep their number');
  await shot('bom-not-fitted');
}
