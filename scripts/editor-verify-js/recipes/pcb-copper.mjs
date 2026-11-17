// The copper areas in the Summary tab of the PCB view (issue 1085): which layers carry a plane, a pour or
// tracks, the zones of the routes file counted apart from the process's pours, and the teardrop fills, counted
// apart from both and only once the board has been routed. The numbers are the server's
// (`GET /api/pcb/copper`, what `netsmith board copper` prints), read from the source and the routes file with
// no placing and no routing. A zone added from a terminal shows with no reload, and takes the layer's pour
// out of the drawing.
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { guarded, waitFor, whenIdle } from '../settle.mjs';
import { scene } from '../pcb-helpers.mjs';

export const fixture = 'routed_pours';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));
const exec = promisify(execFile);

export async function run(tools) {
  const { page, shot, step, expect, project, url, binary } = tools;
  const file = path.join(project, 'src/main.routes.toml');
  const stampOf = () => {
    const on = fs.statSync(file, { bigint: true });
    return `${on.mtimeNs}-${on.size}`;
  };
  // Every run starts from the fixture's file, whatever ran before: two zones on the top layer, by hand.
  fs.copyFileSync(path.join(here, '../fixtures/routed_pours.routes.toml'), file);
  await page.goto(url);
  await page.waitForSelector('.app');
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === stampOf());
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
  await page.waitForSelector('[data-testid=summary-copper-layer]');
  await whenIdle(page);
  const layers = () =>
    page.locator('[data-testid=summary-copper-layer]').evaluateAll((all) =>
      all.map((e) => ({ layer: e.getAttribute('data-layer'), kind: e.getAttribute('data-kind'), text: e.textContent })),
    );
  const teardrops = () => page.locator('[data-testid=summary-copper-teardrops]');

  step('every copper layer from the top, with its use: signal, two planes, a pour');
  let rows = await layers();
  expect(rows.map((r) => `${r.layer}:${r.kind}`).join() === 'top:signal,inner1:plane,inner2:plane,bottom:pour', `the layers are ${rows.map((r) => `${r.layer}:${r.kind}`)}`);
  expect(rows[1].text.includes('plane on GND') && rows[2].text.includes('plane on VCC'), 'the planes name their nets');
  expect(rows[3].text.includes('pour on GND') && rows[3].text.includes('stitch pitch'), `the pour names its net and its fields (${rows[3].text})`);
  await shot('copper-areas');

  step('the routes file\'s zones are counted apart from the pours, and the top layer says it has two');
  expect(rows[0].text.includes('2 hand zones'), `the top layer has two hand zones (${rows[0].text})`);
  expect((await page.locator('[data-testid=summary-copper-zones]').innerText()).includes(': 2'), 'two hand zones in all');
  expect((await page.locator('[data-testid=summary-copper-zone]').count()) === 2, 'a line for each zone');
  expect((await page.locator('[data-testid=summary-copper-zone]').first().innerText()).startsWith('GND on top:'), 'a zone names its net and layer');

  step('the teardrop fills are not counted for a board that is not routed');
  expect((await teardrops().getAttribute('data-counted')) === 'false' && (await teardrops().innerText()).includes('not counted'), 'said as not counted');

  step('the page shows what the server says');
  const server = await page.evaluate(async (b) => (await fetch(`/api/pcb/copper?board=${encodeURIComponent(b)}`)).json(), BOARD);
  expect(server.layers.map((l) => `${l.layer}:${l.kind}`).join() === rows.map((r) => `${r.layer}:${r.kind}`).join(), 'the same layers and uses');
  expect(server.zones.length === 2 && server.teardrops.counted === false, 'the same zones and teardrops');

  step('a zone added from a terminal on the pour\'s layer shows with no reload, and the pour is said to be replaced');
  await page.evaluate(() => (window.__notReloaded = true));
  const before = (await scene(page, BOARD)).routesStamp;
  const added = await guarded('`netsmith routes add-zone` to exit', exec(binary, ['routes', 'add-zone', '--net', 'GND', '--layer', 'bottom', '--rect', '1,1,6,4'], { cwd: project }).catch((e) => e));
  expect(!added.code, `add-zone exits 0 (${added.stderr ?? ''})`);
  await waitFor('the server to have read the zone', async () => {
    const s = await scene(page, BOARD);
    return s.routesStamp !== before && s.routesStamp === stampOf();
  });
  await page.waitForSelector('[data-testid=summary-copper-layer][data-layer=bottom]:has-text("not drawn")');
  await whenIdle(page);
  rows = await layers();
  expect(rows[3].kind === 'pour' && rows[3].text.includes('not drawn: 1 zone of the routes file replaces it'), `the bottom pour is not drawn (${rows[3].text})`);
  expect((await page.locator('[data-testid=summary-copper-zone]').count()) === 3, 'three hand zones now');
  expect((await page.locator('[data-testid=summary-copper-zones]').innerText()).includes(': 3'), 'the count says three');
  expect(await page.evaluate(() => !!window.__notReloaded), 'the page was not reloaded');
  await shot('copper-zone-replaces-pour');

  step('route the board: the teardrop fills are counted, apart from the pours and the zones');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  await page.waitForSelector('[data-testid=summary-copper-teardrops][data-counted=true]');
  await whenIdle(page);
  const line = await teardrops().innerText();
  expect(/Teardrop fills: \d+/.test(line), `the count (${line})`);
  expect((await page.locator('[data-testid=summary-copper-zone]').count()) === 3, 'the hand zones are still three: teardrops are no zone of the routes file');
  expect((await layers()).map((r) => r.kind).join() === 'signal,plane,plane,pour', 'and no layer became a pour');
  await shot('copper-teardrops');
}
