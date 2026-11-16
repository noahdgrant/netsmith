// The routes file edited from a terminal while the editor is open (epic #599, issue #734). The editor
// watches `<stem>.routes.toml` as it watches the layout file, so a `netsmith routes add-track | add-via
// | lock | unlock | delete` run beside the open page shows in it with no reload: the copper, the padlock,
// the ring of a via, the Routes tab's rows. The router's copper is gone after such an edit, as after an
// edit made in the editor (the board is placed again from the file and is not routed until the next
// route). A command the CLI refuses writes nothing and the page does not change. When the editor and the
// CLI edit at once, the editor's write is refused (409, the file changed on disk), the board is read again
// from disk, and the steps of undo stay. The page is open in this recipe's browser; the commands are the
// binary's own, run in the project as a person runs them.
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, screenAt, writeAtomic } from '../pcb-helpers.mjs';

export const fixture = 'routed_led';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));
const exec = promisify(execFile);

export async function run(tools) {
  const { page, shot, step, expect, project, url, binary, allowConsole } = tools;
  // The editor's write that loses the race is a 409 on purpose, which the browser logs.
  allowConsole(/status of 409 .*\/api\/pcb\/routes/);
  const file = path.join(project, 'src/main.routes.toml');
  const text = () => fs.readFileSync(file, 'utf8');
  const stampOf = () => {
    const on = fs.statSync(file, { bigint: true });
    return `${on.mtimeNs}-${on.size}`;
  };
  /** `netsmith routes ...` in the project, as a person runs it: the exit code and both streams. */
  const cli = async (...args) => {
    try {
      const r = await exec(binary, ['routes', ...args], { cwd: project });
      return { code: 0, out: r.stdout, err: r.stderr };
    } catch (e) {
      return { code: e.code, out: e.stdout ?? '', err: e.stderr ?? '' };
    }
  };
  const mm = (n) => n.toFixed(4);
  // The scene's places are the sheet's; `netsmith routes` takes mm from the outline's top left corner.
  let origin = { x: 0, y: 0 };
  const xy = (p) => `${mm(p.x - origin.x)},${mm(p.y - origin.y)}`;

  // Every run starts from the fixture's file, whatever ran before.
  writeAtomic(file, fs.readFileSync(path.join(here, '../fixtures/routed_led.routes.toml'), 'utf8'));
  await page.goto(url);
  await page.waitForSelector('.app');
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === stampOf());
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  await page.waitForSelector('[data-testid=fixed-copper] .fixed-shape');
  await page.getByRole('button', { name: 'Fit' }).click();
  await page.locator('.side-tabs button', { hasText: 'Routes' }).click();
  await page.waitForSelector('[data-testid=route-list]');
  await whenIdle(page);

  let s = await scene(page, BOARD);
  origin = { x: s.originX, y: s.originY };
  const vcc = () => s.routes.find((n) => n.net === 'VCC');
  const anode = s.routes.find((n) => n.net === 'net-led.anode');
  const pads = s.parts.flatMap((p) => p.pads).filter((p) => p.net === 'VCC');
  const gnd = s.parts.flatMap((p) => p.pads).find((p) => p.net === 'GND');
  expect(pads.length === 2 && !vcc(), 'VCC has two pads and no record: the router routes it');
  // This page is never reloaded from here on: a marker on its window is still there at the end.
  await page.evaluate(() => (window.__notReloaded = true));
  /** The page, read now. */
  const marks = async (net) => ({
    shapes: await page.locator(`.fixed-net[data-fixed-net="${net}"] .fixed-shape`).count(),
    vias: await page.locator(`.fixed-via[data-route-net="${net}"]`).count(),
    locks: await page.locator(`[data-route-net="${net}"] .lock-mark`).count(),
    solid: await page.locator(`.fixed-net.net-locked[data-fixed-net="${net}"]`).count(),
    row: await page.locator(`[data-route-row="${net}"]`).count(),
  });
  /** The router's copper in the board's drawing: the tracks of the front copper layer, minus the routes file's (marked apart). */
  const copperPaths = () => page.locator('.pcb-host svg:not(.pcb-overlay) .l-front-copper > *').count();
  const reloaded = () => page.evaluate(() => !window.__notReloaded);
  /** What the page shows once the scene it fetches satisfies `test`. The page learns of a change on disk through the server's
   * watcher (it polls the files) and fetches the board again: nothing here waits for a time, only for what the step expects. */
  const shows = async (what, test) => {
    await waitFor(what, async () => test(await scene(page, BOARD)));
    // The scene is the server's; the page has it when its own state says so: the marks are drawn.
    await whenIdle(page);
    s = await scene(page, BOARD);
    return s;
  };

  step('place and route: the router adds copper for VCC (it is on the board, the chip says routed)');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  await whenIdle(page);
  const routedCopper = await copperPaths();
  expect((await scene(page, BOARD)).routed, 'the server has the board routed');
  await shot('routed');

  step('add-track from the terminal: the copper appears with no reload, the router\'s copper is gone, the Routes tab lists the net');
  const stamp0 = s.routesStamp;
  const t0 = Date.now();
  let r = await cli('add-track', '--net', 'VCC', '--layer', 'bottom', '--from', xy(pads[0]), '--to', xy(pads[1]));
  expect(r.code === 0, `exit 0: ${r.err}${r.out}`);
  await page.waitForSelector('[data-route-row="VCC"]');
  const seen = Date.now();
  console.log(`  the page showed the CLI's track ${seen - t0} ms after the command was started (printed, never judged)`);
  await shows('the page to have read the file the CLI wrote', (n) => n.routesStamp !== stamp0 && n.routesStamp === stampOf());
  expect(!s.routed, 'the board is no longer routed');
  expect((await page.locator('.chip:has-text("routed")').count()) === 0, 'the chip no longer says routed');
  expect((await copperPaths()) < routedCopper, `the router's copper is gone from the drawing (${routedCopper} shapes before, ${await copperPaths()} now)`);
  expect(vcc()?.tracks.length === 1 && vcc().tracks[0].layer === 'bottom' && vcc().tracks[0].drawn, 'the scene has the track, on the bottom, drawn');
  let m = await marks('VCC');
  expect(m.shapes === 1 && m.row === 1, `the track is marked on the board and the net is a row of the Routes tab (${m.shapes} shape, ${m.row} row)`);
  expect((await page.locator('[data-route-row="VCC"]').innerText()).includes('1 track'), 'the row says 1 track');
  expect(!(await reloaded()), 'the page was not reloaded');
  await shot('cli-track');

  step('lock --track 1: a padlock appears on the track');
  r = await cli('lock', '--net', 'VCC', '--track', '1');
  expect(r.code === 0, `exit 0: ${r.err}${r.out}`);
  await page.waitForSelector('[data-route-net="VCC"][data-route-track="1"] .lock-mark');
  await shows('the lock to be read', (n) => n.routes.find((x) => x.net === 'VCC')?.tracks[0].lock);
  expect((await marks('VCC')).locks === 1, 'a padlock on the track');
  expect(/lock = true/.test(text()), 'and `lock = true` in the file');

  step('add-via at the track\'s end: the via has a ring');
  r = await cli('add-via', '--net', 'VCC', '--at', xy(pads[1]));
  expect(r.code === 0, `exit 0: ${r.err}${r.out}`);
  await page.waitForSelector('.fixed-via[data-route-net="VCC"]');
  await shows('the via to be read', (n) => n.routes.find((x) => x.net === 'VCC')?.vias.length === 1);
  expect((await marks('VCC')).vias === 1, 'a ring round the via');
  await page.locator('[data-route-row="VCC"]').waitFor();
  expect((await page.locator('[data-route-row="VCC"]').innerText()).includes('1 via'), 'the row says 1 via');
  await shot('cli-via');

  step('lock the net: its marks turn solid; unlock it: dashed again');
  r = await cli('lock', '--net', 'VCC');
  expect(r.code === 0, `exit 0: ${r.err}${r.out}`);
  await page.waitForSelector('.fixed-net.net-locked[data-fixed-net="VCC"]');
  await shows('the net lock to be read', (n) => n.routes.find((x) => x.net === 'VCC')?.lock);
  expect((await page.locator('[data-route-row="VCC"]').innerText()).includes('lock'), 'the Routes tab marks the net locked');
  await shot('cli-net-locked');
  r = await cli('unlock', '--net', 'VCC');
  expect(r.code === 0, `exit 0: ${r.err}${r.out}`);
  await page.waitForSelector('.fixed-net:not(.net-locked)[data-fixed-net="VCC"]');
  await shows('the net unlock to be read', (n) => !n.routes.find((x) => x.net === 'VCC')?.lock);

  step('a track picked in the editor is let go when the CLI deletes it; the page is not reloaded');
  await page.locator('[data-route-row="VCC/track/1"]').click();
  await page.waitForSelector('[data-testid=route-inspector]');
  expect((await page.locator('[data-testid=route-inspector]').innerText()).includes('track 1 of net VCC'), 'the inspector names the track');
  r = await cli('unlock', '--net', 'VCC', '--track', '1');
  expect(r.code === 0, `exit 0: ${r.err}${r.out}`);
  await shows('the track unlock to be read', (n) => !n.routes.find((x) => x.net === 'VCC')?.tracks[0].lock);
  r = await cli('delete', '--net', 'VCC');
  expect(r.code === 0, `exit 0: ${r.err}${r.out}`);
  await page.waitForSelector('[data-route-row="VCC"]', { state: 'detached' });
  await shows('the record to be gone', (n) => !n.routes.find((x) => x.net === 'VCC'));
  m = await marks('VCC');
  expect(m.shapes === 0 && m.vias === 0 && m.row === 0, 'no copper and no row of VCC');
  expect((await page.locator('[data-testid=route-inspector]').count()) === 0, 'the inspector is gone with the shape');
  expect(!(await reloaded()), 'the page was not reloaded');
  await shot('cli-deleted');

  step('the board is not routed after the CLI edits, until the next route: Place and route brings the router\'s copper back, round the file\'s');
  expect((await page.locator('.chip:has-text("routed")').count()) === 0 && !s.routed, 'still unrouted');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  await whenIdle(page);
  expect((await copperPaths()) === routedCopper, `the router's copper is back (${await copperPaths()} shapes, ${routedCopper} the first time)`);
  expect((await page.locator('[data-testid=fixed-copper] .fixed-shape').count()) > 0, 'the routes file\'s copper is still marked');

  step('refused commands write nothing and the page does not change');
  const before = text();
  const stampBefore = stampOf();
  s = await scene(page, BOARD);
  const sceneBefore = JSON.stringify(s);
  // A locked track of the file cannot be deleted.
  r = await cli('delete', '--net', 'net-led.anode', '--track', '2');
  expect(r.code === 1 && /locked/.test(r.err + r.out), `delete of a locked track: exit 1 and the reason (${r.code}: ${(r.err + r.out).trim()})`);
  // A net the board does not have.
  r = await cli('add-track', '--net', 'NoSuchNet', '--from', '1,1', '--to', '2,2');
  expect(r.code === 1, `a net the board does not have: exit 1 (${r.code}: ${(r.err + r.out).trim()})`);
  // A track of VCC onto a GND pad breaks the design rules.
  r = await cli('add-track', '--net', 'VCC', '--layer', 'top', '--from', xy(pads[1]), '--to', xy(gnd));
  expect(r.code === 1 && /design rules/.test(r.err + r.out), `a track onto another net's pad: exit 1, the design rules (${r.code}: ${(r.err + r.out).trim()})`);
  expect(text() === before && stampOf() === stampBefore, 'the routes file is as it was, byte for byte and in its stamp');
  await whenIdle(page);
  expect(JSON.stringify(await scene(page, BOARD)) === sceneBefore, 'the server\'s scene is as it was');
  expect((await page.locator('.chip:has-text("routed")').count()) === 1, 'the board is still routed (nothing was written, so nothing was placed again)');
  expect(!(await reloaded()), 'the page was not reloaded');
  await shot('cli-refused');

  step('the editor and the CLI edit at once: the editor\'s write is refused (409), the board is read from disk, the steps of undo stay');
  // First an edit of the editor's own, so there is a step to undo: lock a track of the anode net.
  const track = (i) => anode.tracks[i];
  const anodeNow = () => s.routes.find((n) => n.net === 'net-led.anode');
  const pick = async (t) => {
    const at = await screenAt(page, (t.from.x + t.to.x) / 2, (t.from.y + t.to.y) / 2);
    await page.mouse.click(at.x, at.y);
    await page.waitForSelector('[data-testid=route-inspector]');
    await whenIdle(page);
  };
  await pick(track(0));
  let stamp = (await scene(page, BOARD)).routesStamp;
  await page.keyboard.press('l');
  s = await waitFor('the routes file to be written by the editor', async () => {
    const now = await scene(page, BOARD);
    return now.routesStamp !== stamp ? now : null;
  });
  await whenIdle(page);
  expect(s.routes.find((n) => n.net === 'net-led.anode').tracks[0].lock, 'the editor locked track 1 of the anode net');
  expect(!(await page.locator('[data-testid=undo]').isDisabled()), 'there is a step to undo');
  // The editor's next edit is held on its way to the server; the CLI writes the file meanwhile; then the edit goes on, with the
  // stamp of the file as the page read it before the CLI wrote.
  await pick(track(2));
  let cliAnswer = null;
  const answered = new Promise((resolve) => {
    page.route('**/api/pcb/routes', async (route) => {
      cliAnswer = await cli('add-track', '--net', 'VCC', '--layer', 'bottom', '--from', xy(pads[0]), '--to', xy(pads[1]));
      await route.continue();
      resolve();
    });
  });
  await page.keyboard.press('l');
  await answered;
  await page.unroute('**/api/pcb/routes');
  expect(cliAnswer?.code === 0, `the CLI wrote while the editor's edit was on its way: exit 0 (${cliAnswer?.err})`);
  await page.locator('.toast', { hasText: 'changed on disk since the board was read' }).first().waitFor();
  await whenIdle(page);
  s = await scene(page, BOARD);
  expect(vcc()?.tracks.length === 1, 'the board was read from disk again: it has the CLI\'s track');
  expect(!s.routes.find((n) => n.net === 'net-led.anode').tracks[2].lock, 'the editor\'s refused edit was not written (track 3 is not locked)');
  expect((await page.locator('[data-route-net="VCC"][data-route-track="1"]').count()) === 1, 'and the page draws the CLI\'s track');
  expect(!(await page.locator('[data-testid=undo]').isDisabled()), 'the step of undo stays');
  await shot('conflict-409');
  await page.locator('[data-testid=undo]').click();
  await waitFor('the undo to be written', async () => !(await scene(page, BOARD)).routes.find((n) => n.net === 'net-led.anode').tracks[0].lock);
  await whenIdle(page);
  s = await scene(page, BOARD);
  expect(!anodeNow().tracks[0].lock && vcc()?.tracks.length === 1, 'the undo unlocked the anode track and left the CLI\'s track');
  expect(!(await reloaded()), 'the page was not reloaded, ever');
  await shot('after-undo');
}
