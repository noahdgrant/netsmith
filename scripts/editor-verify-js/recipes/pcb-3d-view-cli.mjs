// `netsmith view` drives the 3D view of the running editor from the command line (P8b of issue
// 430, docs/BOARD3D.md, "Driving a running editor"), with the flags of `netsmith render`: the
// state is read as a summary and as JSON, a change with a picture is made and the page shows it,
// the PNG saved is the page's canvas, and a refusal, no page and nothing listening exit 2 with
// their reasons. The page is open in this recipe's browser; the command is the binary's own.
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { fresh, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'height_limits';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lidded';
const HOST = '[data-testid=pcb-3d]';
const exec = promisify(execFile);

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

export async function run(tools) {
  const { page, shot, step, expect, url, binary } = tools;
  await fresh(tools, path.join(here, '../fixtures/height_limits.pcb.toml'));
  const port = new URL(url).port;
  // As an agent runs it: `netsmith view --port N ...`, the exit status and both streams kept.
  const view = async (...args) => {
    try {
      const r = await exec(binary, ['view', '--port', port, ...args]);
      return { code: 0, out: r.stdout, err: r.stderr };
    } catch (e) {
      return { code: e.code, out: e.stdout ?? '', err: e.stderr ?? '' };
    }
  };
  const s = await scene(page, BOARD);
  const tall = partOf(s, 'tall');
  await page.selectOption('.boardpick select', BOARD);

  step('view alone prints the state of the page, as a summary and as the editor\'s JSON');
  let r = await view();
  expect(r.code === 0 && r.out.includes(`board: ${BOARD}`) && r.out.includes('3D view: not on screen'), `the summary says the 3D view is not shown:\n${r.out}`);
  r = await view('--json');
  expect(r.code === 0 && JSON.parse(r.out).board === BOARD && JSON.parse(r.out).shown === false, `--json is the route's JSON: ${r.out.trim()}`);

  step('a change with a picture: the top, a part lit, x-ray; the page shows it and the PNG is its canvas');
  const png = path.join(path.dirname(await shot('before')), 'pcb-3d-view-cli-top.png');
  r = await view('--view', 'top', '--highlight', tall.refdes, '--xray', '--picture', png);
  expect(r.code === 0, `exit 0: ${r.err}`);
  expect(r.out.includes('view: top, orthographic') || r.out.includes('view: top, perspective'), `the state is printed:\n${r.out}`);
  expect(r.out.includes(`highlight: ${tall.refdes}`) && r.out.includes('x-ray: on') && r.out.includes(`picture: ${png}`), `lit, seen through, and the path of the picture:\n${r.out}`);
  expect((await ask(page, 'preset')) === 'top', 'the page looks from the top');
  expect((await page.getByTestId('view-xray').getAttribute('aria-pressed')) === 'true', 'x-ray is on in the page');
  expect(r.out.includes('render: netsmith render') && r.out.includes(`--highlight ${tall.refdes}`), 'the render command is in the summary');
  const bytes = fs.readFileSync(png);
  expect(bytes.subarray(1, 4).toString() === 'PNG' && bytes.length > 5000, `the saved file is a PNG with something in it (${bytes.length} bytes)`);
  const canvas = await page.evaluate((sel) => {
    const c = document.querySelector(`${sel} canvas`);
    return { w: c.width, h: c.height };
  }, HOST);
  expect(bytes.readUInt32BE(16) === canvas.w && bytes.readUInt32BE(20) === canvas.h, `the picture is the canvas's size (${canvas.w}x${canvas.h})`);
  console.log(`     picture ${png}`);
  await shot('after-view-cli');

  step('the editor-only tools, and --highlight none and --section none, are read back');
  r = await view('--colour-by', 'margin', '--height-preview', '3', '--highlight', 'none', '--select', tall.refdes);
  expect(r.code === 2 && /cannot be used with/.test(r.err), `--highlight and --select together are refused before the page is asked: ${r.err.trim()}`);
  r = await view('--colour-by', 'margin', '--height-preview', '3', '--highlight', 'none', '--section', 'none', '--ortho');
  expect(r.code === 0 && r.out.includes('colour by: margin') && r.out.includes('height preview: 3 mm') && r.out.includes('highlight: none') && r.out.includes('orthographic'), `read back:\n${r.out}`);
  expect((await page.getByTestId('colour-by').inputValue()) === 'margin', 'the page colours by margin');

  step('a refusal exits 2 with the editor\'s message and changes nothing');
  const before = JSON.parse((await view('--json')).out);
  r = await view('--view', 'front', '--highlight', 'NOPE');
  expect(r.code === 2 && r.out === '' && r.err.includes('`NOPE`') && r.err.includes('the parts are:') && r.err.includes(tall.refdes), `an unknown id lists what the board has: ${r.err.trim()}`);
  const after = JSON.parse((await view('--json')).out);
  expect(after.view === before.view && JSON.stringify(after.camera) === JSON.stringify(before.camera), 'the camera did not move');
  r = await view('--board', 'NoSuchBoard');
  expect(r.code === 2 && r.err.includes(BOARD), `a board the project has not lists the ones it has: ${r.err.trim()}`);
  r = await view('--case', 'solid');
  expect(r.code === 2 && r.out === '' && r.err.includes('case: the board has no case model to draw'), `--case on a board with no case model is refused: ${r.err.trim()}`);
  r = await view('--theme', 'dark');
  expect(r.code === 2 && r.err.includes("person's own setting"), `--theme says why not: ${r.err.trim()}`);

  step('nothing listening on a port exits 2 and says to start the editor');
  const free = await new Promise((resolve) => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => {
      const { port: p } = server.address();
      server.close(() => resolve(p));
    });
  });
  try {
    await exec(binary, ['view', '--port', String(free), '--view', 'top']);
    expect(false, 'a port with nothing on it is an error');
  } catch (e) {
    expect(e.code === 2 && e.stderr.includes('netsmith editor') && e.stderr.includes(String(free)), `no editor: ${e.stderr.trim()}`);
  }
  await shot('done');
}
