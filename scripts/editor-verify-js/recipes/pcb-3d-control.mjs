// An agent drives the 3D view of a running editor over HTTP (`/api/view`, P8a of issue 430,
// docs/BOARD3D.md), with the names of `netsmith render`, while the page is open: the state is read
// with no page drawing 3D, a change opens the PCB section in 3D, the views, the section, the
// highlights (a part, a limit), x-ray, the lens, a camera place, the colouring, the height
// preview and the stackup apart are set and read back, the page shows each (asked of the view
// itself, and screenshot), the picture is the canvas, the copied command and the API agree, and a
// change the view refuses (an id the board has not, a board the project has not, no guard header)
// changes nothing.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'height_limits';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lidded';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

export async function run(tools) {
  const { page, shot, step, expect, url } = tools;
  await fresh(tools, path.join(here, '../fixtures/height_limits.pcb.toml'));
  const api = new URL('/api/view', url).href;
  // As an agent asks: from outside the page, with the guard header on a change.
  const get = async (route = api) => {
    const r = await fetch(route);
    return { status: r.status, body: r.headers.get('content-type')?.includes('json') ? await r.json() : await r.text() };
  };
  const post = async (change, headers = { 'X-Netsmith': '1' }) => {
    const r = await fetch(api, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(change) });
    const text = await r.text();
    let body = text;
    try {
      body = JSON.parse(text);
    } catch {
      // A refusal is text.
    }
    return { status: r.status, body };
  };
  const s = await scene(page, BOARD);
  const tall = partOf(s, 'tall');
  const flat = partOf(s, 'flat');

  step('with the page on the 2D view, GET /api/view says which board it is on and that 3D is not shown');
  await page.selectOption('.boardpick select', BOARD);
  let r = await get();
  expect(r.status === 200, `GET /api/view answers 200 (${r.status}: ${JSON.stringify(r.body)})`);
  expect(r.body.board === BOARD && r.body.shown === false && r.body.camera === undefined, `the state: ${JSON.stringify(r.body)}`);
  r = await get(`${api}/picture`);
  expect(r.status === 409 && /not on screen/.test(r.body), `no picture while 3D is not shown: ${r.status} ${r.body}`);

  step('POST {} opens the PCB section in 3D and answers once it is drawn: the isometric view, perspective');
  r = await post({});
  expect(r.status === 200 && r.body.shown === true, `the view is shown: ${r.status} ${JSON.stringify(r.body).slice(0, 300)}`);
  expect(await page.locator(`${HOST}[data-ready]`).count() === 1, 'the page draws the 3D view');
  expect(r.body.view === 'iso' && r.body.ortho === false, `it opened on iso, in perspective: view ${r.body.view}, ortho ${r.body.ortho}`);
  expect(r.body.command === (await ask(page, 'command')), `the state's command is the one "Copy as command" gives: ${r.body.command}`);
  await shot('opened-iso');

  step('view, highlight (a part and a limit) and x-ray, with the names of render, as the page shows them');
  r = await post({ view: 'top', highlight: [tall.refdes, 'limit:lid'], xray: true });
  expect(r.status === 200, `the change is made: ${r.status} ${JSON.stringify(r.body).slice(0, 300)}`);
  expect(r.body.view === 'top' && (await ask(page, 'preset')) === 'top', `the camera looks from the top: ${r.body.view}`);
  expect(JSON.stringify(r.body.highlight) === JSON.stringify([tall.refdes, 'limit:lid']), `what is lit, named as render names it: ${r.body.highlight}`);
  expect(JSON.stringify(r.body.selection) === JSON.stringify([tall.refdes]), `the part is the page's selection: ${r.body.selection}`);
  expect((await ask(page, 'focused')).some((k) => k.startsWith('lid:')), 'the limit lid is lit in the view');
  expect(r.body.xray === true && (await page.getByTestId('view-xray').getAttribute('aria-pressed')) === 'true', 'x-ray is on, and its button says so');
  expect(r.body.command.includes('--view top') && r.body.command.includes(`--highlight ${tall.refdes},limit:lid`), `the copied command and the API agree: ${r.body.command}`);
  expect(r.body.camera.up.every((v) => Number.isFinite(v)) && r.body.camera.target.length === 3, `the camera in the scene's frame: ${JSON.stringify(r.body.camera)}`);
  const topShot = await shot('top-lit-xray');

  step("GET /api/view/picture is a PNG of the page's canvas as it is drawn now");
  const pic = await fetch(`${api}/picture`);
  expect(pic.status === 200 && pic.headers.get('content-type') === 'image/png', `a PNG: ${pic.status} ${pic.headers.get('content-type')}`);
  const png = Buffer.from(await pic.arrayBuffer());
  expect(png.subarray(1, 4).toString() === 'PNG', 'the bytes are a PNG');
  const [w, h] = [png.readUInt32BE(16), png.readUInt32BE(20)];
  const canvas = await page.evaluate((sel) => {
    const c = document.querySelector(`${sel} canvas`);
    return { w: c.width, h: c.height };
  }, HOST);
  expect(w === canvas.w && h === canvas.h, `the picture is the canvas's size: ${w}x${h} (the canvas is ${canvas.w}x${canvas.h})`);
  const picFile = path.join(path.dirname(topShot), 'pcb-3d-control-picture-top.png');
  fs.writeFileSync(picFile, png);
  console.log(`     picture ${picFile}`);
  expect(png.length > 5000, `the picture has something in it (${png.length} bytes)`);

  step('a section, the front view and the orthographic lens, read back as render writes them');
  const mid = Math.round((s.outline.width / 2) * 10) / 10;
  r = await post({ view: 'front', section: `x=${mid}`, ortho: true, xray: false, highlight: [] });
  expect(r.status === 200 && r.body.section === `x=${mid}` && r.body.ortho === true && r.body.view === 'front', `the cut and the lens: ${r.body.section}, ortho ${r.body.ortho}, ${r.body.view}`);
  expect((await ask(page, 'ortho')) === true && (await page.getByTestId('section-panel').count()) === 1, 'the page is orthographic and its section panel is open');
  expect(r.body.highlight.length === 0 && r.body.command.includes(`--section x=${mid}`) && !r.body.command.includes('--perspective'), `nothing lit, and the command has the cut: ${r.body.command}`);
  await shot('front-section-ortho');

  step('the camera put at a place, and read back where it was put');
  const target = [s.outline.width / 2, s.outline.height / 2, 0];
  const position = [target[0] - 30, target[1] - 40, 35];
  r = await post({ camera: { position, target }, section: 'none', perspective: true });
  const near = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 0.05);
  expect(r.status === 200 && near(r.body.camera.position, position) && near(r.body.camera.target, target), `the camera is where it was put: ${JSON.stringify(r.body.camera)}`);
  expect(r.body.view === undefined && r.body.section === undefined, `no view of render matches it, and the cut is gone (${r.body.view}, ${r.body.section})`);
  await shot('camera-placed');

  step('colour by margin, the height preview, iso-bottom and the selection through select');
  // The fixture is not routed: its scene has no copper, so it has no net to light, and naming one says so as render does.
  const nets = await ask(page, 'nets');
  r = await post({ select: ['net:GND'] });
  expect(nets.length > 0 || (r.status === 400 && r.body.includes('the nets: (none)')), `a net the scene has not is refused, with the nets it has: ${r.status} ${typeof r.body === 'string' ? r.body : ''}`);
  const pick = nets.length ? `net:${nets[0]}` : tall.refdes;
  r = await post({ view: 'iso-bottom', colourBy: 'margin', heightPreview: 4, select: [pick] });
  expect(r.status === 200 && r.body.colourBy === 'margin' && r.body.heightPreview === 4, `the colouring and the preview: ${r.status} ${r.body.colourBy}, ${r.body.heightPreview}`);
  expect((await page.getByTestId('colour-by').inputValue()) === 'margin' && (await ask(page, 'previewCount')) === 1, 'the page colours by margin and stands the part of unknown height up');
  expect((await page.getByTestId('preview-label').count()) === 1, 'the preview label says it is a preview');
  expect(r.body.view === 'iso-bottom' && JSON.stringify(r.body.selection) === JSON.stringify([pick]), `seen from below, ${pick} picked: ${r.body.view}, ${r.body.selection}`);
  await shot('iso-bottom-margin-preview');

  step('the stackup pulled apart and put back');
  r = await post({ exploded: true, heightPreview: false, colourBy: 'provenance', select: [flat.refdes], view: 'right' });
  if (r.status === 409) {
    expect(/no stackup/.test(r.body), `a board with no stackup says so: ${r.body}`);
  } else {
    expect(r.status === 200 && r.body.exploded === true && (await ask(page, 'exploded')) === true, `the layers are apart: ${r.status}`);
    await shot('exploded-right');
    r = await post({ exploded: false });
    expect(r.body.exploded === false && (await ask(page, 'exploded')) === false, 'and back together');
  }

  step('a change the view refuses changes nothing, and says why as render does');
  const before = (await get()).body;
  r = await post({ view: 'top', highlight: ['NOPE', 'net:NOPE'] });
  expect(r.status === 400 && r.body.includes('`NOPE`, `net:NOPE`') && r.body.includes('the parts are:') && r.body.includes(tall.refdes) && r.body.includes('limit:lid'), `an unknown id lists what the board has: ${r.body}`);
  const after = (await get()).body;
  expect(after.view === before.view && JSON.stringify(after.camera) === JSON.stringify(before.camera) && JSON.stringify(after.selection) === JSON.stringify(before.selection), 'the camera and the selection did not move');
  r = await post({ board: 'NoSuchBoard' });
  expect(r.status === 404 && r.body.includes(BOARD) && !r.body.includes('system'), `a board the project has not (it has no system to list): ${r.status} ${r.body}`);
  // A member's id is a system's; on a board alone it is an id the board has not.
  r = await post({ highlight: [`lid/${tall.refdes}`] });
  expect(r.status === 400 && r.body.includes('that the board does not have') && r.body.includes(`\`lid/${tall.refdes}\``), `a member's id on a board alone: ${r.body}`);
  r = await post({ theme: 'dark' });
  expect(r.status === 400 && r.body.includes("person's own setting"), `the theme is not changed: ${r.body}`);
  r = await post({ view: 'top' }, {});
  expect(r.status === 403, `a change without the guard header is refused: ${r.status}`);
  r = await fetch(api, { headers: { Origin: 'https://evil.example' } });
  expect(r.status === 403, `another origin may not read the view: ${r.status}`);
  await shot('after-refusals');
}
