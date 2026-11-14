// The router's copper gives way to the track being drawn (issue #726, second slice): on a routed board, the copper
// of the router that the track crosses is not an obstacle (an edit of the routes file throws the router's copper away,
// so the track goes straight over it) and is marked while the track is drawn; the edit writes only the drawn track;
// the board is then unrouted, with a toast that says so; routing again lays the router's copper round the new track.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, screenAt } from '../pcb-helpers.mjs';

export const fixture = 'routed_led';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));

export async function run(tools) {
  const { page, shot, step, expect, project, url } = tools;
  const file = path.join(project, 'src/main.routes.toml');
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
  await page.selectOption('[data-testid=grid]', '0');
  await page.getByRole('button', { name: 'Fit' }).click();
  for (let i = 0; i < 2; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);

  const copperOf = (net) => page.locator(`.l-front-copper [data-net="${net}"], .l-back-copper [data-net="${net}"]`).count();
  const route = async () => {
    await page.getByRole('button', { name: /Place and route|Route again/ }).click();
    await waitFor('the board to be routed', async () => (await scene(page, BOARD)).routed);
    await whenIdle(page);
  };

  step('route the board: the router lays copper (the net the routes file has no copper of) beside the routes file\'s');
  await route();
  let s = await scene(page, BOARD);
  expect(s.routed, 'the board is routed');
  expect((await copperOf('VCC')) > 0, 'the drawing has the router\'s VCC copper');
  const [j1, r1] = s.parts.flatMap((p) => p.pads).filter((p) => p.net === 'VCC');
  const via = s.routes.find((n) => n.net === 'GND').vias[0].at;
  const at = (p) => screenAt(page, p.x, p.y);
  const draft = () => page.locator('[data-testid=track-draft]');
  const shoved = () => page.locator('[data-testid=router-shoved]');
  /** Move the pointer to `p` and wait for the walkaround's answer to it. */
  const hover = async (p) => {
    const q = await at(p);
    await page.mouse.move(q.x, q.y);
    await whenIdle(page);
    await page.waitForFunction(() => document.querySelector('[data-testid=track-draft]')?.getAttribute('data-walk') !== 'asked');
  };

  step('a GND track from the via, ending beyond the router\'s VCC track: it goes over it, and the VCC copper it crosses is marked');
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-options]');
  await page.selectOption('[data-testid=track-layer]', 'top');
  let p = await at(via);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-net="GND"]');
  expect((await shoved().count()) === 0, 'nothing is marked while the pointer is on the via');
  const between = { x: (j1.x + r1.x) / 2, y: j1.y };
  const target = { x: between.x + 0.2, y: between.y + 1.4 };
  await hover(target);
  expect((await draft().getAttribute('data-walk')) === 'path', 'the segment has a way');
  expect((await shoved().count()) === 1, 'the crossed copper is marked');
  const tracks = Number(await shoved().getAttribute('data-tracks'));
  const nets = await shoved().locator('[data-net]').evaluateAll((es) => [...new Set(es.map((e) => e.getAttribute('data-net')))]);
  expect(tracks >= 1 && nets.length === 1 && nets[0] === 'VCC', `the router's VCC track is what is crossed (${tracks} track(s) of ${nets})`);
  const pill = await page.locator('[data-testid=track-crossed]').innerText();
  expect(/Crosses \d+ router tracks?/.test(pill), `the note on the board says so ("${pill.split('\n')[0]}")`);
  await shot('shove-preview');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('shove-preview-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('a pointer that crosses no router copper marks nothing');
  await hover({ x: via.x + 1.0, y: via.y });
  expect((await draft().getAttribute('data-walk')) === 'path' && (await shoved().count()) === 0, 'no mark');
  expect((await page.locator('[data-testid=track-crossed]').count()) === 0, 'no note on the board');

  step('the clicks fix the corners, Enter ends the track: one edit, of the drawn track only, and the router\'s copper is discarded');
  await hover(target);
  p = await at(target);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForFunction(() => Number(document.querySelector('[data-testid=track-draft]')?.getAttribute('data-points')) >= 2);
  expect((await shoved().count()) === 1, 'the copper crossed stays marked while the track is drawn');
  await page.keyboard.press('Enter');
  await waitFor('the track to be written', async () => (await scene(page, BOARD)).routes.find((n) => n.net === 'GND').tracks.length > 6);
  await whenIdle(page);
  s = await scene(page, BOARD);
  expect(!s.routed && !s.routes.some((n) => n.net === 'VCC'), 'the board is unrouted and the routes file has no VCC record: only the track was written');
  expect((await copperOf('VCC')) === 0, 'the drawing has no VCC copper: the router\'s is gone');
  await page.locator('.toast', { hasText: "the router's copper was discarded" }).first().waitFor();
  expect((await shoved().count()) === 0, 'nothing is marked any more');
  await shot('shove-committed');

  step('routing again lays the router\'s copper round the new track');
  await route();
  s = await scene(page, BOARD);
  expect(s.routed && (await copperOf('VCC')) > 0, 'the router has laid VCC copper again');
  await shot('shove-rerouted');
  await page.keyboard.press('Control+z');
  await waitFor('the track to be undone', async () => (await scene(page, BOARD)).routes.find((n) => n.net === 'GND').tracks.length === 6);
  await whenIdle(page);
  await page.keyboard.press('Escape');
}
