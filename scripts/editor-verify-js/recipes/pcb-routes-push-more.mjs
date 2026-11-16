// Push and shove beyond the track (issue 1009): a via placed with the via tool and the end of a track dragged in Select shove the
// unlocked copper of the routes file out of their way, as a track does, in one edit and one step of undo; with "go round only" a
// via that has no room is refused with what is in the way; and a track drawn on the top changes to the bottom through a via at its
// last corner when another layer is picked while it is drawn. The routes file is written by the recipe: an unlocked ground track
// hangs from D1's pad, as in pcb-routes-push.
import fs from 'node:fs';
import path from 'node:path';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, screenAt, writeAtomic } from '../pcb-helpers.mjs';

export const fixture = 'routed_led';
const BOARD = 'LedExample';

const ROUTES = `version = 1

[LedExample.nets.GND]
pads = "8a44310675df4e54"
tracks = [
  { layer = "top", width = 0.15, from = [10.8125, 3.75], to = [10.8125, 6.0] },
]
`;

export async function run(tools) {
  const { page, shot, step, expect, project, url, allowConsole } = tools;
  // The check's refusal of a track is a 422 of the routes edit, which the page shows as a toast.
  allowConsole(/status of 422 .*\/api\/pcb\/routes/);
  const file = path.join(project, 'src/main.routes.toml');
  writeAtomic(file, ROUTES);
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
  await page.waitForSelector('[data-testid=fixed-copper] .fixed-shape', { state: 'attached' });
  await page.selectOption('[data-testid=grid]', '0');
  await page.getByRole('button', { name: 'Fit' }).click();
  for (let i = 0; i < 3; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);

  let s = await scene(page, BOARD);
  const at = (p) => screenAt(page, p.x, p.y);
  const r1 = s.parts.find((p) => p.refdes === 'R1').pads.find((p) => p.net === 'VCC');
  const record = (net) => scene(page, BOARD).then((x) => x.routes.find((n) => n.net === net));
  const was = (await record('GND')).tracks[0];
  const preview = () => page.locator('[data-testid=shove-preview]');
  const shovedCount = async () => ((await preview().count()) === 0 ? 0 : Number(await preview().getAttribute('data-tracks')));
  const move = async (p) => {
    const q = await at(p);
    await page.mouse.move(q.x, q.y);
    await whenIdle(page);
  };
  const click = async (p) => {
    const q = await at(p);
    await page.mouse.click(q.x, q.y);
    await whenIdle(page);
  };
  const undoTo = async (what, check) => {
    await page.keyboard.press('Control+z');
    await waitFor(what, check);
    await whenIdle(page);
  };

  // A VCC track from R1's pad that ends left of the ground track, clear of it: drawn going round only, so nothing moves.
  const end = { x: was.from.x - 0.38, y: was.from.y + 1.3 };
  step('a VCC track drawn to just left of the ground track, going round only: nothing is shoved');
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.selectOption('[data-testid=track-layer]', 'top');
  await page.selectOption('[data-testid=track-shove]', 'walk');
  await click(r1);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  await move(end);
  await page.waitForFunction(() => document.querySelector('[data-testid=track-draft]')?.getAttribute('data-walk') === 'path');
  await click(end);
  await page.keyboard.press('Enter');
  await waitFor('the VCC track to be written', async () => (await scene(page, BOARD)).routes.some((n) => n.net === 'VCC'));
  await whenIdle(page);
  const vcc = await record('VCC');
  const last = vcc.tracks[vcc.tracks.length - 1].to;
  expect((await record('GND')).tracks[0].to.x === was.to.x, 'the ground track is where it was');
  const written = fs.readFileSync(file, 'utf8');

  step('the check has the last word: a track from that end at a right angle to it is walked, and refused when it is ended');
  await page.selectOption('[data-testid=track-shove]', 'shove');
  await click(last);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  const square = { x: last.x - 1.2, y: last.y + 1.0 };
  await move(square);
  await page.waitForFunction(() => document.querySelector('[data-testid=track-draft]')?.getAttribute('data-walk') === 'path');
  await click(square);
  await page.keyboard.press('Enter');
  await page.locator('.toast', { hasText: 'the design rules check finds' }).first().waitFor();
  await page.waitForSelector('[data-testid=track-draft][data-points="2"]');
  await whenIdle(page);
  expect(fs.readFileSync(file, 'utf8') === written, 'the routes file is as it was');
  await shot('push-check-refused');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=track-draft]', { state: 'detached' });

  step('the via tool on the end of the VCC track: the via shoves the ground track aside, shown before the click');
  await page.keyboard.press('v');
  await page.selectOption('[data-testid=track-shove]', 'shove');
  await move(last);
  await page.waitForSelector('[data-testid=via-ghost]');
  await preview().waitFor();
  await whenIdle(page);
  expect((await shovedCount()) === 1, 'one track is shoved');
  expect((await preview().locator('.shove-track').getAttribute('data-net')) === 'GND', 'it is the ground track');
  await page.locator('[data-testid=track-shoved]').waitFor();
  expect(fs.readFileSync(file, 'utf8') === written, 'nothing is written while the pointer moves');
  await shot('push-via-preview');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('push-via-preview-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('a click places the via and moves the ground track in one edit; one undo puts both back');
  await click(last);
  await waitFor('the via to be written', async () => (await record('VCC')).vias.length === 1);
  await whenIdle(page);
  const pushed = (await record('GND')).tracks[0];
  expect(pushed.from.x === was.from.x && pushed.to.x > was.to.x, `the ground track turned about its pad, away (${was.to.x} to ${pushed.to.x})`);
  await page.locator('.toast', { hasText: 'added a via to net VCC' }).first().waitFor();
  await shot('push-via-placed');
  await undoTo('the via to be undone', async () => (await record('VCC')).vias.length === 0);
  expect(fs.readFileSync(file, 'utf8') === written, 'the routes file is as it was before the via');

  step('going round only, the same via has no room: the tip names the ground track, and a click is refused');
  await page.selectOption('[data-testid=track-shove]', 'walk');
  await move({ x: last.x + 0.01, y: last.y });
  await move(last);
  await page.locator('.pcb-tip', { hasText: 'No room there' }).waitFor();
  const tip = await page.locator('.pcb-tip').innerText();
  expect(/track of net GND/.test(tip), `the tip names it ("${tip.replace(/\n/g, ' ')}")`);
  expect((await shovedCount()) === 0, 'nothing is shoved');
  await shot('push-via-no-room');
  await click(last);
  await page.locator('.toast', { hasText: 'No via there' }).first().waitFor();
  expect(fs.readFileSync(file, 'utf8') === written, 'nothing is written');
  await page.selectOption('[data-testid=track-shove]', 'shove');

  step('in Select, the end of the VCC track dragged nearer: the ground track is shoved while it is dragged, and on the drop');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.pcb-host.tool-select');
  const near = { x: was.from.x - 0.18, y: last.y };
  const from = await at(last);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  const half = await at({ x: (last.x + near.x) / 2, y: last.y });
  await page.mouse.move(half.x, half.y);
  const to = await at(near);
  await page.mouse.move(to.x, to.y);
  await whenIdle(page);
  await preview().waitFor();
  expect((await shovedCount()) === 1, 'the drag shows one track shoved');
  await shot('push-drag-preview');
  await page.mouse.up();
  await waitFor('the drag to be written', async () => {
    const v = await record('VCC');
    return Math.abs(v.tracks[v.tracks.length - 1].to.x - near.x) < 1e-3;
  });
  await whenIdle(page);
  const dragged = (await record('GND')).tracks[0];
  expect(dragged.to.x > was.to.x, 'the ground track moved with the drag');
  await page.locator('.toast', { hasText: 'moved track' }).first().waitFor();
  await shot('push-drag-dropped');
  await undoTo('the drag to be undone', async () => fs.readFileSync(file, 'utf8') === written);

  step('a track that changes layer: picked bottom while drawn on the top, a via goes at the last corner and the track goes on below');
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.selectOption('[data-testid=track-layer]', 'top');
  // From the end of the VCC track straight down (a corner the check allows), then on the bottom to the left.
  await click(last);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  const corner = { x: last.x, y: last.y + 1.1 };
  await move(corner);
  await page.waitForFunction(() => document.querySelector('[data-testid=track-draft]')?.getAttribute('data-walk') === 'path');
  await click(corner);
  await page.waitForFunction(() => Number(document.querySelector('[data-testid=track-draft]')?.getAttribute('data-points')) === 2);
  await page.selectOption('[data-testid=track-layer]', 'bottom');
  await page.waitForSelector('[data-testid=track-draft][data-runs="2"][data-vias="1"][data-layer="bottom"]');
  const below = { x: corner.x - 1.5, y: corner.y };
  await move(below);
  await page.waitForFunction(() => document.querySelector('[data-testid=track-draft]')?.getAttribute('data-walk') === 'path');
  await shot('push-layer-change');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('push-layer-change-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);
  await click(below);
  await page.keyboard.press('Enter');
  await waitFor('the track and its via to be written', async () => (await record('VCC')).vias.length === 1);
  await whenIdle(page);
  const layered = await record('VCC');
  const layers = [...new Set(layered.tracks.map((t) => t.layer))].sort();
  expect(layers.join() === 'bottom,top', `the track is on both layers (${layers})`);
  expect(Math.abs(layered.vias[0].at.x - corner.x) < 1e-3 && Math.abs(layered.vias[0].at.y - corner.y) < 1e-3, 'the via is at the corner');
  await page.locator('.toast', { hasText: 'via' }).first().waitFor();
  await undoTo('the layered track to be undone', async () => fs.readFileSync(file, 'utf8') === written);
  await page.selectOption('[data-testid=track-layer]', 'top');
  await page.keyboard.press('Escape');
}
