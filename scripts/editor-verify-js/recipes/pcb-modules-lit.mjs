// All the parts of a module lit on the board, in the 2D and 3D views (issue 1129). The board `Lights` has three
// modules, `a`, `b` and `c` (an LED and its resistor each). Hovering a row of the Modules tab lights the module's parts
// in both views and a click keeps them lit; ctrl and a click adds a second module in a second colour, and a click on
// one lit takes it off. The part inspector lights a part's module. A module picked in another view (the page's one
// selection, as a module block of the schematic picks it) is lit on the board. The page's state (`GET /api/view`) says which modules are lit, with their colours, and `view editor-3d
// --highlight module:<path>` lights them from the command line; a module the board has not is refused with the list.
import { execFile } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { guarded, whenIdle } from '../settle.mjs';
import { fresh, openPcb, partAt, scene } from '../pcb-helpers.mjs';

export const fixture = 'lights';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lights';
const HOST = '[data-testid=pcb-3d]';
const exec = promisify(execFile);

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

export async function run(tools) {
  const { page, shot, step, expect, url, binary } = tools;
  await fresh(tools, path.join(here, '../fixtures/lights.pcb.toml'));
  const api = new URL('/api/view', url).href;
  const state = async () => (await guarded('GET /api/view', fetch(api))).json();
  const post = async (change) => {
    const r = await guarded('POST /api/view', fetch(api, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Netsmith': '1' }, body: JSON.stringify(change) }));
    const text = await r.text();
    return { status: r.status, body: r.headers.get('content-type')?.includes('json') ? JSON.parse(text) : text };
  };
  const view = async (...args) => {
    try {
      const r = await guarded('netsmith view editor-3d', exec(binary, ['view', 'editor-3d', '--port', new URL(url).port, ...args]));
      return { code: 0, out: r.stdout, err: r.stderr };
    } catch (e) {
      if (e?.name === 'Hung') throw e;
      return { code: e.code, out: e.stdout ?? '', err: e.stderr ?? '' };
    }
  };
  const s = await scene(page, BOARD);
  const partsOf = (m) => s.parts.filter((p) => p.path.startsWith(`${m}.`)).map((p) => p.path).sort();
  await openPcb(page, BOARD);
  await page.getByRole('button', { name: 'Fit' }).click();
  await whenIdle(page);
  const groups = () =>
    page.locator('.pcb-overlay .module-group').evaluateAll((gs) =>
      gs.map((g) => ({ module: g.getAttribute('data-module'), slot: g.getAttribute('data-slot'), parts: g.querySelectorAll('.module-part').length, hovered: g.classList.contains('hovered'), colour: getComputedStyle(g.querySelector('.module-part')).stroke })),
    );
  const dimmed = () => page.locator('.pcb-host svg.has-mod g[data-path]:not(.mod-lit)').count();
  const row = (m) => page.locator(`[data-testid=module-tree] .module-row[data-module="${m}"]`);

  step('the Modules tab: hovering a row lights its module on the board, a click keeps it lit');
  await page.getByRole('button', { name: /^Modules \(3\)$/ }).click();
  await page.locator('[data-testid=module-tree]').waitFor();
  await row('a').hover();
  await whenIdle(page);
  let g = await groups();
  expect(g.length === 1 && g[0].module === 'a' && g[0].hovered && g[0].parts === partsOf('a').length, `the hovered module is drawn: ${JSON.stringify(g)}`);
  expect((await dimmed()) === s.parts.length - partsOf('a').length, 'and the other parts step back');
  await shot('hover-a');
  await row('a').click();
  await page.mouse.move(5, 5);
  await whenIdle(page);
  g = await groups();
  expect(g.length === 1 && g[0].module === 'a' && !g[0].hovered && g[0].slot === '1', `a stays lit once the pointer leaves: ${JSON.stringify(g)}`);
  expect((await row('a').getAttribute('data-lit')) === '1', 'its row says it is lit');
  let st = await state();
  expect(JSON.stringify(st.modules.map((m) => m.path)) === '["a"]' && st.highlight.includes('module:a'), `the page state says a is lit: ${JSON.stringify(st.modules)} ${JSON.stringify(st.highlight)}`);
  expect(st.modules[0].parts.length === partsOf('a').length && /^#[0-9a-f]{6}$/.test(st.modules[0].colour), `with its parts and colour: ${JSON.stringify(st.modules[0])}`);
  expect(JSON.stringify(st.selection) === '["module:a"]', `the page's selection is the module: ${JSON.stringify(st.selection)}`);

  step('ctrl and a click adds b in a second colour; a click on one lit takes it off');
  await row('b').click({ modifiers: ['Control'] });
  await page.mouse.move(5, 5);
  await whenIdle(page);
  g = await groups();
  expect(g.length === 2 && g[1].module === 'b' && g[1].slot === '2' && g[0].colour !== g[1].colour, `two modules, two colours: ${JSON.stringify(g)}`);
  st = await state();
  expect(st.modules.map((m) => m.path).join(',') === 'a,b' && st.modules[0].colour !== st.modules[1].colour, `the page state lists both, in order: ${JSON.stringify(st.modules)}`);
  await shot('two-modules-2d');
  await row('a').click();
  await page.mouse.move(5, 5);
  await whenIdle(page);
  g = await groups();
  expect(g.length === 1 && g[0].module === 'b' && g[0].slot === '2', `a is off and b keeps its colour: ${JSON.stringify(g)}`);
  await page.locator('[data-testid=modules-clear]').click();
  await whenIdle(page);
  expect((await groups()).length === 0 && (await dimmed()) === 0, 'Clear puts every module out');

  step('the part inspector lights the part\'s module');
  await page.getByRole('button', { name: 'Fit' }).click();
  await whenIdle(page);
  const led = await partAt(page, 'c.led', BOARD);
  await page.mouse.click(led.centre.x, led.centre.y);
  await page.locator('[data-testid=light-module-btn]').waitFor();
  await page.locator('[data-testid=light-module-btn]').click();
  await page.mouse.move(5, 5);
  await whenIdle(page);
  g = await groups();
  expect(g.length === 1 && g[0].module === 'c' && g[0].parts === partsOf('c').length, `c is lit from its part: ${JSON.stringify(g)}`);
  expect((await page.locator('[data-testid=light-module-btn]').getAttribute('aria-pressed')) === 'true', 'the action says it is on');
  await shot('inspector-c');

  step('two modules in the 3D view: each body in its module\'s colour, from the command line');
  let r = await view('--highlight', 'module:a,module:b');
  expect(r.code === 0 && r.out.includes('highlight: module:a, module:b') && /module a: #[0-9a-f]{6}/.test(r.out) && /module b: #[0-9a-f]{6}/.test(r.out), `view editor-3d lights two modules:\n${r.out}${r.err}`);
  await page.locator(`${HOST}[data-ready]`).waitFor();
  await whenIdle(page);
  const tints = await ask(page, 'moduleTints');
  const colourOf = (p) => tints.find((t) => t.path === p)?.colour;
  const [a0, b0] = [partsOf('a')[0], partsOf('b')[0]];
  expect(tints.length === partsOf('a').length + partsOf('b').length, `the 3D view tints exactly their parts: ${JSON.stringify(tints)}`);
  expect(colourOf(a0) && colourOf(b0) && colourOf(a0) !== colourOf(b0) && partsOf('a').every((p) => colourOf(p) === colourOf(a0)), `in two colours: ${JSON.stringify(tints)}`);
  st = await state();
  expect(st.modules.map((m) => m.colour).join(',') === [colourOf(a0), colourOf(b0)].join(','), `the page state has the colours the view draws: ${JSON.stringify(st.modules)}`);
  expect(st.command.includes('module:a') && st.command.includes('module:b'), `the view 3d command lights them too: ${st.command}`);
  await shot('two-modules-3d');

  step('a module the board has not is refused with the list, and nothing changes');
  r = await view('--highlight', 'module:nope');
  expect(r.code === 2 && r.err.includes('`module:nope`') && r.err.includes('the modules: module:a, module:b, module:c'), `refused with the list: ${r.err.trim()}`);
  expect((await state()).modules.length === 2, 'the two modules are still lit');
  const sel = await post({ select: ['module:c'] });
  expect(sel.status === 200 && JSON.stringify(sel.body.selection) === '["module:c"]' && sel.body.modules.map((m) => m.path).join(',') === 'c', `select module:c picks it and lights it: ${JSON.stringify(sel.body.modules)}`);
  r = await view('--highlight', 'none');
  expect(r.code === 0 && (await state()).modules.length === 0, '`--highlight none` puts the modules out');

  step('a module picked while the schematic is shown is lit on the board');
  // The board is one sheet, so the schematic picks a module with go to anything (on a board of several sheets, a card
  // of the overview picks it): the page's one selection, the same as a module block picked.
  await page.locator('.pane-strip [data-section="schematic"]').first().click();
  await page.waitForSelector('.schematic');
  await page.keyboard.press('Control+p');
  await page.waitForSelector('.qp[role=dialog]');
  await page.keyboard.type('b');
  await page.waitForFunction(() => {
    const row = document.querySelector('.qp-row[role=option][aria-selected=true]');
    return !!row && row.querySelector('.qp-label')?.textContent === 'b' && !!row.querySelector('.qp-detail')?.textContent?.startsWith('module');
  });
  await page.keyboard.press('Enter');
  await page.waitForSelector('.qp[role=dialog]', { state: 'detached' });
  await whenIdle(page);
  st = await state();
  expect(JSON.stringify(st.selection) === '["module:b"]' && st.modules.map((m) => m.path).join(',') === 'b', `the module picked in the schematic is lit: ${JSON.stringify(st.selection)} ${JSON.stringify(st.modules)}`);
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await whenIdle(page);
  await shot('from-schematic');
}
