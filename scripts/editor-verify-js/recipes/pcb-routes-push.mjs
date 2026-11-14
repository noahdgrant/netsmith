// Push and shove (issue 726, the third slice): while the track tool draws a track, the unlocked tracks and vias of the routes
// file that are in its way move aside with it (the server asks `netsmith_router::shove`), the preview shows them at their new
// places with the old places faded, a locked shape is not moved (the track bends round it, as the walkaround does), the edit is
// one edit of several nets (the drawn track and the moved shape) and one step of undo, and the toolbar's mode can be set to go
// round only. The routes file is written by the recipe: an unlocked ground track hangs from D1's pad, and a locked anode track
// hangs from R1's.
import fs from 'node:fs';
import path from 'node:path';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, screenAt } from '../pcb-helpers.mjs';

export const fixture = 'routed_led';
const BOARD = 'LedExample';

// In the routes file's frame (mm from the outline's top left corner); the pad hashes are those of routed_led.routes.toml.
const ROUTES = `version = 1

[LedExample.nets.GND]
pads = "8a44310675df4e54"
tracks = [
  { layer = "top", width = 0.15, from = [10.8125, 3.75], to = [10.8125, 6.0] },
]

[LedExample.nets."net-led.anode"]
pads = "61fce8bd4b57d9f4"
tracks = [
  { layer = "top", width = 0.15, from = [8.6, 3.81], to = [8.6, 5.8], lock = true },
]
`;

