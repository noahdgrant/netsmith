// The system scene in the 3D view (ADR 0110, features/pcb-3d.md): on a board of a system, the 3D
// view's list has the system above its members; choosing it draws every board placed against the
// others by their mates, with a chip that says how many boards and the mated height. A click on a
// part of another member makes that member the page's board (its 2D board and schematic follow)
// and selects the part there, while the 3D view stays on the system. A finding across the boards
// (a header made too tall for the stack) is listed, and a click on it flies the camera to it.
// The camera stays where it is as the page's board changes. The tools work across the members: a
// net is its member's (`led/<net>`) and is lit on that board alone; the section cuts every board;
// the layers come apart board by board, the boards apart with them; and a board can be hidden.
// An agent asks for the System view by the system's name (`/api/view`, `netsmith view --board
// TwoBoards`) with a member's ids (`led/J1`, `net:led/GND`), and `render --board TwoBoards`
// draws what the page's command says.
import fs from 'node:fs';
import path from 'node:path';

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

import { openPcb } from '../pcb-helpers.mjs';

const exec = promisify(execFile);

export const fixture = 'two_boards';

const HOST = '[data-testid=pcb-3d]';
const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);
const ready = (page) => page.waitForSelector(`${HOST}[data-ready]`, { timeout: 60000 });
/** Wait until the 3D view draws the system: a part of each member, by `member/path`. */
const systemDrawn = (page) =>
  page.waitForFunction(
    (sel) => {
      const parts = document.querySelector(sel)?.view3d?.parts?.() ?? [];
      return parts.some((p) => p.path.startsWith('led/')) && parts.some((p) => p.path.startsWith('power/'));
    },
    HOST,
    { timeout: 60000 },
  );
/** Wait until the camera has stopped moving (a preset's move eases over several frames, and a
 * frame of a system's scene in a software renderer is slow). */
async function settle(page) {
  let last = '';
  for (let i = 0; i < 60; i++) {
    const now = JSON.stringify(await ask(page, 'camera'));
    if (now === last) return;
    last = now;
    await page.waitForTimeout(150);
  }
}
/** Where a fixed point of the scene (the base's origin) is on screen: it moves when the camera does. */
const origin = (page) => ask(page, 'toScreen', [0, 0, 0]);

export async function run({ page, url, project, shot, step, expect, binary }) {
  const file = path.join(project, 'src/main.nsm');
  const original = fs.readFileSync(file, 'utf8');
  await page.goto(url);
  await page.waitForSelector('.app');
  await openPcb(page, 'TwoBoards/power');

  step('the 3D view of a member lists its system above its boards');
  await page.getByTestId('pcb-3d-toggle').click();
  await ready(page);
  const options = await page.locator('[data-testid=view-system] option').allInnerTexts();
  expect(options[0] === 'System TwoBoards' && options.includes('TwoBoards/power') && options.includes('TwoBoards/led'), `the list: ${options.join(' | ')}`);
  expect((await ask(page, 'parts')).every((p) => !p.path.includes('/')), 'the board alone first: no part of another member');

  step('choosing the system draws both boards, the LED board on the power board at the mated height');
  await page.selectOption('[data-testid=view-system]', 'system:TwoBoards');
  await systemDrawn(page);
  await page.waitForTimeout(400);
  const chip = await page.getByTestId('system-chip').innerText();
  expect(chip.trim() === 'System TwoBoards: 2 boards, mated at 10.65–11.35 mm', `the chip, with no finding: ${chip}`);
  const drawn = await ask(page, 'sample');
  expect(drawn.drawn > 0.05, `the canvas is drawn (${(drawn.drawn * 100).toFixed(0)} % not the background)`);
  await shot('system-iso');
  await page.getByTestId('view-front').click();
  await settle(page);
  await shot('system-front');
  await page.locator('[data-testid=view-iso]').click();
  await settle(page);

  step("a click on the LED board's socket makes the LED board the page's, selects the socket, and stays on the system, the camera where it was");
  const at = await ask(page, 'project', 'led/power_in');
  expect(!!at, 'the socket is on screen');
  const camBefore = await ask(page, 'camera');
  await page.mouse.move(at.x, at.y);
  await page.mouse.click(at.x, at.y);
  await page.waitForFunction(() => document.querySelector('.boardpick select')?.value === 'TwoBoards/led', null, { timeout: 30000 });
  await ready(page);
  await systemDrawn(page);
  // The LED board's placement is read and the scene fetched again; the view is not drawn afresh.
  await page.waitForTimeout(1500);
  const camAfter = await ask(page, 'camera');
  const camMoved = Math.hypot(...camAfter.position.map((v, i) => v - camBefore.position[i])) + Math.hypot(...camAfter.target.map((v, i) => v - camBefore.target[i]));
  expect(camMoved < 0.01, `the camera stayed where it was (it moved ${camMoved.toFixed(3)} mm)`);
  await page.waitForSelector('[data-testid=part-inspector]', { timeout: 30000 });
  const title = await page.locator('[data-testid=part-inspector] h3').innerText();
  expect(title.length > 0, `the inspector shows the socket (${title})`);
  expect((await page.locator('[data-testid=view-system]').inputValue()) === 'system:TwoBoards', 'the 3D view is still on the system');
  await page.mouse.move(5, 5);
  await page.waitForTimeout(400);
  await shot('system-picked');

  await tools(page, shot, step, expect);
  await agent({ page, url, project, shot, step, expect, binary });

  step('a header too tall for the stack is a finding across the boards; a click flies the camera to it');
  fs.writeFileSync(file, original.replace('    .power_out: Header1x2\n', '    .power_out: Header1x2 { .height &= 12mm }\n'));
  try {
    await finding(page, shot, expect);
  } finally {
    fs.writeFileSync(file, original);
  }
}

