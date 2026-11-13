// Screenshots of each sheet of the invented bench board at 1366 x 768 and 2000 x 1000, for the
// pull request's before and after pictures: `SHOT_LABEL=after scripts/editor-verify drive
// schematic-shots`. The files are `<label>-<sheet>-<width>.png` in the evidence folder.
import { whenIdle } from '../settle.mjs';
import { openSheet, sheetNames } from '../schematic-helpers.mjs';

export const fixture = 'schematic_bench';

export const SHEETS = ['adc', 'mcu', 'buck', 'modem', 'ldo', 'obd', 'ten'];

export async function run({ page, shot, step, expect }) {
  const label = process.env.SHOT_LABEL ?? 'shot';
  await page.waitForSelector('.schematic');
  for (const [w, h] of [[1366, 768], [2000, 1000]]) {
    await page.setViewportSize({ width: w, height: h });
    step(`every module at ${w} x ${h}`);
    for (const name of SHEETS) {
      // A module the old drawing cut in two has two tabs, `modem (1/2)` and `modem (2/2)`.
      const tabs = (await sheetNames(page)).filter((t) => t === name || t.startsWith(`${name} (`));
      for (const [i, tab] of tabs.entries()) {
        await openSheet(page, tab);
        await whenIdle(page);
        // The panel with the parts list is off, so the drawing has the width.
        const details = page.getByRole('button', { name: 'Details' });
        if ((await details.getAttribute('aria-pressed')) === 'true') await details.click();
        await page.keyboard.press('f');
        await whenIdle(page);
        await shot(`${label}-${name}${tabs.length > 1 ? `-part${i + 1}` : ''}-${w}`);
      }
    }
  }
  expect(true, 'the pictures were taken');
}
