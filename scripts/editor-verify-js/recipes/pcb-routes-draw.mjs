// Drawing and moving the routes file's copper in the PCB view (epic #599): the track tool draws a
// track from a pad through the corners clicked (double-click ends it), at the net's width by the
// board's rules, on the layer picked; the via tool places a via on a track's end; a drag of a via
// moves it with the track ends that meet there; a track that breaks the design rules is refused
// with the reason and comes back to be changed; and every edit of the routes file, R5a's delete
// included, is one step of undo and redo. Each edit is the routes file's, made as `netsmith
// routes add-track`, `add-via`, `delete` make it.
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
  // The refused track is a 422 on purpose, which the browser logs.
  allowConsole(/status of 422 .*\/api\/pcb\/routes/);
  const file = path.join(project, 'src/main.routes.toml');
  const text = () => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : '');
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
  // No grid: a corner goes where it is clicked, so the track drawn here stays straight enough for the rules.
  await page.selectOption('[data-testid=grid]', '0');
  await page.getByRole('button', { name: 'Fit' }).click();
  // The board is small: closer, so its tracks can be seen and clicked.
  for (let i = 0; i < 3; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);

  let s = await scene(page, BOARD);
  /** The next scene the server has once the routes file is written. */
  const written = async (what) => {
    const stamp = s.routesStamp;
    s = await waitFor(what, async () => {
      const now = await scene(page, BOARD);
      return now.routesStamp !== stamp ? now : null;
    });
    await whenIdle(page);
    return s;
  };
  const vcc = () => s.routes.find((n) => n.net === 'VCC');
  const pads = s.parts.flatMap((p) => p.pads).filter((p) => p.net === 'VCC');
  expect(pads.length === 2 && !vcc(), 'VCC has two pads and no record: the router routes it');
  const width = s.netWidths.find((w) => w.net === 'VCC').width;
  const at = (p) => screenAt(page, p.x, p.y);
  const mid = { x: (pads[0].x + pads[1].x) / 2, y: (pads[0].y + pads[1].y) / 2 };

  step('X takes the track tool; a click on a pad starts a track of its net, each click adds a corner');
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-options]');
  await page.selectOption('[data-testid=track-layer]', 'bottom');
  let p = await at(pads[0]);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  p = await at(mid);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-points="2"]');
  const end = await at(pads[1]);
  await page.mouse.move(end.x, end.y);
  await whenIdle(page);
  expect((await page.locator('[data-testid=tool-track][aria-checked=true]').count()) === 1, 'the toolbar shows the track tool on');
  await shot('track-draft');

  step('a double-click on the other pad ends it: one edit of the routes file, at the net width, on the bottom');
  await page.mouse.dblclick(end.x, end.y);
  await written('the track to be written');
  expect(vcc()?.tracks.length === 2, `VCC's record has the two segments (${vcc()?.tracks.length})`);
  expect(vcc().tracks.every((t) => t.layer === 'bottom' && t.width === width && t.drawn), `on the bottom at ${width} mm, drawn`);
  expect(/\[LedExample\.nets\.VCC\]\npads = "[0-9a-f]{16}"\ntracks = \[\n {2}\{ layer = "bottom"/.test(text()), 'the file has the record with its pad hash, as `routes add-track` writes it');
  expect((await page.locator('[data-testid=track-draft]').count()) === 0, 'the draft is gone');
  await page.locator('.toast', { hasText: 'added a track to net VCC (2 times)' }).first().waitFor();
  await shot('track-drawn');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  await shot('track-drawn-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('Ctrl+Z takes the track away, Ctrl+Shift+Z puts it back');
  const drawn = text();
  await page.keyboard.press('Control+z');
  await written('the track to be undone');
  expect(!vcc() && !text().includes('nets.VCC'), 'no VCC record in the scene or the file');
  await page.locator('.toast', { hasText: 'Undid: draw a track of 2 segments on net VCC' }).waitFor();
  await page.keyboard.press('Control+Shift+z');
  await written('the track to be redone');
  expect(text() === drawn, 'the file is as the drawing left it');

  step('V takes the via tool; a click on the corner places a via of the net there');
  await page.keyboard.press('v');
  await whenIdle(page);
  await page.waitForSelector('.pcb-host.tool-via');
  const corner = vcc().tracks[0].to;
  p = await at(corner);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await written('the via to be written');
  const via = vcc().vias[0];
  expect(via && Math.hypot(via.at.x - corner.x, via.at.y - corner.y) < 1e-3, 'the via is on the corner');
  expect(via.diameter === s.viaDiameter && via.drill === s.viaDrill, `of the process's size (${via.diameter} mm, drill ${via.drill} mm)`);
  await page.keyboard.press('Escape');
  await whenIdle(page);
  await shot('via-placed');

  step('a drag of the via moves it, and the ends of the two tracks that meet there go with it');
  const to = { x: pads[0].x + 0.4 * (pads[1].x - pads[0].x), y: pads[0].y + 0.4 * (pads[1].y - pads[0].y) };
  p = await at(via.at);
  const q = await at(to);
  await page.mouse.move(p.x, p.y);
  await page.mouse.down();
  await page.mouse.move((p.x + q.x) / 2, (p.y + q.y) / 2);
  await page.mouse.move(q.x, q.y);
  await page.waitForSelector('.fixed-net.dragging[data-fixed-net="VCC"]');
  await shot('via-dragging');
  await page.mouse.up();
  await written('the drag to be written');
  const moved = vcc();
  const near = (a, b) => Math.hypot(a.x - b.x, a.y - b.y) < 0.05;
  expect(near(moved.vias[0].at, to) && near(moved.tracks[0].to, to) && near(moved.tracks[1].from, to), 'the via and both track ends are where it was dropped');
  await page.locator('.toast', { hasText: 'moved track 1 of net VCC; moved track 2 of net VCC; moved via 1 of net VCC' }).first().waitFor();
  await shot('via-moved');
  await page.keyboard.press('Control+z');
  await written('the drag to be undone');
  expect(near(vcc().vias[0].at, corner) && near(vcc().tracks[0].to, corner), 'undo puts the via and the ends back on the corner');

  step('a track from a VCC pad onto a GND pad: the walkaround refuses the click on the top, and the check refuses the track at its end');
  const gnd = s.parts.flatMap((x) => x.pads).find((x) => x.net === 'GND');
  const before = text();
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.selectOption('[data-testid=track-layer]', 'top');
  p = await at(pads[1]);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  p = await at(gnd);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.locator('.toast', { hasText: 'no room for a track there' }).first().waitFor();
  expect((await page.locator('[data-testid=track-draft]').getAttribute('data-points')) === '1', 'the click on the pad of another net added no corner (the walkaround, issue #726)');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=track-draft]', { state: 'detached' });
  // The bottom has no copper under the ground pad (a surface-mount pad): the walkaround lets the track end there. The layer is
  // changed to the top before the track is ended, so the check, which has the last word, refuses it.
  await page.selectOption('[data-testid=track-layer]', 'bottom');
  p = await at(pads[1]);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-net="VCC"]');
  p = await at(gnd);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-points="2"]');
  await page.selectOption('[data-testid=track-layer]', 'top');
  await whenIdle(page);
  await page.keyboard.press('Enter');
  await page.locator('.toast', { hasText: 'the design rules check finds' }).first().waitFor();
  await page.waitForSelector('[data-testid=track-draft][data-points="2"]');
  await whenIdle(page);
  expect(text() === before, 'the routes file is as it was');
  await shot('track-refused');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid=track-draft]', { state: 'detached' });
  await page.keyboard.press('Escape');
  await page.waitForSelector('.pcb-host.tool-select');

  step('a delete is a step of undo too: undo brings the shape back where it was in the list');
  const anode = () => s.routes.find((n) => n.net === 'net-led.anode');
  const tracks = anode().tracks;
  const t1 = tracks[0];
  p = await screenAt(page, (t1.from.x + t1.to.x) / 2, (t1.from.y + t1.to.y) / 2);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=route-inspector]');
  await page.keyboard.press('Delete');
  await written('the track to be deleted');
  expect(anode().tracks.length === tracks.length - 1, 'one track fewer');
  await page.locator('[data-testid=undo]').click();
  await written('the delete to be undone');
  expect(JSON.stringify(anode().tracks) === JSON.stringify(tracks), 'the record is as it was, track 1 first again');
  await page.locator('.toast', { hasText: 'Undid: delete track 1 of net net-led.anode' }).waitFor();
  await whenIdle(page);
  await shot('delete-undone');
}
