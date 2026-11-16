// The track tool on an inner signal layer (issue 833): the layer picker lists the outer layers and
// the process's signal layers (not its planes); a track drawn on `inner2` from a through-hole pad
// is written to the routes file on that layer, at the net's width there, as `netsmith routes
// add-track --layer inner2` writes it, and the draft is drawn in the inner layer's tone.
import fs from 'node:fs';
import path from 'node:path';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, screenAt } from '../pcb-helpers.mjs';

export const fixture = 'six_layer';
const BOARD = 'SixLayerExample';

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
  for (let i = 0; i < 2; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);

  let s = await scene(page, BOARD);
  expect(JSON.stringify(s.trackLayers) === JSON.stringify(['top', 'inner2', 'inner3', 'bottom']), `the track layers are the outer and signal layers (${s.trackLayers})`);

  step('X takes the track tool; its layer picker offers the signal layers and no plane');
  await page.keyboard.press('x');
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-options]');
  const options = await page.locator('[data-testid=track-layer] option').evaluateAll((os) => os.map((o) => o.value));
  expect(JSON.stringify(options) === JSON.stringify(['top', 'inner2', 'inner3', 'bottom']), `the picker lists ${options}`);
  await page.selectOption('[data-testid=track-layer]', 'inner2');
  await whenIdle(page);

  step('a track from a VIN pad of the input header, on inner2, is drawn in the inner tone');
  const header = s.parts.find((p) => p.path.endsWith('power_in'));
  expect(header, 'the input header is on the board');
  const pad = header.pads.find((p) => p.net === 'VIN');
  // 1.5 mm straight towards the nearer of the top and bottom edges.
  const up = pad.y < s.outline.y + s.outline.height / 2;
  const to = { x: pad.x, y: pad.y + (up ? -1.5 : 1.5) };
  let p = await screenAt(page, pad.x, pad.y);
  await page.mouse.click(p.x, p.y);
  await whenIdle(page);
  await page.waitForSelector('[data-testid=track-draft][data-net="VIN"]');
  const end = await screenAt(page, to.x, to.y);
  await page.mouse.move(end.x, end.y);
  await whenIdle(page);
  expect((await page.locator('.track-draft.layer-inner').count()) === 1, 'the draft has the inner layer\'s class');
  await shot('inner-draft');

  step('a double-click ends it: the routes file has it on inner2 at the net\'s width there');
  const stamp = s.routesStamp;
  await page.mouse.dblclick(end.x, end.y);
  s = await waitFor('the track to be written', async () => {
    const now = await scene(page, BOARD);
    return now.routesStamp !== stamp ? now : null;
  });
  await whenIdle(page);
  const vin = s.routes.find((n) => n.net === 'VIN');
  const width = s.netWidths.find((w) => w.net === 'VIN');
  expect(vin && vin.tracks.length >= 1, `VIN has a record with the track (${vin?.tracks.length})`);
  expect(
    vin.tracks.every((t) => t.layer === 'inner2' && Math.abs(t.width - (width.inner_width ?? width.width)) < 1e-9),
    `on inner2 at ${width.inner_width} mm (${JSON.stringify(vin.tracks.map((t) => [t.layer, t.width]))})`,
  );
  expect(/layer = "inner2"/.test(fs.readFileSync(file, 'utf8')), 'the file says inner2, as `routes add-track` writes it');
  await shot('inner-drawn');
}
