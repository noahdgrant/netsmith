// The pads in the pours in the Summary tab of the PCB view (issue 1187): for each pour, plane and zone of a net,
// the pads of its net and how the fill joins each (solid, by how many thermal spokes, or not), the counts by kind,
// the starved thermal reliefs, the pads of other nets it clears and its copper islands. The numbers are the
// server's (`GET /api/pcb/fill`, what `netsmith board fill` prints, from `netsmith_pcb::FillReport`), of the board
// as the PCB view has it placed (and routed), filled as the Gerbers fill it.
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { guarded, waitFor, whenIdle } from '../settle.mjs';
import { scene } from '../pcb-helpers.mjs';

export const fixture = 'routed_pours';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));
const exec = promisify(execFile);

export async function run(tools) {
  const { page, shot, step, expect, project, url, binary } = tools;
  const file = path.join(project, 'src/main.routes.toml');
  const stampOf = () => {
    const on = fs.statSync(file, { bigint: true });
    return `${on.mtimeNs}-${on.size}`;
  };
  // Every run starts from the fixture's file, whatever ran before: two zones on the top layer, by hand.
  fs.copyFileSync(path.join(here, '../fixtures/routed_pours.routes.toml'), file);
  await page.goto(url);
  await page.waitForSelector('.app');
  await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === stampOf());
  await page.reload();
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
  // A recipe before this one of the same session may have routed the board, so it may be routed here.
  await page.waitForSelector('[data-testid=summary-fill-counts]');
  await whenIdle(page);
  const server = await page.evaluate(async (b) => (await fetch(`/api/pcb/fill?board=${encodeURIComponent(b)}`)).json(), BOARD);
  const areas = () =>
    page.locator('[data-testid=summary-fill-area]').evaluateAll((all) =>
      all.map((e) => ({ layer: e.getAttribute('data-layer'), net: e.getAttribute('data-net'), starved: e.getAttribute('data-starved'), text: e.querySelector('summary').textContent })),
    );

  step('a line for each pour, plane and zone, the same as the server says');
  const rows = await areas();
  expect(rows.length === server.fills.length && rows.length > 0, `${rows.length} fills on the page, ${server.fills.length} from the server`);
  expect(rows.map((r) => `${r.layer}:${r.net}`).join() === server.fills.map((f) => `${f.layer}:${f.net}`).join(), `the same fills (${rows.map((r) => `${r.layer}:${r.net}`)})`);
  expect(rows.some((r) => r.layer === 'inner1' && r.text.includes('plane on GND')), 'the ground plane is one');
  expect(rows.some((r) => r.layer === 'bottom' && r.text.includes('pour on GND')), 'the ground pour is one');
  expect(rows.some((r) => r.layer === 'top' && r.text.includes('zone on GND')), 'the hand zone is one');
  for (const [i, f] of server.fills.entries()) {
    expect(rows[i].text.includes(`by 2 or more spokes`) && rows[i].text.includes(`clears ${f.cleared.length} pad`), `the counts and the cleared pads of ${f.layer} (${rows[i].text})`);
  }
  await page.locator('[data-testid=summary-fill-counts]').scrollIntoViewIfNeeded();
  await whenIdle(page);
  await shot('fill-areas');

  step('opened, a fill lists each pad of its net and how it is joined');
  const withPads = server.fills.findIndex((f) => f.pads.length > 0);
  expect(withPads >= 0, 'a fill joins a pad');
  const opened = page.locator('[data-testid=summary-fill-area]').nth(withPads);
  await opened.locator('summary').click();
  await opened.scrollIntoViewIfNeeded();
  await whenIdle(page);
  const pads = await opened.locator('[data-testid=summary-fill-pad]').evaluateAll((all) => all.map((e) => ({ pad: e.getAttribute('data-pad'), join: e.getAttribute('data-join'), text: e.textContent })));
  const want = server.fills[withPads].pads;
  expect(pads.length === want.length, `${pads.length} pads listed, ${want.length} from the server`);
  expect(pads.map((p) => `${p.pad}:${p.join}`).join() === want.map((p) => `${p.part}.${p.pad}:${p.join}`).join(), 'the same pads, joined the same way');
  for (const [i, p] of want.entries()) {
    if (p.join === 'thermal' && p.spokes > 0) expect(pads[i].text.includes(`${p.joinedSpokes} of ${p.spokes} spokes`), `the spokes of ${pads[i].pad} (${pads[i].text})`);
  }
  await shot('fill-pads');

  step('the counts of the board and its starved reliefs are the server\'s');
  const counts = page.locator('[data-testid=summary-fill-counts]');
  expect(Number(await counts.getAttribute('data-starved')) === server.counts.starved, 'the starved count');
  expect((await page.locator('[data-testid=summary-fill-starved]').count()) === server.starved.length, 'a line for each starved relief');

  step('the command line says the same of the same board');
  const cli = await guarded('`netsmith board fill` to exit', exec(binary, ['board', 'fill', '--format', 'json'], { cwd: project, maxBuffer: 1 << 26 }).catch((e) => e));
  expect(!cli.code, `board fill exits 0 (${cli.stderr ?? ''})`);
  const printed = JSON.parse(cli.stdout);
  // The command fills the board placed from the same layout file; its copper is the routes file's and the router's
  // copper kept on disk, which the page has only once it has routed, so the fills and their pads are compared here,
  // not how each is joined.
  const names = (fills, part) => fills.map((f) => `${f.layer}:${f.net}:${f.pads.map((p) => `${p[part]}.${p.pad}`).join('+')}`).join();
  expect(names(printed.fills, 'part') === names(server.fills, 'part'), `the same fills and pads (${names(printed.fills, 'part')})`);
  step('route the board: the fill is asked again, of the routed board');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  await page.waitForSelector('[data-testid=summary-fill-counts][data-routed=true]');
  await whenIdle(page);
  const routed = await page.evaluate(async (b) => (await fetch(`/api/pcb/fill?board=${encodeURIComponent(b)}`)).json(), BOARD);
  expect(routed.routed === true, 'the server filled the routed board');
  expect(Number(await counts.getAttribute('data-starved')) === routed.counts.starved, 'the page shows its starved count');
  await counts.scrollIntoViewIfNeeded();
  await whenIdle(page);
  await shot('fill-routed');
}
