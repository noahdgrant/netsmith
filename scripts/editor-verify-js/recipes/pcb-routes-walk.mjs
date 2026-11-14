// The walkaround of the track tool (issue #726): while a track is drawn, the segment from its last corner to
// the pointer bends round the copper in the way (the server asks `netsmith_router::walk`, the router's room: the
// design rules check's own spaces) with corners of 45 degrees at most; a click fixes the corners it chose as
// ordinary track corners; where there is no way the preview says so and the click is refused with the reason;
// and the end of the track still goes through the routes file's check.
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
  for (let i = 0; i < 3; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);

  let s = await scene(page, BOARD);
  const at = (p) => screenAt(page, p.x, p.y);
  const pads = s.parts.flatMap((p) => p.pads);
  const vcc = pads.filter((p) => p.net === 'VCC');
  const r1 = s.parts.find((p) => p.refdes === 'R1').pads.find((p) => p.net === 'VCC');
  const j1 = vcc.find((p) => p !== r1);
  const gnd = pads.find((p) => p.net === 'GND');
  const draft = () => page.locator('[data-testid=track-draft]');
  const state = async () => ({
    walk: await draft().getAttribute('data-walk'),
    points: Number(await draft().getAttribute('data-points')),
    corners: Number(await draft().getAttribute('data-walk-corners')),
  });
  /** Move the pointer to `p` and wait for the walkaround's answer to it. */
  const hover = async (p) => {
    const q = await at(p);
    await page.mouse.move(q.x, q.y);
    await whenIdle(page);
    await page.waitForFunction(() => document.querySelector('[data-testid=track-draft]')?.getAttribute('data-walk') !== 'asked');
    return state();
  };

  step('X takes the track tool, top layer; a click on R1\'s VCC pad starts a track');
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-options]');
  await page.selectOption('[data-testid=track-layer]', 'top');
  let p = await at(r1);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');

  step('a pointer in the open gets the straight segment: the walkaround has nothing to go round');
  const open = { x: r1.x - 1.2, y: r1.y };
  let st = await hover(open);
  expect(st.walk === 'path' && st.corners === 0, `a straight segment (${JSON.stringify(st)})`);

  step('a pointer beyond the anode track, which a straight segment would cross: the segment bends round it');
  const wall = s.routes.find((n) => n.net === 'net-led.anode').tracks[1];
  const target = { x: (wall.from.x + wall.to.x) / 2 + 0.3, y: wall.from.y + 1.3 };
  st = await hover(target);
  expect(st.walk === 'path' && st.corners >= 1, `the preview bends round it with ${st.corners} corner(s) of its own`);
  await shot('walk-preview');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('walk-preview-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('a click fixes the corners it chose: they are ordinary corners of the draft');
  const corners = st.corners;
  p = await at(target);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForFunction((n) => Number(document.querySelector('[data-testid=track-draft]')?.getAttribute('data-points')) === n, 1 + corners + 1);
  expect((await state()).points === 2 + corners, `the draft has the pad, the ${corners} corner(s) and the point clicked`);

  step('Enter ends the track: one edit of the routes file, through the same check, with a track for each segment');
  await page.keyboard.press('Enter');
  await waitFor('the track to be written', async () => (await scene(page, BOARD)).routes.some((n) => n.net === 'VCC'));
  await whenIdle(page);
  s = await scene(page, BOARD);
  const track = s.routes.find((n) => n.net === 'VCC').tracks;
  expect(track.length === corners + 1 && track.every((t) => t.layer === 'top' && t.drawn), `${track.length} segments on the top, drawn`);
  await page.locator('.toast', { hasText: 'added a track to net VCC' }).first().waitFor();
  await shot('walk-drawn');
  await page.keyboard.press('Control+z');
  await waitFor('the track to be undone', async () => !(await scene(page, BOARD)).routes.some((n) => n.net === 'VCC'));
  await whenIdle(page);

  step('where there is no way (the point is on a pad of another net) the preview says so and a click is refused with the reason');
  p = await at(j1);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-points="1"]');
  st = await hover(gnd);
  expect(st.walk === 'refused', `the preview is refused (${JSON.stringify(st)})`);
  await page.getByText('No way there').waitFor();
  await shot('walk-refused');
  p = await at(gnd);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.locator('.toast', { hasText: 'no room for a track there' }).first().waitFor();
  expect((await state()).points === 1, 'the click added no corner');

  step('the other layer has its own copper: the same pointer, on the bottom, has a way where the top had none');
  const wallTop = s.routes.find((n) => n.net === 'GND').tracks[1];
  const across = { x: Math.min(wallTop.from.x, wallTop.to.x) + 0.9, y: wallTop.from.y - 0.9 };
  const onTop = await hover(across);
  expect(onTop.walk === 'refused' || onTop.corners > 0, `on the top the ground track is in the way (${JSON.stringify(onTop)})`);
  await shot('walk-top-across');
  await page.selectOption('[data-testid=track-layer]', 'bottom');
  st = await hover(across);
  expect(st.walk === 'path' && st.corners === 0, `on the bottom the segment crosses under the ground track as it is (${JSON.stringify(st)})`);
  await shot('walk-bottom');

  step('Esc drops the track');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=track-draft]', { state: 'detached' });
  await page.keyboard.press('Escape');
  await page.waitForSelector('.pcb-host.tool-select');
}