async function finding(page, shot, expect) {
  await page.waitForSelector('[data-testid=system-chip].bad', { timeout: 60000 });
  const bad = await page.getByTestId('system-chip').innerText();
  expect(bad.includes('1 finding'), `the chip: ${bad}`);
  await page.getByTestId('system-chip').click();
  const row = page.locator('[data-testid=system-findings] [data-code=system_collision]');
  await row.waitFor();
  const before = await origin(page);
  await row.click();
  await page.waitForTimeout(1200);
  const after = await origin(page);
  const moved = Math.hypot(after.x - before.x, after.y - before.y);
  expect(moved > 20, `the camera flew to the finding: the base's origin moved ${moved.toFixed(0)} px on screen`);
  await shot('system-finding');
}

/** The tools across the members: lit nets, the section, the layers apart, a board hidden. */
async function tools(page, shot, step, expect) {
  step('a net of the LED board, picked in its nets list, is lit as led/<net> on the LED board alone');
  const nets = await ask(page, 'nets');
  expect(nets.some((n) => n.startsWith('led/')) && nets.some((n) => n.startsWith('power/')), `the nets are each member's: ${nets.join(', ')}`);
  await page.locator('.side-tabs button', { hasText: /^Nets/ }).click();
  const row = page.locator('.net-row', { hasText: 'GND' }).first();
  await row.click();
  await page.waitForFunction(() => document.querySelector('.net-row.sel')?.innerText.includes('GND'));
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.litNets().includes('led/GND'), HOST, { timeout: 10000 });
  const lit = await ask(page, 'litNets');
  expect(lit.length === 1 && lit[0] === 'led/GND', `only the LED board's ground is lit, not the power board's: ${lit.join(', ')}`);
  const report = await ask(page, 'litReport');
  expect(report.layers.length > 0 && report.barrels >= 1, `its copper and its pads' barrels are lit (layers ${report.layers.join(', ')}, ${report.barrels} barrels)`);
  expect(report.dimmed > 0, `the rest is dimmed (${report.dimmed} pieces)`);
  const cmd = await ask(page, 'command');
  expect(cmd.includes('--board TwoBoards') && cmd.includes('--highlight net:led/GND'), `the command lights the member's net: ${cmd}`);
  await page.mouse.move(5, 5);
  await page.waitForTimeout(300);
  await shot('system-net-lit');
  await row.click();
  await page.waitForFunction(() => !document.querySelector('.net-row.sel'));

  step("the section across Y cuts both boards, each in its own stackup's bands");
  await page.getByTestId('view-front').click();
  await settle(page);
  await page.getByTestId('tool-section').click();
  await page.getByTestId('section-y').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.caps().length > 0, HOST, { timeout: 10000 });
  const caps = await ask(page, 'caps');
  expect(caps.includes('substrate') && caps.includes('mask-top'), `the cut is filled, the laminate and the masks in their colours (${caps.join(', ')})`);
  const scmd = await ask(page, 'command');
  expect(/--section y=[\d.-]+/.test(scmd), `the command has the cut: ${scmd}`);
  await page.getByTestId('view-fit').click();
  await settle(page);
  await page.waitForTimeout(300);
  await shot('system-section');
  await page.getByTestId('tool-section').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.caps().length === 0, HOST);

  step("the layers come apart board by board, and the LED board moves up so the two stackups do not meet");
  const ledZ = (await ask(page, 'box', 'led/power_in')).min[2];
  await page.getByTestId('tool-explode').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.exploded(), HOST);
  await page.waitForTimeout(900);
  const lifts = await ask(page, 'lifts');
  expect(lifts.power === 0 && lifts.led > 5, `the base stays and the LED board moves up by both stackups' halves: ${JSON.stringify(lifts)}`);
  const ledApart = (await ask(page, 'box', 'led/power_in')).min[2];
  expect(Math.abs(ledApart - ledZ) > 1, `the LED board's socket moved with its board's layers (${(ledApart - ledZ).toFixed(1)} mm)`);
  await page.getByTestId('view-iso').click();
  await settle(page);
  await page.getByTestId('view-fit').click();
  await settle(page);
  await shot('system-apart');
  await page.getByTestId('tool-explode').click();
  await page.waitForFunction((sel) => !document.querySelector(sel).view3d.exploded(), HOST);
  await page.waitForTimeout(900);
  const back = (await ask(page, 'box', 'led/power_in')).min[2];
  expect(Math.abs(back - ledZ) < 0.01 && (await ask(page, 'lifts')).led === 0, `together again, the LED board is where it was (${back.toFixed(2)} mm, was ${ledZ.toFixed(2)})`);

  step('the LED board hidden: the power board alone is drawn, and the last board shown cannot be hidden');
  const full = await ask(page, 'sample');
  await page.locator('[data-testid=member-toggle][data-member=led]').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.hidden().includes('led'), HOST);
  await page.waitForTimeout(300);
  const less = await ask(page, 'sample');
  expect(less.drawn < full.drawn, `less is drawn (${(less.drawn * 100).toFixed(0)} % of the canvas, was ${(full.drawn * 100).toFixed(0)} %)`);
  expect(await page.locator('[data-testid=member-toggle][data-member=power]').isDisabled(), 'the power board, the last shown, cannot be hidden');
  await page.getByTestId('view-fit').click();
  await settle(page);
  await shot('system-led-hidden');
  await page.locator('[data-testid=member-toggle][data-member=led]').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.hidden().length === 0, HOST);
  await page.getByTestId('view-fit').click();
  await settle(page);
}

