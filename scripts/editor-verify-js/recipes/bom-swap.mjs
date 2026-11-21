// The swap of a generic part no catalog row stands for, in the bill of materials. The fixture's resistor is 249 ohm (E96), which no
// row of the standard catalog has, so its line is unresolved. Swap lists the three nearest rows with what the board's checks say with
// each in its place, and the edit each would make; Use this writes the row's value over the stated one in the source, the page takes
// the new analysis, and the line has an LCSC number. At the end the source is put back.
import fs from 'node:fs';
import path from 'node:path';

import { analysedSince, pageVersion, whenIdle } from '../settle.mjs';

export const fixture = 'swap';

export async function run({ page, shot, step, expect, section, project }) {
  const main = path.join(project, 'src/main.nsm');
  const original = fs.readFileSync(main, 'utf8');
  const r1 = async () => {
    const d = await page.evaluate(async () => (await fetch('/api/board?name=LedExample', { cache: 'no-store' })).json());
    return d.parts.find((p) => p.path === 'r1').refdes;
  };
  const refdes = await r1();

  step('open the BOM: the resistor is unresolved and has a Swap button');
  await section('bom');
  await page.waitForSelector('table.grid');
  const line = page.locator(`tr.dr[data-hv-ref="${refdes}"]`);
  expect((await line.getAttribute('data-sourcing')) === 'ambiguous', 'the resistor line is unresolved');
  expect((await page.locator('.swap-btn').count()) === 1, 'one Swap button, on the unresolved line only');

  step('Swap: the nearest rows, each with its check and its edit');
  await line.locator('.swap-btn').click();
  const panel = page.locator('[data-testid="swap-panel"]');
  await panel.locator('table.swap-rows tr').first().waitFor();
  await whenIdle(page);
  const rows = panel.locator('table.swap-rows tr');
  expect((await rows.count()) === 3, 'three nearest rows');
  const first = await rows.first().innerText();
  expect(first.includes('240Ω 1% 0402 C25094'), `the nearest row first (${first})`);
  expect(first.includes('Passes'), 'its check passes');
  expect(first.includes('249ohm +/- 1% → 240ohm +/- 1%'), 'the edit it would make');
  expect((await panel.locator('tr[data-check="passes"]').count()) === 3, 'every alternative passes the checks');
  expect((await panel.innerText()).includes(`src/main.nsm:`), 'where the value is stated');
  await shot('swap-panel');

  step('Use this: the value is written over the stated one and the part resolves');
  const before = await pageVersion(page);
  await rows.first().getByRole('button', { name: 'Use this' }).click();
  await analysedSince(page, before);
  expect(fs.readFileSync(main, 'utf8') === original.replace('249ohm +/- 1%', '240ohm +/- 1%'), 'only the value changed in the file');
  await page.waitForSelector(`tr.dr[data-hv-ref="${refdes}"]:not([data-sourcing="ambiguous"])`);
  await whenIdle(page);
  expect((await panel.count()) === 0, 'the panel closed');
  expect((await page.locator('.swap-btn').count()) === 0, 'nothing left to swap');
  const lcsc = await page.locator(`tr.dr[data-hv-ref="${refdes}"] td.sku`).innerText();
  expect(/^C\d+$/.test(lcsc.trim()), `the line has an LCSC number (${lcsc})`);
  await shot('swapped');

  step('put the source back');
  const after = await pageVersion(page);
  fs.writeFileSync(main, original);
  await analysedSince(page, after);
}
