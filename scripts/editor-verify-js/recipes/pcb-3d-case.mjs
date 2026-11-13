// A board's case model in the 3D view (features/pcb-3d.md, "A case model"; ADR 0109): the case's
// solids are drawn see-through round the board, a toggle hides and shows them, a second one draws
// them solid (opaque, in the way of a pick; `--case` of `render` and `view`), the problems dock
// lists the fit check's findings, and picking one flies the camera to it and lights the case solid
// it is about (showing the case if it was hidden), and an agent lights a case model by name over
// `/api/view` (`highlight: ["case:<name>"]`, as `render --highlight` names it), with an unknown
// name refused with the list. The fixture's case is two STEP files written by
// hand: a box with a boss under one hole, and a lid 8 mm over the board that a 9 mm header goes
// into.
import { execFile } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { fresh, openPcb, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'case_fit';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Cased';
const HOST = '[data-testid=pcb-3d]';
const exec = promisify(execFile);

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

/** Let the view draw what changed (a frame, and a camera move of 180 ms). */
const settle = (page) => page.waitForTimeout(350);

export async function run(tools) {
  const { page, shot, step, expect, url, binary } = tools;
  await fresh(tools, path.join(here, '../fixtures/case_fit.pcb.toml'));
  await openPcb(page, BOARD);
  const s = await scene(page, BOARD);
  const tall = partOf(s, 'tall');
  const hole = partOf(s, 'hole2');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`, { timeout: 30000 });
  await settle(page);

  step('the case is drawn see-through round the board: the box (six solids) and the lid, with a toggle and a legend entry');
  const cases = await ask(page, 'cases');
  const names = cases.map((c) => `${c.case}:${c.solid}`);
  expect(cases.length === 7 && names.includes('base:boss') && names.includes('lid:lid'), `the case's solids: ${names.join(', ')}`);
  expect(await ask(page, 'caseShown'), 'the case is shown');
  const toggle = page.getByTestId('tool-case');
  expect((await toggle.getAttribute('aria-pressed')) === 'true', 'the Case tool is on');
  expect((await page.getByTestId('case-legend').count()) === 1, 'the legend names the case');
  await shot('case-shown');

  step('the toggle hides the case, and the legend drops it');
  await toggle.click();
  await settle(page);
  expect(!(await ask(page, 'caseShown')), 'the case is hidden');
  expect((await toggle.getAttribute('aria-pressed')) === 'false', 'the Case tool is off');
  expect((await page.getByTestId('case-legend').count()) === 0, 'no legend entry');
  await shot('case-hidden');

  step('the Solid tool draws the case opaque, so it hides the board and a pick finds nothing behind it; the key steps through the modes');
  const solidTool = page.getByTestId('tool-case-solid');
  expect((await solidTool.getAttribute('aria-pressed')) === 'false', 'the Solid tool is off');
  await solidTool.click();
  await settle(page);
  expect((await ask(page, 'caseMode')) === 'solid' && (await ask(page, 'caseShown')), 'the case is solid, and shown');
  expect((await solidTool.getAttribute('aria-pressed')) === 'true' && (await toggle.getAttribute('aria-pressed')) === 'true', 'the Solid and Case tools are pressed');
  expect((await page.getByTestId('case-legend').innerText()).includes('solid'), 'the legend says the case is solid');
  expect((await ask(page, 'project', tall.path)) === null, 'the case is in the way: the header inside it is not picked');
  await shot('case-solid');
  // Shift+C: solid, hidden, see-through, solid.
  await page.keyboard.press('Shift+C');
  await settle(page);
  expect((await ask(page, 'caseMode')) === 'hidden', 'Shift+C hides the solid case');
  expect(!!(await ask(page, 'project', tall.path)), 'with the case hidden the header is found again');
  await page.keyboard.press('Shift+C');
  await settle(page);
  expect((await ask(page, 'caseMode')) === 'see-through', 'Shift+C draws it see-through again');
  expect(!!(await ask(page, 'project', tall.path)), 'a see-through case is no obstacle to a pick');
  await shot('case-see-through-again');
  await solidTool.click();
  await solidTool.click();
  await settle(page);
  expect((await ask(page, 'caseMode')) === 'see-through', 'the Solid tool puts it back to see-through');
  await toggle.click();
  await settle(page);
  expect((await ask(page, 'caseMode')) === 'hidden', 'the Case tool hides it, for the finding below');

  step('the problems dock lists the fit; picking the collision flies to the header, lights the lid and shows the case again');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('[data-testid=dock]');
  const collision = page.locator('[data-testid=problem]', { hasText: 'is inside the solid `lid`' }).first();
  await collision.waitFor({ timeout: 20000 });
  const text = await collision.innerText();
  expect(text.includes(tall.refdes) && text.includes('the solid `lid` of the case model `lid` by 1.00 mm'), `the collision: ${text.replace(/\n/g, ' | ')}`);
  // case_clearance, case_reach, case_unchecked and case_boss, by what each says.
  for (const words of ['from the solid `wall_right`', "a connector at the board's right edge", 'has no height and stands under', 'has no case solid']) {
    expect((await page.locator('[data-testid=problem]', { hasText: words }).count()) > 0, `the dock lists: ${words}`);
  }
  const before = await ask(page, 'project', tall.path);
  await collision.click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.focusedSolids().includes('case:lid:lid'), HOST);
  await page.waitForTimeout(1000);
  expect(await ask(page, 'caseShown'), 'a finding about the case shows it');
  expect((await toggle.getAttribute('aria-pressed')) === 'true', 'and the toggle says so');
  const after = await ask(page, 'project', tall.path);
  expect(before && after && Math.hypot(after.x - before.x, after.y - before.y) > 10, `the camera moved to the finding (${JSON.stringify(before)} to ${JSON.stringify(after)})`);
  await shot('collision-focused');

  step('picking the hole with no boss flies to the hole; no solid is lit');
  const boss = page.locator('[data-testid=problem]', { hasText: 'has no case solid' }).first();
  await boss.click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.focusedSolids().length === 0, HOST);
  await page.waitForTimeout(1000);
  const text2 = await boss.innerText();
  expect(text2.includes(hole.refdes), `the boss finding names the hole: ${text2.replace(/\n/g, ' | ')}`);
  await shot('boss-focused');

  step('a finding about a part inside a solid case sees through it, so the part shows');
  await solidTool.click();
  await settle(page);
  expect((await ask(page, 'caseMode')) === 'solid', 'the case is solid again');
  await collision.click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.caseMode() === 'see-through', HOST);
  expect((await solidTool.getAttribute('aria-pressed')) === 'false', 'the Solid tool says it is see-through');
  await page.waitForTimeout(800);
  await shot('finding-through-solid');

  step('/api/view lights a case model by name, shows the case if it was hidden, and names it as render does');
  const api = new URL('/api/view', url).href;
  const post = async (change) => {
    const r = await fetch(api, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Netsmith': '1' }, body: JSON.stringify(change) });
    const text = await r.text();
    let body = text;
    try {
      body = JSON.parse(text);
    } catch {
      // A refusal is text.
    }
    return { status: r.status, body };
  };
  await toggle.click();
  await settle(page);
  expect(!(await ask(page, 'caseShown')), 'the case is hidden before the change');
  let r = await post({ view: 'iso', highlight: ['case:lid'] });
  expect(r.status === 200, `the change is made: ${r.status} ${JSON.stringify(r.body).slice(0, 300)}`);
  expect(JSON.stringify(r.body.highlight) === JSON.stringify(['case:lid']), `what is lit, named as render names it: ${JSON.stringify(r.body.highlight)}`);
  expect(JSON.stringify(await ask(page, 'litCases')) === JSON.stringify(['lid']) && (await ask(page, 'focusedSolids')).length === 0, 'the lid is lit by name, and no finding is');
  expect(await ask(page, 'caseShown'), 'lighting a case model shows the case');
  expect((await toggle.getAttribute('aria-pressed')) === 'true', 'and the toggle says so');
  expect(r.body.command.includes('--highlight case:lid'), `the copied command and the API agree: ${r.body.command}`);
  await shot('case-lit-by-name');

  step('an unknown case model is refused with the list, and changes nothing; [] lights nothing');
  r = await post({ highlight: ['case:box'] });
  expect(r.status === 400 && String(r.body).includes('`case:box`') && String(r.body).includes('the case models: case:base, case:lid'), `refused with the list: ${r.status} ${r.body}`);
  expect(JSON.stringify(await ask(page, 'litCases')) === JSON.stringify(['lid']), 'the lid is still lit');
  r = await post({ highlight: [] });
  expect(r.status === 200 && r.body.highlight.length === 0 && (await ask(page, 'litCases')).length === 0, `nothing is lit: ${JSON.stringify(r.body.highlight)}`);

  step('/api/view draws the case hidden, see-through or solid (as render --case does); a lit solid stays see-through, an unknown mode is refused');
  r = await post({ case: 'solid' });
  expect(r.status === 200 && r.body.case === 'solid', `the case is solid: ${r.status} ${JSON.stringify(r.body).slice(0, 200)}`);
  expect((await ask(page, 'caseMode')) === 'solid' && (await solidTool.getAttribute('aria-pressed')) === 'true', 'the page shows it, and the Solid tool is pressed');
  expect(r.body.command.includes('--case solid'), `the copied command names it: ${r.body.command}`);
  await settle(page);
  await shot('api-case-solid');
  r = await post({ highlight: ['case:lid'] });
  expect(r.status === 200 && r.body.case === 'solid' && JSON.stringify(r.body.highlight) === JSON.stringify(['case:lid']), `lighting the lid leaves the case solid: ${JSON.stringify(r.body).slice(0, 200)}`);
  await settle(page);
  expect(!!(await ask(page, 'project', tall.path)), 'the lit lid is see-through, so the header inside it is found');
  await shot('api-case-solid-lit');
  r = await post({ highlight: [], case: 'hidden' });
  expect(r.status === 200 && r.body.case === 'hidden' && !(await ask(page, 'caseShown')), `hidden: ${JSON.stringify(r.body).slice(0, 200)}`);
  expect(!r.body.command.includes('--case see-through') && r.body.command.includes('--case hidden'), `the command: ${r.body.command}`);
  r = await post({ case: 'see-through' });
  expect(r.status === 200 && r.body.case === 'see-through' && !r.body.command.includes('--case'), `see-through is render's default and is not named: ${r.body.command}`);
  r = await post({ case: 'opaque' });
  expect(r.status === 400 || r.status === 422, `an unknown mode is refused: ${r.status} ${r.body}`);
  expect((await ask(page, 'caseMode')) === 'see-through', 'and changes nothing');

  step('netsmith view --case draws the page as asked and the summary says so');
  const port = new URL(url).port;
  const view = async (...args) => {
    try {
      const out = await exec(binary, ['view', '--port', port, ...args], { timeout: 90000 });
      return { code: 0, out: out.stdout, err: out.stderr };
    } catch (e) {
      return { code: e.code, out: e.stdout ?? '', err: e.stderr ?? '' };
    }
  };
  let cli = await view('--case', 'solid');
  expect(cli.code === 0 && cli.out.includes('case: solid') && cli.out.includes('--case solid'), `view --case solid:\n${cli.out}${cli.err}`);
  expect((await ask(page, 'caseMode')) === 'solid', 'the page is solid');
  cli = await view('--case', 'hidden');
  expect(cli.code === 0 && cli.out.includes('case: hidden'), `view --case hidden:\n${cli.out}${cli.err}`);
  cli = await view('--case', 'see-through');
  expect(cli.code === 0 && cli.out.includes('case: see-through'), `view --case see-through:\n${cli.out}${cli.err}`);
  cli = await view('--case', 'opaque');
  expect(cli.code === 2 && cli.err.includes('see-through') && cli.err.includes('solid'), `an unknown mode is refused with the names: ${cli.err.trim()}`);
}
