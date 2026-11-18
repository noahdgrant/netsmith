// What the fill took out and the pads it starves, marked on the 2D PCB view (issue 1188): each copper island the fill of a
// pour removed (copper that touched no copper of its net, so the Gerbers have none there) hatched and outlined, and each pad
// the fill starves (fewer than two spokes and tracks join it to the pour) or joins by nothing (no spoke and no track, the
// check's `unjoinable_thermal`, issue #1222) ringed, as `netsmith view pcb --islands --drc` marks them. The marks are the
// server's report of the fill (`GET /api/pcb/fill`, `netsmith_pcb::FillReport`), drawn over the copper; each kind has its
// switch in the Layers menu, and both read in the light and the dark theme. The routes file here
// (`routed_pours_fill.routes.toml`) routes VCC on the back round the header's ground pad, so close that the back pour joins it
// with none of its spokes (an unjoinable pad), and draws a closed ring of VCC on the back whose inside the fill takes out.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene } from '../pcb-helpers.mjs';

export const fixture = 'routed_pours';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));

export async function run(tools) {
  const { page, shot, step, expect, project, url } = tools;
  const file = path.join(project, 'src/main.routes.toml');
  const stampOf = () => {
    const on = fs.statSync(file, { bigint: true });
    return `${on.mtimeNs}-${on.size}`;
  };
  fs.copyFileSync(path.join(here, '../fixtures/routed_pours_fill.routes.toml'), file);
  await page.goto(url);
  await page.waitForSelector('.app');
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === stampOf());
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  // The report of the fill comes after the pours: wait for the marks of the placement drawn.
  await page.waitForSelector('[data-testid=removed-islands][data-current=true]', { state: 'attached' });
  await page.waitForSelector('[data-testid=starved-pads][data-current=true]', { state: 'attached' });
  await whenIdle(page);
  await page.getByRole('button', { name: 'Fit' }).click();
  for (let i = 0; i < 3; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);

  step('the server reports one removed island with its outline and one pad on the back that the fill joins by nothing');
  const report = await page.evaluate(async (b) => (await fetch(`/api/pcb/fill?board=${encodeURIComponent(b)}`)).json(), BOARD);
  const removed = report.fills.flatMap((f) => f.removedIslands.map((i) => ({ ...i, layer: f.layerName })));
  expect(removed.length === 1 && removed[0].layer === 'B.Cu', `one island removed on the back (${removed.map((i) => i.layer)})`);
  expect(removed[0].outline.length >= 1 && removed[0].outline[0].length >= 3, 'it has its outline');
  const names = (list) => list.map((s) => `${s.part}.${s.pad}`);
  expect(report.starved.length === 0, `no starved pad (${names(report.starved)})`);
  expect(report.unjoinable.length === 1 && report.unjoinable[0].layer === 'bottom', `one unjoinable pad on the bottom (${names(report.unjoinable)})`);
  const lone = report.unjoinable[0];

  step('the page hatches the island where the server says and rings the pad joined by nothing');
  const island = page.locator('.l-islands > path.removed-island');
  expect((await island.count()) === 1, 'one island drawn');
  expect((await island.getAttribute('data-layer')) === 'B.Cu' && (await island.getAttribute('class')).includes('island-back'), 'on the back');
  const inside = await page.evaluate((ring) => {
    const p = document.querySelector('.l-islands > path.removed-island');
    // The middle of the outline's box: the ring is a small round shape, so its middle is in it.
    const xs = ring.map((q) => q.x);
    const ys = ring.map((q) => q.y);
    const mid = new DOMPoint((Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2);
    const s = getComputedStyle(p);
    return { filled: p.isPointInFill(mid), fill: s.fill, stroke: s.stroke, dash: s.strokeDasharray };
  }, removed[0].outline[0]);
  expect(inside.filled, 'the middle of the island is in its shape');
  expect(inside.fill.includes('pcb-hatch-island'), `it is hatched (${inside.fill})`);
  expect(inside.dash !== 'none' && inside.stroke !== 'none', `and outlined, dashed (${inside.stroke} ${inside.dash})`);
  const starved = page.locator('.l-starved > g.starved-pad');
  const pad = `${lone.part}.${lone.pad}`;
  expect((await starved.count()) === 1 && (await starved.getAttribute('data-pad')) === pad && (await starved.getAttribute('data-kind')) === 'unjoinable', `the unjoinable pad ${pad} is ringed`);
  const ring = await page.locator('.l-starved .starved-ring').evaluate((c) => ({ cx: Number(c.getAttribute('cx')), cy: Number(c.getAttribute('cy')), stroke: getComputedStyle(c).stroke }));
  expect(Math.abs(ring.cx - lone.at.x) < 1e-3 && Math.abs(ring.cy - lone.at.y) < 1e-3, 'the ring is round the pad');
  const tip = await page.locator('.l-starved title').textContent();
  expect(lone.why !== '' && tip.includes(lone.why), `its tip says why no spoke joins it (${tip})`);

  step('the marks are over the copper and take no part of the board out of reach');
  const over = await page.evaluate(() => {
    const svg = document.querySelector('.pcb-host svg');
    const marks = svg.querySelector('.l-islands');
    return [...svg.querySelectorAll('.l-pours, .l-pads, .l-front-copper, .l-back-copper')].every((e) => !!(e.compareDocumentPosition(marks) & Node.DOCUMENT_POSITION_FOLLOWING));
  });
  expect(over, 'drawn after the pours, the pads and the tracks');

  step('the colours are the theme\'s and differ between the light and the dark theme');
  const paint = () =>
    page.evaluate(() => ({
      island: getComputedStyle(document.querySelector('.l-islands > path')).stroke,
      starved: getComputedStyle(document.querySelector('.l-starved .starved-ring')).stroke,
    }));
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);
  const light = await paint();
  await shot('fill-marks-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  const dark = await paint();
  expect(light.island !== dark.island && light.starved !== dark.starved, `each takes the theme's colour (${JSON.stringify(light)} in light, ${JSON.stringify(dark)} in dark)`);
  await page.getByTestId('pcb-side-back').click();
  await whenIdle(page);
  await shot('fill-marks-dark-back');
  await page.getByTestId('pcb-side-both').click();
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('the Layers menu switches the islands and the starved pads apart');
  const shown = (sel) => page.locator(sel).first().evaluate((e) => getComputedStyle(e).display !== 'none');
  await page.locator('details.layers summary').click();
  const toggle = async (label) => {
    await page.locator('.layer-list label', { hasText: label }).locator('input').click();
    await whenIdle(page);
  };
  await toggle('Removed islands');
  expect(!(await shown('.l-islands')) && (await shown('.l-starved')), 'islands off: only the islands go');
  await toggle('Removed islands');
  await toggle('Starved pads');
  expect((await shown('.l-islands')) && !(await shown('.l-starved')), 'starved pads off: only the rings go');
  await page.locator('.layer-list button', { hasText: 'Show all' }).click();
  await whenIdle(page);
  expect((await shown('.l-islands')) && (await shown('.l-starved')), 'show all brings both back');
  await page.keyboard.press('Escape');

  step('the fixture\'s own routes file has neither: the marks go with the copper that made them');
  fs.copyFileSync(path.join(here, '../fixtures/routed_pours.routes.toml'), file);
  const want = stampOf();
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === want);
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('[data-testid=pours][data-current=true]', { state: 'attached' });
  await whenIdle(page);
  const after = await page.evaluate(async (b) => (await fetch(`/api/pcb/fill?board=${encodeURIComponent(b)}`)).json(), BOARD);
  expect(after.starved.length === 0 && after.unjoinable.length === 0 && after.fills.every((f) => f.removedIslands.length === 0), 'the server reports none');
  expect((await page.locator('.l-islands, .l-starved').count()) === 0, 'and the page draws none');
}