/** An agent drives the System view: `/api/view` and `netsmith view` with the system's name and a
 * member's ids, and `netsmith render` of the command the page gives. */
async function agent({ page, url, project, shot, step, expect, binary }) {
  const api = new URL('/api/view', url).href;
  const post = async (change) => {
    const r = await fetch(api, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Netsmith': '1' }, body: JSON.stringify(change) });
    const text = await r.text();
    try {
      return { status: r.status, body: JSON.parse(text) };
    } catch {
      return { status: r.status, body: text };
    }
  };
  const boardPicked = () => page.evaluate(() => document.querySelector('.boardpick select')?.value);
  const shownIn3d = () => page.locator('[data-testid=view-system]').inputValue();

  step('POST /api/view with a board of the system shows it alone; with the system\'s name, its System view');
  let r = await post({ board: 'TwoBoards/power' });
  expect(r.status === 200 && r.body.board === 'TwoBoards/power' && r.body.member === undefined && r.body.shown, `the board alone: ${JSON.stringify(r.body).slice(0, 200)}`);
  expect((await shownIn3d()) === 'TwoBoards/power', 'the 3D view shows the board alone');
  r = await post({ board: 'TwoBoards', view: 'iso', highlight: ['led/J1'] });
  expect(r.status === 200 && r.body.board === 'TwoBoards' && r.body.member === 'led' && r.body.shown, `the System view, on the LED board: ${JSON.stringify(r.body).slice(0, 300)}`);
  expect(JSON.stringify(r.body.selection) === '["led/J1"]' && JSON.stringify(r.body.highlight) === '["led/J1"]', `the socket, named as render names it: ${JSON.stringify(r.body.selection)}`);
  expect((await boardPicked()) === 'TwoBoards/led' && (await shownIn3d()) === 'system:TwoBoards', 'the page is on the LED board, the 3D view on the system');
  expect(r.body.command.includes('--board TwoBoards') && r.body.command.includes('--highlight led/J1'), `the command: ${r.body.command}`);
  const command = r.body.command;

  step("a net of the other member, selected, makes the power board the page's and lights power/GND alone");
  r = await post({ select: ['net:power/GND'] });
  expect(r.status === 200 && r.body.member === 'power' && JSON.stringify(r.body.selection) === '["net:power/GND"]', `the power board's ground: ${JSON.stringify(r.body).slice(0, 300)}`);
  expect((await boardPicked()) === 'TwoBoards/power', 'the page is on the power board');
  await page.waitForFunction((sel) => JSON.stringify(document.querySelector(sel).view3d.litNets()) === '["power/GND"]', HOST, { timeout: 10000 });
  r = await post({ highlight: ['led/net:GND'] });
  expect(r.status === 200 && JSON.stringify(r.body.selection) === '["net:led/GND"]', `render's spelling of a member's net is the same id: ${JSON.stringify(r.body.selection)}`);

  step('an id of no member, and ids of two, are refused with the list; nothing changes');
  const before = JSON.stringify(r.body.selection);
  r = await post({ highlight: ['J1'] });
  expect(r.status === 400 && r.body.includes('the system `TwoBoards` does not have: `J1`') && r.body.includes('led/J1 (led/power_in)') && r.body.includes('net:led/GND'), `a designator without its member: ${r.body}`);
  r = await post({ highlight: ['led/J1', 'power/J1'] });
  expect(r.status === 400 && r.body.includes('2 members'), `two members' parts: ${r.body}`);
  r = await post({ board: 'Nope' });
  expect(r.status === 404 && r.body.includes('its systems: TwoBoards'), `a name the project has not lists its boards and systems: ${r.body}`);
  r = await post({});
  expect(r.status === 200 && JSON.stringify(r.body.selection) === before && r.body.board === 'TwoBoards', `nothing moved: ${JSON.stringify(r.body.selection)}`);

  step('a section and the layers apart of the system, through /api/view, read back as render writes them');
  r = await post({ section: 'y=5.05', exploded: true });
  expect(r.status === 200 && r.body.section === 'y=5.05' && r.body.exploded === true && r.body.command.includes('--section y=5.05'), `cut and apart: ${r.status} ${JSON.stringify(r.body).slice(0, 300)}`);
  expect((await ask(page, 'caps')).length > 0 && (await ask(page, 'lifts')).led > 5, 'the page cuts both boards and pulls them apart');
  r = await post({ section: 'none', exploded: false });
  expect(r.status === 200 && r.body.section === undefined && r.body.exploded === false, `and back: ${JSON.stringify(r.body).slice(0, 200)}`);

  step('netsmith view --board TwoBoards with a member\'s part saves the page\'s picture; render draws the command');
  const port = new URL(url).port;
  const dir = path.dirname(await shot('system-api'));
  const viewPng = path.join(dir, 'pcb-3d-system-view-cli.png');
  // No section here: a picture of a cut system takes seconds in a software renderer, near the read's 10 s (the section is driven above).
  const v = await exec(binary, ['view', '--port', port, '--board', 'TwoBoards', '--view', 'front', '--highlight', 'led/J1', '--picture', viewPng], { timeout: 90000 });
  expect(v.stdout.includes('board: TwoBoards') && v.stdout.includes('member: led') && v.stdout.includes('highlight: led/J1'), `the summary:\n${v.stdout}`);
  expect(fs.readFileSync(viewPng).subarray(1, 4).toString() === 'PNG', 'the page\'s picture is saved');
  const renderPng = path.join(dir, 'pcb-3d-system-render.png');
  const args = command.split(' ').slice(2).filter((a) => a !== 'src/main.nsm');
  const rr = await exec(binary, ['render', path.join(project, 'src/main.nsm'), ...args, '--size', '800x500', '--out', renderPng], { timeout: 90000, env: { ...process.env, NETSMITH_NO_KICAD: '1' } });
  expect(rr.stdout.includes(renderPng) && fs.readFileSync(renderPng).subarray(1, 4).toString() === 'PNG', `render draws the page's command (${command})`);
  await post({ select: [], section: 'none' });
}
