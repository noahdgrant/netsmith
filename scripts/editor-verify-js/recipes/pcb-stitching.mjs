// The stitching vias the export will lay, drawn on the 2D PCB view (issue 1489): the board's pours state a stitch pitch, so `fab`
// stitches them to the other layers of their net, and the view draws those vias before any export, in a tone of their own (a
// dashed disc in `--pcb-stitching`), as not yet in the routes file. They are the server's (`GET /api/pcb/stitching`,
// `netsmith_router::stitching_vias`, the stitching `fab` runs, what `netsmith view pcb --stitching` draws). `Stitching` in the
// Layers menu hides them and brings them back.
import { whenIdle } from '../settle.mjs';
import { BOARD, fresh, openPcb, scene } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools);
  await openPcb(page);
  const sc = await scene(page);
  // The stitching is asked after the fill: wait for the vias of the placement drawn.
  await page.waitForSelector('[data-testid=stitching-vias][data-current=true]', { state: 'attached' });
  await whenIdle(page);

  step('the server lists the stitching vias of the placement drawn, and the page draws each where it says');
  const server = await page.evaluate(async (b) => (await fetch(`/api/pcb/stitching?board=${encodeURIComponent(b)}`)).json(), BOARD);
  expect(server.layoutStamp === sc.layoutStamp && server.routesStamp === sc.routesStamp, 'of the placement the scene draws');
  expect(server.vias.length > 0, `the stitched pours have vias (${server.vias.length})`);
  const drawn = page.locator('.l-stitching > circle.stitching-via');
  expect((await drawn.count()) === server.vias.length, `one disc for each (${await drawn.count()} of ${server.vias.length})`);
  const first = server.vias[0];
  const disc = await drawn.first().evaluate((c) => ({
    cx: Number(c.getAttribute('cx')),
    cy: Number(c.getAttribute('cy')),
    r: Number(c.getAttribute('r')),
    net: c.getAttribute('data-net'),
    dash: getComputedStyle(c).strokeDasharray,
    fill: getComputedStyle(c).fill,
  }));
  expect(Math.abs(disc.cx - first.x) < 1e-3 && Math.abs(disc.cy - first.y) < 1e-3 && Math.abs(disc.r - first.diameter / 2) < 1e-3, 'the first is where the server says, its size');
  expect(disc.net === first.net, `on its net (${disc.net})`);
  expect(disc.dash !== 'none', `dashed, as copper not yet in the routes file (${disc.dash})`);
  const via = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--pcb-via').trim());
  const tone = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--pcb-stitching').trim());
  expect(tone !== '' && tone !== via, `a tone apart from the vias' (${tone}, the vias ${via})`);
  await page.getByRole('button', { name: 'Fit' }).click();
  for (let i = 0; i < 2; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  await whenIdle(page);
  await shot('stitching');

  step('the Layers menu hides the stitching and brings it back, and leaves the other vias');
  const shown = (sel) => page.locator(sel).first().evaluate((e) => getComputedStyle(e).display !== 'none');
  await page.locator('details.layers summary').click();
  const toggle = async (label) => {
    await page.locator('.layer-list label', { hasText: label }).locator('input').click();
    await whenIdle(page);
  };
  await toggle('Stitching');
  expect(!(await shown('.l-stitching')), 'Stitching off: the discs go');
  await shot('stitching-off');
  await toggle('Stitching');
  expect(await shown('.l-stitching'), 'Stitching on: they come back');
  await toggle('Vias');
  expect(await shown('.l-stitching'), 'Vias off leaves the stitching');
  await page.locator('.layer-list button', { hasText: 'Show all' }).click();
  await whenIdle(page);
  await page.keyboard.press('Escape');
}