export async function run(tools) {
  const { page, shot, step, expect, project, url } = tools;
  const file = path.join(project, 'src/main.routes.toml');
  fs.writeFileSync(file, ROUTES);
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
  // A vertical track has no width as a box: attached, not visible.
  await page.waitForSelector('[data-testid=fixed-copper] .fixed-shape', { state: 'attached' });
  await page.selectOption('[data-testid=grid]', '0');
  await page.getByRole('button', { name: 'Fit' }).click();
  for (let i = 0; i < 3; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);

  let s = await scene(page, BOARD);
  const at = (p) => screenAt(page, p.x, p.y);
  const pads = s.parts.flatMap((p) => p.pads);
  const r1 = s.parts.find((p) => p.refdes === 'R1').pads.find((p) => p.net === 'VCC');
  const j1 = pads.find((p) => p.net === 'VCC' && p !== r1);
  const gndRecord = () => scene(page, BOARD).then((x) => x.routes.find((n) => n.net === 'GND'));
  const was = (await gndRecord()).tracks[0];
  const draft = () => page.locator('[data-testid=track-draft]');
  const preview = () => page.locator('[data-testid=shove-preview]');
  const state = async () => ({
    walk: await draft().getAttribute('data-walk'),
    points: Number(await draft().getAttribute('data-points')),
    corners: Number(await draft().getAttribute('data-walk-corners')),
    shoved: (await preview().count()) === 0 ? 0 : Number(await preview().getAttribute('data-tracks')),
  });
  /** Move the pointer to `p` and wait for the answer to it. */
  const hover = async (p) => {
    const q = await at(p);
    await page.mouse.move(q.x, q.y);
    await whenIdle(page);
    await page.waitForFunction(() => document.querySelector('[data-testid=track-draft]')?.getAttribute('data-walk') !== 'asked');
    return state();
  };
  const click = async (p) => {
    const q = await at(p);
    await page.mouse.click(q.x, q.y);
    await whenIdle(page);
  };

  step('X takes the track tool: the toolbar has the layer and the mode, which shoves by default');
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-options]');
  await page.selectOption('[data-testid=track-layer]', 'top');
  expect((await page.locator('[data-testid=track-shove]').inputValue()) === 'shove', 'the mode is "shove copper aside"');

  step('a locked track in the way is not moved: the segment bends round it and nothing is shoved');
  await click(j1);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  const lockedBar = s.routes.find((n) => n.net === 'net-led.anode').tracks[0];
  const beyondLocked = { x: lockedBar.from.x + 0.8, y: lockedBar.from.y + 1.3 };
  let st = await hover(beyondLocked);
  expect(st.walk === 'path' && st.corners >= 1 && st.shoved === 0, `the way bends round the locked track, nothing moves (${JSON.stringify(st)})`);
  await shot('push-locked');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('push-locked-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=track-draft]', { state: 'detached' });

  step('an unlocked track in the way moves: the segment goes straight and the ground track turns about its pad');
  await click(r1);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  const target = { x: was.from.x + 0.2, y: was.from.y + 1.3 };
  st = await hover(target);
  expect(st.walk === 'path' && st.corners === 0 && st.shoved === 1, `straight, and one track shoved (${JSON.stringify(st)})`);
  const shoved = page.locator('[data-testid=shove-preview] .shove-track');
  expect((await shoved.getAttribute('data-net')) === 'GND' && (await shoved.getAttribute('data-track')) === '1', 'it is track 1 of net GND');
  await page.locator('[data-testid=track-shoved]').waitFor();
  expect(/Shoves 1 track/.test(await page.locator('[data-testid=track-shoved]').innerText()), 'the note on the board says so');
  expect((await gndRecord()).tracks[0].to.x === was.to.x, 'nothing is written while the pointer moves');
  await shot('push-preview');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('push-preview-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('"go round only" leaves the ground track where it is and bends round it instead');
  await page.selectOption('[data-testid=track-shove]', 'walk');
  const beyond = { x: was.from.x + 0.9, y: target.y };
  st = await hover(beyond);
  expect(st.shoved === 0 && st.corners >= 1, `no shove, and the way bends round the ground track (${JSON.stringify(st)})`);
  await shot('push-go-round');
  await page.selectOption('[data-testid=track-shove]', 'shove');
  st = await hover(target);
  expect(st.shoved === 1 && st.corners === 0, `back to the shove (${JSON.stringify(st)})`);

  step('a click and Enter write the drawn track and the moved track as one edit');
  await click(target);
  await page.waitForFunction(() => Number(document.querySelector('[data-testid=track-draft]')?.getAttribute('data-points')) === 2);
  expect((await state()).shoved === 1, 'the shove stays shown while the pointer rests on the corner');
  await page.keyboard.press('Enter');
  await waitFor('the track to be written', async () => (await scene(page, BOARD)).routes.some((n) => n.net === 'VCC'));
  await whenIdle(page);
  s = await scene(page, BOARD);
  const moved = (await gndRecord()).tracks[0];
  expect(moved.from.x === was.from.x && moved.from.y === was.from.y, 'the ground track still starts on D1\'s pad');
  expect(moved.to.x > was.to.x + 0.3 && moved.to.y === was.to.y, `its free end moved to the right (${was.to.x} to ${moved.to.x})`);
  expect(s.routes.find((n) => n.net === 'VCC').tracks.length === 1, 'the VCC record has the drawn track');
  const text = fs.readFileSync(file, 'utf8');
  expect(text.includes('lock = true'), 'the locked anode track is still in the file, locked');
  await page.locator('.toast', { hasText: 'added a track to net VCC' }).first().waitFor();
  await shot('push-committed');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('push-committed-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('one undo puts both records back, one redo makes both again');
  await page.keyboard.press('Control+z');
  await waitFor('the edit to be undone', async () => !(await scene(page, BOARD)).routes.some((n) => n.net === 'VCC'));
  await whenIdle(page);
  expect(fs.readFileSync(file, 'utf8') === ROUTES, 'the routes file is as it was written');
  const back = (await gndRecord()).tracks[0];
  expect(back.to.x === was.to.x && back.to.y === was.to.y, 'the ground track is where it was');
  await shot('push-undone');
  await page.keyboard.press('Control+Shift+z');
  await waitFor('the edit to be redone', async () => (await scene(page, BOARD)).routes.some((n) => n.net === 'VCC'));
  await whenIdle(page);
  const again = (await gndRecord()).tracks[0];
  expect(again.to.x === moved.to.x, 'the ground track is shoved again');
  await page.keyboard.press('Control+z');
  await waitFor('the edit to be undone again', async () => !(await scene(page, BOARD)).routes.some((n) => n.net === 'VCC'));
  await whenIdle(page);

  step('Esc drops a track being drawn and with it the shove: nothing is written');
  await click(r1);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  st = await hover(target);
  expect(st.shoved === 1, 'the shove is shown');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=track-draft]', { state: 'detached' });
  expect((await preview().count()) === 0, 'the shove is not shown any more');
  expect(fs.readFileSync(file, 'utf8') === ROUTES, 'the routes file is untouched');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.pcb-host.tool-select');
}
