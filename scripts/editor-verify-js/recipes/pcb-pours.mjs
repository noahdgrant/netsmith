// The pours and zones in the 2D PCB view (epic #599): the board's own pours and planes and the zones of
// the routes file are drawn under the tracks, pads and vias, one fill for each net on each layer, in
// the colour of the layer (the front's, the back's, a plane's tone) with transparency, and go with
// the Layers menu's switch of their copper. A hole is a hole, a zone's priority gives the area to
// the higher one, a pointer goes through the fill, and the fill is as readable in the light theme
// as in the dark one. The fill is the server's (`GET /api/pcb/pours`), the one the Gerbers hold.
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
  // The board is small in the window: the screenshots show it larger.
  const zoomIn = async () => {
    for (let i = 0; i < 3; i++) await page.getByRole('button', { name: 'Zoom in' }).click();
  };
  const stampOf = () => {
    const on = fs.statSync(file, { bigint: true });
    return `${on.mtimeNs}-${on.size}`;
  };
  // Every run starts from the fixture's file, whatever ran before.
  fs.copyFileSync(path.join(here, '../fixtures/routed_pours.routes.toml'), file);
  await page.goto(url);
  await page.waitForSelector('.app');
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === stampOf());
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  // The fill comes after the board is drawn: wait for the one that is the scene's own.
  const fill = '[data-testid=pours][data-current=true]';
  await page.waitForSelector(fill, { state: 'attached' });
  await whenIdle(page);
  await page.getByRole('button', { name: 'Fit' }).click();
  await zoomIn();
  await whenIdle(page);

  step('the server fills each layer that has a pour, plane or zone: the planes, the back pour and the front zones');
  const answer = await page.evaluate(async (b) => (await fetch(`/api/pcb/pours?board=${b}`)).json(), BOARD);
  const s = await scene(page, BOARD);
  expect(answer.error === null, `the board fills (${answer.error})`);
  expect(answer.layers.map((l) => l.layer).join() === 'F.Cu,In1.Cu,In2.Cu,B.Cu', `the layers filled are ${answer.layers.map((l) => l.layer)}`);
  const netsOn = (layer) => answer.layers.find((l) => l.layer === layer).nets.map((n) => n.net).sort().join();
  expect(netsOn('F.Cu') === 'GND,net-led.anode', `the front has the ground zone and the anode zone (${netsOn('F.Cu')})`);
  expect(netsOn('In1.Cu') === 'GND' && netsOn('In2.Cu') === 'VCC' && netsOn('B.Cu') === 'GND', 'the planes and the back pour are those of the design');
  expect(answer.layoutStamp === s.layoutStamp && answer.routesStamp === s.routesStamp, 'the answer names the placement it filled');

  step('the page draws one path for each net on each layer, in a group of its own');
  const total = answer.layers.reduce((n, l) => n + l.nets.length, 0);
  expect((await page.locator('.l-pours > path.pour').count()) === total, `${total} paths drawn`);
  const sides = await page.locator('.l-pours > path.pour').evaluateAll((all) => all.map((p) => `${p.getAttribute('data-layer')}:${p.getAttribute('data-net')}:${p.getAttribute('class')}`));
  expect(sides.includes('F.Cu:GND:pour pour-front') && sides.includes('B.Cu:GND:pour pour-back') && sides.includes('In1.Cu:GND:pour pour-inner') && sides.includes('In2.Cu:VCC:pour pour-inner'), `each is of its side (${sides.join(', ')})`);
  expect((await page.locator('.l-pours > path.pour-inner[data-plane="2"][data-layer="In2.Cu"]').count()) === 1, 'the second inner layer has the second plane tone');

  step('it is under the pads, the tracks, the vias and the labels, and the pointer goes through it');
  const order = await page.evaluate(() => {
    const svg = document.querySelector('.pcb-host svg');
    const pours = svg.querySelector('.l-pours');
    const after = (sel) => [...svg.querySelectorAll(sel)].every((e) => !!(pours.compareDocumentPosition(e) & Node.DOCUMENT_POSITION_FOLLOWING));
    return {
      pads: after('.l-pads'),
      tracks: after('.l-front-copper, .l-back-copper'),
      vias: after('.l-vias'),
      labels: after('.l-refs, .l-plane-labels'),
      outline: !!(svg.querySelector('.l-outline').compareDocumentPosition(pours) & Node.DOCUMENT_POSITION_FOLLOWING),
      events: [...svg.querySelectorAll('.l-pours > path')].every((p) => getComputedStyle(p).pointerEvents === 'none'),
    };
  });
  expect(order.outline, 'it is over the board');
  expect(order.pads && order.tracks && order.vias && order.labels, `every pad, track, via and label is drawn after it (${JSON.stringify(order)})`);
  expect(order.events, 'no fill takes the pointer');

  step('a hole is a hole, a zone of higher priority has the area, and what is outside a zone has no fill');
  const filled = (layer, net, x, y) =>
    page.evaluate(
      ([l, n, px, py]) => {
        const p = document.querySelector(`.l-pours > path[data-layer="${l}"][data-net="${n}"]`);
        return p.isPointInFill(new DOMPoint(px, py));
      },
      [layer, net, s.outline.x + x, s.outline.y + y],
    );
  expect(await filled('F.Cu', 'GND', 2.0, 5.4), 'the ground zone is filled in its corner');
  expect(!(await filled('F.Cu', 'GND', 5.0, 5.1)), "the ground zone's hole is empty");
  expect(!(await filled('F.Cu', 'GND', 11.0, 5.2)) && (await filled('F.Cu', 'net-led.anode', 11.0, 5.2)), "where the anode zone is, the ground zone gives way to it (priority)");
  expect(!(await filled('F.Cu', 'GND', 0.3, 0.3)), 'the margin outside the zone has no fill');
  expect(await filled('B.Cu', 'GND', 0.8, 0.8) || (await filled('B.Cu', 'GND', 1.5, 3.4)), 'the back pour fills the board inside its edge');

  step('it is the layer\'s colour with transparency, and the colour is the theme\'s');
  const paint = () =>
    page.evaluate(() => {
      const of = (sel) => {
        const c = getComputedStyle(document.querySelector(sel));
        return { fill: c.fill, opacity: Number(c.fillOpacity) };
      };
      return { front: of('.pour-front'), back: of('.pour-back'), plane1: of('.pour-inner[data-plane="1"]'), plane2: of('.pour-inner[data-plane="2"]') };
    });
  const light = await paint();
  const fills = Object.values(light).map((p) => p.fill);
  expect(new Set(fills).size === 4, `each layer has a colour of its own (${fills.join(' ')})`);
  expect(Object.values(light).every((p) => p.opacity > 0.1 && p.opacity < 0.6), 'transparent: the board and the other layers show through');
  await shot('pours-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await whenIdle(page);
  const dark = await paint();
  expect(Object.keys(light).every((k) => light[k].fill !== dark[k].fill), `each takes the dark theme's own colour (${light.front.fill} in light, ${dark.front.fill} in dark)`);
  await shot('pours-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await whenIdle(page);

  step('the Layers menu switches the fill with its copper: front, back and planes');
  const shown = (sel) => page.locator(sel).first().evaluate((e) => getComputedStyle(e).display !== 'none');
  await page.locator('details.layers summary').click();
  const toggle = async (label) => {
    await page.locator('.layer-list label', { hasText: label }).locator('input').click();
    await whenIdle(page);
  };
  await toggle('Front copper');
  expect(!(await shown('.pour-front')) && (await shown('.pour-back')) && (await shown('.pour-inner')), 'front copper off: only the front fill goes');
  await shot('pours-front-off');
  await toggle('Front copper');
  await toggle('Back copper');
  expect((await shown('.pour-front')) && !(await shown('.pour-back')) && (await shown('.pour-inner')), 'back copper off: only the back fill goes');
  await toggle('Back copper');
  await toggle('Planes');
  expect((await shown('.pour-front')) && (await shown('.pour-back')) && !(await shown('.pour-inner')), 'planes off: the planes\' fill goes');
  await page.locator('.layer-list button', { hasText: 'Show all' }).click();
  await whenIdle(page);
  expect((await shown('.pour-front')) && (await shown('.pour-back')) && (await shown('.pour-inner')), 'show all brings every fill back');
  await page.keyboard.press('Escape');

  step('the fill follows the routes file: a zone taken out is gone from the board, and the fill is drawn again');
  const text = fs.readFileSync(file, 'utf8');
  const at = text.indexOf('\n[[LedExample.nets."net-led.anode".zones]]');
  const without = at < 0 ? text : `${text.slice(0, at)}\n`;
  expect(without !== text, 'the anode zone is taken out of the file');
  fs.writeFileSync(file, without);
  const want = stampOf();
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === want);
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector(fill, { state: 'attached' });
  await whenIdle(page);
  const gone = await page.locator('.l-pours > path[data-layer="F.Cu"]').evaluateAll((all) => all.map((p) => p.getAttribute('data-net')).join());
  expect(gone === 'GND', `the front has the ground zone only (${gone})`);
  expect(await filled('F.Cu', 'GND', 11.0, 5.2), 'and it fills where the anode zone was');
  await page.getByRole('button', { name: 'Fit' }).click();
  await zoomIn();
  await whenIdle(page);
  await shot('zone-taken-out');
}
