// The routes file in the PCB view (epic #599): its copper is marked over the board (solid on a
// locked net, dashed on one the router may add to, a padlock on a locked shape, a ring round a via),
// the side panel lists its records, a shape is picked by a click, locked and unlocked with L and the
// inspector, and deleted with Delete, which is refused with the reason while it is locked. A record
// whose pads moved is a warning in the findings and in the problems dock. Every change is the routes
// file's, made as `netsmith routes` makes it.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, screenAt } from '../pcb-helpers.mjs';

export const fixture = 'routed_led';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));

export async function run(tools) {
  const { page, shot, step, expect, project, url, allowConsole } = tools;
  // The refused delete is a 422 on purpose, which the browser logs.
  allowConsole(/status of 422 .*\/api\/pcb\/routes/);
  const file = path.join(project, 'src/main.routes.toml');
  const text = () => fs.readFileSync(file, 'utf8');
  // Every run starts from the fixture's file, whatever ran before.
  fs.copyFileSync(path.join(here, '../fixtures/routed_led.routes.toml'), file);
  const stampOf = () => {
    const on = fs.statSync(file, { bigint: true });
    return `${on.mtimeNs}-${on.size}`;
  };
  await page.goto(url);
  await page.waitForSelector('.app');
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === stampOf());
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  await page.waitForSelector('[data-testid=fixed-copper] .fixed-shape');
  await whenIdle(page);

  step('the fixed copper is marked: a solid line on the locked net, dashed on the open one, a padlock on the locked track');
  let s = await scene(page, BOARD);
  const gnd = s.routes.find((n) => n.net === 'GND');
  const anode = s.routes.find((n) => n.net === 'net-led.anode');
  expect(gnd?.lock && !anode?.lock, 'GND is locked, the anode net is not');
  expect((await page.locator('.fixed-net.net-locked[data-fixed-net="GND"] .fixed-shape').count()) === gnd.tracks.length + gnd.vias.length, 'every GND shape is marked as on a locked net');
  expect((await page.locator('.fixed-net:not(.net-locked)[data-fixed-net="net-led.anode"] .fixed-shape').count()) === anode.tracks.length, 'every anode track is marked as on an open net');
  expect((await page.locator('[data-route-net="net-led.anode"][data-route-track="2"] .lock-mark').count()) === 1, 'the locked track has a padlock');
  expect((await page.locator('.fixed-via[data-route-net="GND"]').count()) === 1, 'the via has a ring');
  const dash = await page.locator('[data-route-net="net-led.anode"][data-route-track="1"] .fixed-mark').evaluate((e) => getComputedStyle(e).strokeDasharray);
  const solid = await page.locator('[data-route-net="GND"][data-route-track="2"] .fixed-mark').evaluate((e) => getComputedStyle(e).strokeDasharray);
  expect(dash !== 'none' && solid === 'none', `dashed on the open net (${dash}), solid on the locked one (${solid})`);
  await page.getByRole('button', { name: 'Fit' }).click();
  await whenIdle(page);
  await shot('fixed-copper');
  // The marks are in the text's colour, which reads on the copper of a dark theme too.
  const strokeOf = () => page.locator('[data-route-net="GND"][data-route-track="2"] .fixed-mark').evaluate((e) => getComputedStyle(e).stroke);
  const light = await strokeOf();
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  const dark = await strokeOf();
  expect(dark !== light, `a mark takes the theme's colour (${light} in light, ${dark} in dark)`);
  await shot('fixed-copper-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('the Routes tab lists the records, numbered as `netsmith routes show` numbers them');
  await page.locator('.side-tabs button', { hasText: 'Routes' }).click();
  await page.waitForSelector('[data-testid=route-list]');
  const rows = await page.locator('[data-testid=route-list] .row').allInnerTexts();
  expect(rows.some((r) => r.includes('GND') && r.includes('6 tracks, 1 via') && r.includes('lock')), `GND's row (${rows[0]?.replace(/\n/g, ' ')})`);
  expect(rows.some((r) => r.includes('track 2') && r.includes('lock')), 'the locked track is marked in the list');
  await shot('routes-list');

  step('a click on a track on the board picks it; the inspector shows it');
  const t1 = anode.tracks[0];
  const at = await screenAt(page, (t1.from.x + t1.to.x) / 2, (t1.from.y + t1.to.y) / 2);
  await page.mouse.click(at.x, at.y);
  await page.waitForSelector('[data-testid=route-inspector]');
  expect((await page.locator('[data-testid=route-inspector]').innerText()).includes('track 1 of net net-led.anode'), 'the inspector names track 1 of the anode net');
  expect((await page.locator('[data-route-net="net-led.anode"][data-route-track="1"].picked').count()) === 1, 'the track is lit on the board');
  await shot('track-picked');

  step('L locks it: the file says so and a padlock appears');
  let stamp = s.routesStamp;
  await page.keyboard.press('l');
  s = await waitFor('the routes file to be written', async () => {
    const now = await scene(page, BOARD);
    return now.routesStamp !== stamp ? now : null;
  });
  await whenIdle(page);
  expect(s.routes.find((n) => n.net === 'net-led.anode').tracks[0].lock, 'the scene has it locked');
  expect(/\{ layer = "top", width = 0\.15, from = \[12\.4, 3\.8\], to = \[11\.6, 4\.6\], lock = true \}/.test(text()), 'the routes file has `lock = true` on it');
  expect((await page.locator('[data-route-net="net-led.anode"][data-route-track="1"] .lock-mark').count()) === 1, 'a padlock on the track');

  step('Delete is refused while it is locked: the reason is said and the file is unchanged');
  const before = text();
  await page.keyboard.press('Delete');
  await page.locator('.toast', { hasText: 'track 1 of net net-led.anode is locked' }).waitFor();
  await whenIdle(page);
  expect(text() === before, 'the routes file is as it was');
  await shot('delete-refused');

  step('unlocked from the inspector, Delete deletes it');
  stamp = s.routesStamp;
  await page.locator('[data-testid=route-lock]').click();
  s = await waitFor('the track to be unlocked', async () => {
    const now = await scene(page, BOARD);
    return now.routesStamp !== stamp ? now : null;
  });
  await whenIdle(page);
  expect(!s.routes.find((n) => n.net === 'net-led.anode').tracks[0].lock, 'unlocked');
  stamp = s.routesStamp;
  await page.keyboard.press('Delete');
  s = await waitFor('the track to be deleted', async () => {
    const now = await scene(page, BOARD);
    return now.routesStamp !== stamp ? now : null;
  });
  await whenIdle(page);
  expect(s.routes.find((n) => n.net === 'net-led.anode').tracks.length === anode.tracks.length - 1, 'the record has one track fewer');
  expect(!text().includes('from = [12.4, 3.8], to = [11.6, 4.6]'), 'and so has the file');
  await page.locator('.toast', { hasText: 'deleted track 1 of net net-led.anode' }).first().waitFor();

  step('the net is locked from the list: its marks turn solid');
  await page.locator('[data-route-row="net-led.anode"]').click();
  await page.waitForSelector('[data-testid=route-inspector]');
  stamp = s.routesStamp;
  await page.locator('[data-testid=route-lock]').click();
  s = await waitFor('the net to be locked', async () => {
    const now = await scene(page, BOARD);
    return now.routesStamp !== stamp ? now : null;
  });
  await whenIdle(page);
  expect(s.routes.find((n) => n.net === 'net-led.anode').lock, 'the net is locked');
  expect((await page.locator('.fixed-net.net-locked[data-fixed-net="net-led.anode"]').count()) === 1, 'its marks are those of a locked net');
  await page.keyboard.press('Escape');
  await whenIdle(page);
  await shot('net-locked');

  step('a record whose pads moved is a warning in the findings and the problems dock');
  fs.writeFileSync(file, text().replace(/(\[LedExample\.nets\.GND\]\npads = )"[0-9a-f]+"/, '$1"0000000000000000"'));
  const stale = stampOf();
  await waitFor('the server to have read the changed routes file', async () => (await scene(page, BOARD)).routesStamp === stale);
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('[data-testid=findings-chip].warn, [data-testid=findings-chip].bad');
  await page.locator('.side-tabs button', { hasText: 'Findings' }).click();
  await page.waitForSelector('[data-testid=finding]');
  const found = await page.locator('[data-testid=finding]').allInnerTexts();
  expect(found.some((f) => f.includes('routes_pads_moved') && f.includes('the pads of net `GND` are not where')), `the findings say so (${found.join(' | ').replace(/\n/g, ' ')})`);
  expect(found.some((f) => f.includes('routes file')), 'with the routes file as its source');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('.dock .problem');
  const problems = await page.locator('.dock .problem').allInnerTexts();
  expect(problems.some((p) => p.includes('the pads of net `GND` are not where')), 'the problems dock lists it');
  await page.locator('.side-tabs button', { hasText: 'Routes' }).click();
  await page.waitForSelector('[data-route-row="GND"]');
  expect((await page.locator('[data-route-row="GND"]').innerText()).includes('stale'), 'the Routes list marks GND stale');
  expect((await page.locator('[data-route-row="GND/track/2"].undrawn').count()) === 1, 'its unlocked tracks are not drawn');
  await whenIdle(page);
  await shot('stale-record');
}
