// The table of the routed nets in the Summary tab of the PCB view (issue 1223): each net with copper, its routed and
// straight length, its vias, layer changes and layers, and each diff pair's lengths, skew, coupled length and vias
// side by side. The numbers are the server's (`GET /api/pcb/nets`, what `netsmith board nets` prints, from
// `netsmith_fab::NetTable`), of the board as the PCB view has it placed (and routed).
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { guarded, waitFor, whenIdle } from '../settle.mjs';
import { scene, writeAtomic } from '../pcb-helpers.mjs';

export const fixture = 'routed_led';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));
const exec = promisify(execFile);

export async function run(tools) {
  const { page, shot, step, expect, project, url, binary } = tools;
  const file = path.join(project, 'src/main.routes.toml');
  // Every run starts from the fixture's file, whatever ran before: the copper of GND, by hand.
  writeAtomic(file, fs.readFileSync(path.join(here, '../fixtures/routed_led.routes.toml'), 'utf8'));
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
  await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
  await page.waitForSelector('[data-testid=summary-nets], [data-testid=summary-nets-none]');
  await whenIdle(page);
  const ask = () => page.evaluate(async (b) => (await fetch(`/api/pcb/nets?board=${encodeURIComponent(b)}`)).json(), BOARD);
  const rows = () => page.locator('[data-testid=summary-nets-net]').evaluateAll((all) => all.map((e) => ({ net: e.getAttribute('data-net'), text: e.textContent })));

  step('a line for each net with copper, the same as the server says');
  const server = await ask();
  // A recipe before this one of the same session may have routed the board, so it may be routed here.
  expect(server.nets.length > 0, `the routes file's copper is measured (${server.nets.length} nets)`);
  const table = page.locator('[data-testid=summary-nets]');
  await table.locator('summary').click();
  await table.scrollIntoViewIfNeeded();
  await whenIdle(page);
  let listed = await rows();
  expect(listed.map((r) => r.net).join() === server.nets.map((n) => n.net).join(), `the same nets (${listed.map((r) => r.net)})`);
  for (const [i, n] of server.nets.entries()) {
    expect(listed[i].text.startsWith(`${n.net}: `) && listed[i].text.includes(`${n.vias} via`), `the line of ${n.net} (${listed[i].text})`);
  }
  expect((await page.locator('[data-testid=summary-nets-pair]').count()) === server.pairs.length, 'a line for each diff pair');
  await shot('nets-table');

  step('the command line says the same of the same copper');
  const cli = await guarded('`netsmith board nets` to exit', exec(binary, ['board', 'nets', '--format', 'json'], { cwd: project, maxBuffer: 1 << 26 }).catch((e) => e));
  expect(!cli.code, `board nets exits 0 (${cli.stderr ?? ''})`);
  const printed = JSON.parse(cli.stdout);
  if (!server.routed) {
    // Not routed here, the page and the command measure the routes file's copper on the same placement.
    const numbers = (t) => t.nets.map((n) => `${n.net}:${n.length.toFixed(3)}:${n.vias}`).join();
    expect(numbers(printed) === numbers(server), `the same nets, lengths and vias (${numbers(printed)})`);
  }

  step('route the board: the table is asked again, of the routed board');
  await page.getByRole('button', { name: /Place and route|Route again/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  await page.waitForSelector('[data-testid=summary-nets][data-routed=true]');
  await whenIdle(page);
  const routed = await ask();
  expect(routed.routed === true, 'the server measured the routed board');
  if (!(await table.evaluate((e) => e.open))) await table.locator('summary').click();
  await whenIdle(page);
  listed = await rows();
  expect(listed.length === routed.nets.length && listed.length >= server.nets.length, `the routed board's ${routed.nets.length} nets are listed`);
  await table.scrollIntoViewIfNeeded();
  await whenIdle(page);
  await shot('nets-routed');
}
