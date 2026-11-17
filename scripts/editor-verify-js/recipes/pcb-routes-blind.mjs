// The via tool on a board of an HDI process (issue 1071): the toolbar offers the vias the process
// makes (the through via first, then the blind vias of each face and the buried via of the core);
// a via placed with a blind span picked is that blind via of the process, of the laser's size,
// written to the routes file with its two layers as `netsmith routes add-via --layers top,inner1`
// writes it, and standing in a resistor's pad (the process fills its blind vias by plating).
import fs from 'node:fs';
import path from 'node:path';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, screenAt } from '../pcb-helpers.mjs';

export const fixture = 'hdi_vias';
const BOARD = 'HdiVias';

export async function run(tools) {
  const { page, shot, step, expect, project, url } = tools;
  const file = path.join(project, 'src/main.routes.toml');
  // Every run starts with no routes file, whatever ran before.
  fs.rmSync(file, { force: true });
  await page.goto(url);
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  await page.selectOption('[data-testid=grid]', '0');
  await page.getByRole('button', { name: 'Fit' }).click();
  await whenIdle(page);

  let s = await scene(page, BOARD);
  const spans = s.viaOffers.map((o) => o.layers.join(','));
  expect(JSON.stringify(spans) === JSON.stringify(['top,inner1', 'inner4,bottom', 'inner1,inner4', '']), `the process's vias (${spans})`);

  step('V takes the via tool; its picker offers the through via, then the blind and buried vias');
  await page.keyboard.press('v');
  await whenIdle(page);
  await page.waitForSelector('[data-testid=via-options]');
  const options = await page.locator('[data-testid=via-span] option').evaluateAll((os) => os.map((o) => [o.value, o.textContent]));
  expect(
    JSON.stringify(options.map((o) => o[0])) === JSON.stringify(['', 'top,inner1', 'inner4,bottom', 'inner1,inner4']),
    `the picker lists ${JSON.stringify(options)}`,
  );
  expect(/blind via top to inner1, 0\.25 mm/.test(options[1][1]), `a blind via is named by its layers and size (${options[1][1]})`);
  expect(/buried via inner1 to inner4/.test(options[3][1]), `the buried via is named so (${options[3][1]})`);
  await page.selectOption('[data-testid=via-span]', 'top,inner1');
  await whenIdle(page);

  step('a click on a pad of the first resistor puts the blind via in the pad');
  const r1 = s.parts.find((p) => p.path.endsWith('r1'));
  expect(r1, 'the first resistor is on the board');
  const pad = r1.pads.find((p) => p.net);
  const at = await screenAt(page, pad.x, pad.y);
  await page.mouse.move(at.x, at.y);
  await whenIdle(page);
  await shot('blind-ghost');
  const stamp = s.routesStamp;
  await page.mouse.click(at.x, at.y);
  s = await waitFor('the via to be written', async () => {
    const now = await scene(page, BOARD);
    return now.routesStamp !== stamp ? now : null;
  });
  await whenIdle(page);
  const record = s.routes.find((n) => n.net === pad.net);
  expect(record && record.vias.length === 1, `the pad's net has a record with the via (${JSON.stringify(record?.vias)})`);
  const v = record.vias[0];
  expect(
    JSON.stringify(v.layers) === JSON.stringify(['top', 'inner1']) && v.diameter === 0.25 && v.drill === 0.1,
    `it is the laser's blind via from top to inner1 (${JSON.stringify(v)})`,
  );
  expect(Math.abs(v.at.x - pad.x) < 1e-6 && Math.abs(v.at.y - pad.y) < 1e-6, 'it stands at the pad\'s centre');
  expect(/layers = \["top", "inner1"\]/.test(fs.readFileSync(file, 'utf8')), 'the file says its layers, as `routes add-via --layers` writes them');
  await shot('blind-placed');

  step('the side panel names it a blind via');
  await page.locator('.side-tabs button', { hasText: 'Routes' }).click();
  await page.waitForSelector(`[data-route-row="${pad.net}/via/1"]`);
  await page.locator(`[data-route-row="${pad.net}/via/1"]`).click();
  await whenIdle(page);
  const kind = await page.locator('[data-testid=route-inspector]').textContent();
  expect(/blind via top to inner1/.test(kind), `the inspector says what it is (${kind})`);
  await shot('blind-inspector');
}
