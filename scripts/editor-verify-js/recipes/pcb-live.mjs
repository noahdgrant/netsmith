// Watching the placer and the router from the PCB view (issue 764): while the board is placed the view draws the parts where
// the placer has them, with its last line and Cancel; a cancelled placing keeps nothing and offers to place again; while the
// board is routed the tracks laid so far are drawn over it; and a run started in a terminal with `--to-editor` shows in the
// status bar, its stderr JSON lines with `--progress json`. The scene and the route are held at the page with a gate, so what
// they show while they run is there for as long as the recipe looks, on a machine of any speed.
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { promisify } from 'node:util';

import { gate, guarded, whenIdle } from '../settle.mjs';
import { BOARD, fresh, scene, writeAtomic } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

const exec = promisify(execFile);

export async function run(tools) {
  const { page, shot, step, expect, project, url, binary, allowConsole } = tools;
  // A cancelled placing answers 499, which the browser logs as a failed request.
  allowConsole(/status of 499/);
  await fresh(tools);
  const parts = (await scene(page)).parts.length;
  const layout = path.join(project, 'src/main.pcb.toml');
  // The server keeps a board it placed until a file changes: the layout file written again (the same text, a new stamp) makes
  // the next scene a placing.
  const placeAgain = () => writeAtomic(layout, fs.readFileSync(layout, 'utf8'));
  const openSection = async () => {
    await page.selectOption('.boardpick select', BOARD);
    await page.locator('.pane-strip [data-section="pcb"]').first().click();
  };

  step('while the board is placed: the parts where the placer has them, its last line, and Cancel');
  const placed = gate();
  let hold = 'answer';
  await page.route('**/api/pcb/scene?*', async (route) => {
    if (hold === 'answer') {
      // The server places the board now and shows the page the placer's glimpses; its answer waits for the gate.
      const answer = await route.fetch();
      await placed.opened;
      await route.fulfill({ response: answer });
    } else if (hold === 'cancel') {
      await cancelled.opened;
      await route.fulfill({ status: 499, contentType: 'application/json', body: JSON.stringify({ code: 'cancelled', message: `${BOARD}: the placing was cancelled; nothing it placed is kept` }) });
    } else await route.continue();
  });
  placeAgain();
  await openSection();
  await page.waitForSelector(`[data-testid=pcb-live][data-parts="${parts}"]`);
  expect((await page.locator('[data-testid=pcb-live] .live-part').count()) === parts, `every part is drawn (${parts})`);
  expect((await page.locator('[data-testid=pcb-live] .live-region').count()) === 1, 'the region the placer places them in is drawn');
  expect(await page.locator('[data-testid=cancel-placing]').isVisible(), 'Cancel is offered while it places');
  const line = await page.locator('[data-testid=placing-line]').innerText();
  expect(line.length > 0, `a line says what the placer does (${line})`);
  expect(Number(await page.locator('.pcb-view').getAttribute('data-live-place')) >= 1, 'the page counted the glimpses');
  await shot('placing-live');
  placed.open();
  await page.waitForSelector('.pcb-overlay');
  expect((await page.locator('[data-testid=pcb-placing]').count()) === 0, 'the board placed replaces the glimpse');

  step('Cancel while it places: the server is asked to stop, nothing is kept, and the board can be placed again');
  const cancelled = gate();
  hold = 'cancel';
  await page.reload();
  await page.waitForSelector('.app');
  await openSection();
  await page.waitForSelector('[data-testid=cancel-placing]');
  const asked = page.waitForRequest((r) => r.url().endsWith('/api/pcb/place/cancel') && r.method() === 'POST');
  await page.locator('[data-testid=cancel-placing]').click();
  const answer = await (await guarded('the cancel to be sent', asked)).response();
  expect(answer?.status() === 204, `the server took the cancel (${answer?.status()})`);
  cancelled.open();
  await page.waitForSelector('[data-testid=place-again]');
  expect((await page.locator('.pcb-overlay').count()) === 0, 'no board is drawn');
  await shot('placing-cancelled');
  hold = 'pass';
  await page.locator('[data-testid=place-again]').click();
  await page.waitForSelector('.pcb-overlay');
  await whenIdle(page);

  step('while the board is routed: the tracks laid so far over it');
  const routed = gate();
  await page.route('**/api/pcb/route?*', async (route) => {
    const answer = await route.fetch();
    await routed.opened;
    await route.fulfill({ response: answer });
  });
  await page.locator('.pcb-bar button.primary', { hasText: /Place and route|Route again/ }).click();
  await page.waitForSelector('.pcb-host [data-testid=pcb-live-tracks]:not([data-tracks="0"])');
  const laid = Number(await page.locator('.pcb-host [data-testid=pcb-live-tracks]').getAttribute('data-tracks'));
  expect(laid > 0 && (await page.locator('.pcb-host .l-live .live-track').count()) === laid, `the tracks laid are drawn (${laid})`);
  await shot('routing-live');
  routed.open();
  await page.waitForSelector('.pcb-host [data-testid=pcb-live-tracks]', { state: 'detached' });
  await page.locator('.pcb-bar .chip', { hasText: /routed/ }).first().waitFor();
  await page.unroute('**/api/pcb/route?*');
  await page.unroute('**/api/pcb/scene?*');

  step('a run in a terminal with --to-editor: the status bar shows it, and --progress json prints JSON lines');
  const port = new URL(url).port;
  const run = await guarded('`netsmith board3d --to-editor` to exit', exec(binary, ['board3d', '--board', BOARD, '--progress', 'json', `--to-editor=${port}`], { cwd: project, maxBuffer: 64 * 1024 * 1024 }));
  const lines = run.stderr.split('\n').filter((l) => l.startsWith('{'));
  const events = lines.map((l) => JSON.parse(l));
  expect(events.some((e) => e.stage === 'placing' && e.board && typeof e.elapsedMs === 'number' && e.message), `a line of the placer as data (${lines[0]})`);
  expect(!run.stderr.includes('--to-editor: the editor'), 'the editor took every line');
  const item = page.locator('[data-testid=outside-run][data-ended=ok]');
  await item.waitFor();
  const text = await item.innerText();
  expect(text.startsWith('board3d (terminal): board3d: done'), `the bar says how it ended (${text})`);
  await shot('outside-run');
  await item.click();
  await item.waitFor({ state: 'detached' });
}
