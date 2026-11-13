// The BOM's dates and stale mark, and "Refresh prices" (ADR 0039), against stand-ins for the
// LCSC endpoint (JLCPCB is not asked). No network is used.
import { whenIdle } from '../settle.mjs';
export const fixture = 'catalog_refresh';

export async function run({ page, shot, step, expect, section }) {
  step('open the BOM tab');
  await section('bom');
  await page.waitForSelector('table.grid');
  const resistor = page.locator('table.grid:not(.refresh-rows) tbody tr', { hasText: 'C9999999' });
  expect(await resistor.locator('.checked.stale').count() === 1 && await resistor.locator('.stale-dot').count() === 1, 'the resistor, checked 2026-06-01, is marked stale');
  expect((await resistor.locator('.checked').innerText()).includes('2026-06-01'), 'its date is shown');
  expect((await page.locator('.refresh-bar').innerText()).includes('read more than 90 days ago'), 'the bar counts the stale lines');
  await shot('bom-stale');

  step('preview a check');
  await page.getByLabel('preview only').check();
  await page.getByRole('button', { name: 'Check parts against LCSC' }).click();
  await page.waitForSelector('.refresh-result');
  let text = await page.locator('.refresh-result').innerText();
  expect(text.includes('preview, nothing written'), 'a preview says it wrote nothing');
  expect(text.includes('C9999999') && text.includes('LCSC lists stock 0 where the row has stock 42') && text.includes('check JLCPCB'), `the stock difference is reported as information: ${text}`);
  expect(text.includes('C9999998') && text.includes('MISSING'), 'a part the endpoint does not list is MISSING');
  expect(text.includes('1 verified'), `the verified row is counted: ${text}`);
  expect(text.includes('JLCPCB data is not fetched') && text.includes('asks LCSC only'), `the result says JLCPCB is not fetched, and what is asked: ${text}`);
  expect(text.includes('Never written: price, stock, price_breaks, library, date'), `the result says what is never written: ${text}`);
  expect(text.includes('terms of use have not been read') && text.includes('future work'), 'the unread terms and the future work are said');
  const button = page.getByRole('button', { name: 'Check parts against LCSC' });
  expect((await button.getAttribute('title')).includes('never writes a price, stock or library'), 'the tooltip says what is never written');
  expect((await resistor.locator('.checked').innerText()).includes('2026-06-01'), 'the BOM is unchanged');
  await shot('check-preview');

  step('check for real');
  await page.getByLabel('preview only').uncheck();
  await page.getByRole('button', { name: 'Check parts against LCSC' }).click();
  await page.waitForSelector('.refresh-result .chip:has-text("checked dates written")');
  // The price, stock and their date are the hand-entered ones: LCSC's price 0.0205 and stock 0
  // were not written, and the row is as stale as it was.
  await whenIdle(page);
  const row = await resistor.innerText();
  expect(row.includes('$0.012') && row.includes('42') && row.includes('2026-06-01'), `the BOM still shows the hand-entered numbers: ${row}`);
  expect(!row.includes('0.0205') && (await resistor.locator('.checked.stale').count()) === 1, 'LCSC\'s price and stock were not written, so the row is still stale');
  await shot('check-done');
}
