// The Gerber view of the PCB area (issue #1107): before fab it says there is no package; after Make fab files it draws the
// package fab wrote with netsmith's own reader, the SVG `netsmith view gerber --format svg` writes, byte for byte. A file
// turned off is hidden and the picture changes; the bottom is mirrored; the ruler measures in mm; and a package a person
// opens with a file the reader does not understand lists it with its line, as the command does.
import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { guarded, whenIdle } from '../settle.mjs';

export const fixture = 'two_boards';

const BOARD = 'TwoBoards/led';
const DRAWING = '[data-testid=gerber-drawing]';

/** The panel's answer for a view, as the server gives it. */
const answer = (page, layer) =>
  page.evaluate(async ([b, l]) => (await fetch(`/api/gerber?board=${encodeURIComponent(b)}&layer=${l}`)).json(), [BOARD, layer]);

export async function run({ page, shot, step, expect, section, project, binary, allowConsole }) {
  await page.selectOption('.boardpick select', BOARD);
  await section('pcb');

  step('the Gerber switch of the PCB bar: before fab there is no package, and the view says how to make one');
  // The page asks for the package and the server answers 404: the browser logs it.
  allowConsole(/404/);
  // A recipe before this one in the session (`build`) may have made the board's package: take it away, so the view
  // starts with none (the server answers 404 for a package that is not on disk).
  const left = await answer(page, 'top');
  if (left.path) fs.rmSync(path.join(project, path.dirname(left.path)), { recursive: true, force: true });
  await page.getByTestId('pcb-gerber').click();
  await page.waitForSelector('[data-testid=gerber-none]');
  await shot('no-package');

  step('Make fab files from the empty view: the package is drawn, its files listed with their colours');
  await page.locator('[data-testid=gerber-none] button', { hasText: 'Make fab files' }).click();
  await page.waitForSelector('.build .chip:has-text("package made"), .build .chip:has-text("stopped")');
  expect((await page.locator('.build').innerText()).includes('design rules check: passed'), 'fab made the package');
  await section('pcb');
  await page.waitForSelector(`${DRAWING}[data-layer=top] svg g.layer`);
  await whenIdle(page);
  const rows = page.getByTestId('gerber-layer');
  const files = await rows.evaluateAll((els) => els.map((e) => ({ file: e.dataset.file, fn: e.dataset.function, layer: e.dataset.layer ?? null })));
  expect(files.length >= 8, `the package's files are listed (${files.length})`);
  const topCopper = files.find((f) => f.fn === 'copper' && f.layer === 'top');
  expect(!!topCopper, 'the top copper is one of them');
  expect(files.some((f) => f.fn === 'drill'), 'and the drill');
  const swatch = await rows.filter({ has: page.locator(`text=${topCopper.file.split('/').pop()}`) }).locator('.swatch').evaluate((e) => getComputedStyle(e).backgroundColor);
  expect(swatch !== 'rgba(0, 0, 0, 0)', `the top copper has its colour in the list (${swatch})`);
  expect((await page.locator('.gerber-side').innerText()).includes('plated'), 'the holes are listed by size');
  await shot('top');

  step('the drawing is the one `netsmith view gerber` writes for the same package and view');
  const top = await answer(page, 'top');
  expect(top.path && !path.isAbsolute(top.path), `the package is named from the project's folder (${top.path})`);
  const out = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'gerber-')), 'top.svg');
  await guarded(
    'netsmith view gerber',
    promisify(execFile)(binary, ['view', 'gerber', path.join(project, top.path), '--layer', 'top', '--format', 'svg', '--picture', out]),
  );
  expect(fs.readFileSync(out, 'utf8') === top.svg, 'the command writes the same SVG, byte for byte');
  const groups = await page.locator(`${DRAWING} g.layer`).count();
  expect(groups === top.drawn.length, `the page shows every file the view draws (${groups})`);

  step('turn the top copper off: its group is hidden and the picture changes; on again, it is back');
  const before = await page.locator(DRAWING).screenshot();
  const box = rows.filter({ has: page.locator(`text=${topCopper.file.split('/').pop()}`) }).locator('input[type=checkbox]');
  await box.uncheck();
  await whenIdle(page);
  const display = () => page.locator(`${DRAWING} g.layer`).evaluateAll((gs, f) => gs.find((g) => g.dataset.file === f)?.style.display, topCopper.file);
  expect((await display()) === 'none', 'the top copper is hidden');
  const without = await page.locator(DRAWING).screenshot();
  expect(!before.equals(without), 'the picture is not the one with the copper');
  await shot('top-copper-off');
  await box.check();
  await whenIdle(page);
  expect((await display()) === '', 'the top copper is drawn again');

  step('the bottom view is mirrored, as a person sees the board turned over');
  await page.getByTestId('gerber-view-bottom').click();
  await page.waitForSelector(`${DRAWING}[data-layer=bottom][data-mirrored]`);
  await whenIdle(page);
  expect((await page.locator(`${DRAWING} svg`).first().getAttribute('data-show')) === 'bottom', 'the reader drew the bottom');
  await shot('bottom');

  step('the ruler measures between two points, in mm');
  await page.getByTestId('gerber-ruler-toggle').click();
  const r = await page.locator(DRAWING).boundingBox();
  const region = (await answer(page, 'bottom')).region;
  const a = { x: r.x + r.width * 0.25, y: r.y + r.height * 0.5 };
  const b = { x: r.x + r.width * 0.75, y: r.y + r.height * 0.5 };
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 4 });
  await page.mouse.up();
  await whenIdle(page);
  const mm = Number(await page.getByTestId('gerber-ruler').getAttribute('data-mm'));
  const want = region[2] * 0.5;
  expect(Math.abs(mm - want) < 0.01, `half the drawing's width is ${want.toFixed(3)} mm: the ruler reads ${mm} mm`);
  expect((await page.getByTestId('gerber-ruler').innerText()).includes('mm'), 'the reading is shown in the bar');
  await shot('ruler');
  await page.getByTestId('gerber-ruler-toggle').click();

  step('open a package with a file the reader does not understand: it is listed with its line and not drawn');
  const odd = path.join(path.dirname(out), 'odd-F_Cu.gbr');
  fs.writeFileSync(odd, '%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\n%SRX2Y1I5.0J0*%\nD10*\nX0Y0D03*\n%SR*%\nM02*\n');
  await page.getByTestId('gerber-file').setInputFiles(odd);
  await page.waitForSelector('[data-testid=gerber-unread]');
  await whenIdle(page);
  const note = await page.getByTestId('gerber-unread').first().innerText();
  expect(note.includes('odd-F_Cu.gbr, line'), `the note names the file and the line: ${note.split('\n')[0]}`);
  expect((await page.getByTestId('gerber-unread-chip').count()) === 1, 'the bar says something was not understood');
  await shot('not-understood');

  step("back to the board's own package");
  await page.getByTestId('gerber-fab').click();
  await page.waitForSelector(`${DRAWING} svg g.layer`);
  await whenIdle(page);
  expect((await page.getByTestId('gerber-unread').count()) === 0, "fab's package has nothing the reader did not understand");
  fs.rmSync(path.dirname(out), { recursive: true, force: true });
}
