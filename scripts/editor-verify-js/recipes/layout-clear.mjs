// Clearing and finding the layout file (ADR 0061, stage 4), on a board of several modules: the palette clears a module's entries or the
// file's, after a question; a part with an entry is marked in the drawing; the file is in the project tree and opens in Source; a problem
// of the layout stage opens the file at its line; and a change by another program redraws the page.
import { layoutFile, partStates, partsOf } from '../layout-helpers.mjs';
import { openSheet } from '../schematic-helpers.mjs';

export const fixture = 'schematic_bench';

/** The recipe leaves no layout file behind: the next recipe of the session draws the same project. */
export async function run(tools) {
  try {
    await body(tools);
  } finally {
    layoutFile(tools.project, 'schematic_bench.sch.toml').remove();
    // Long enough for the page to read the open file once more, which says 404, while the console is still listened to.
    await tools.page.waitForTimeout(1500);
  }
}

async function body({ page, shot, step, expect, project, section, allowConsole }) {
  // The file is open in a tab of Source and is deleted at the end: the page reads the open files after each analysis and the server says 404.
  allowConsole(/api\/file\?path=schematic_bench\.sch\.toml/);
  const NAME = 'schematic_bench.sch.toml';
  const layout = layoutFile(project, NAME);
  layout.remove();
  const board = 'SchematicBench';
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForSelector('svg.schematic');
  await page.waitForSelector('.sheetpick .crumb-sheet');
  const parts = await partsOf(page, board);
  const [buckCap, adcCap, buckIc] = [parts['buck.c_in'], parts['adc.c_filter'], parts['buck.ic']];
  expect(!!buckCap && !!adcCap && !!buckIc, `the bench has its modules' parts (${buckCap} ${adcCap} ${buckIc})`);
  const put = (text) =>
    page.evaluate(
      async ([b, body]) => {
        const r = await fetch(`/api/schematic-hints?board=${encodeURIComponent(b)}`, { method: 'PUT', headers: { 'X-Netsmith': '1', Accept: 'application/json', 'Content-Type': 'text/plain' }, body });
        return r.status;
      },
      [board, text],
    );
  const settle = async () => {
    await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 30000 });
    await page.waitForTimeout(700);
  };
  const palette = async (text) => {
    await page.keyboard.press('Control+Shift+P');
    await page.waitForSelector('.qp');
    await page.keyboard.press('Control+a');
    await page.keyboard.type(`>${text}`);
    await page.waitForTimeout(250);
  };
  const canvas = () => page.locator('.schematic-host').click({ position: { x: 6, y: 6 } });
  const hinted = async (ref) => (await partStates(page))[ref]?.hinted ?? null;
  const waitHinted = (ref, want) =>
    page.waitForFunction(([r, w]) => {
      const g = [...document.querySelectorAll(`svg.schematic g.part[data-ref="${r}"]`)].find((e) => e.getClientRects().length);
      const hint = g?.previousElementSibling?.previousElementSibling?.previousElementSibling;
      return !!hint?.classList.contains('hinted') === w;
    }, [ref, want], { timeout: 30000 });

  step('on a module of the bench, with no entry, neither clear command is offered');
  await openSheet(page, 'buck');
  await canvas();
  await palette('clear');
  const none = (await page.locator('.qp-row').allInnerTexts()).join(' ');
  expect(!/layout of this module|every layout entry/.test(none), `neither is in the palette (${none.replace(/\n/g, ' ')})`);
  await page.keyboard.press('Escape');

  step('a part with an entry wears the hand-arranged outline, in the theme’s own token');
  const file = `version = 1\n\n[Other.parts.x]\ndx = 1.27\ndy = 0\n\n[${board}.sheets.adc.parts.c_filter]\nrotate = 180\n\n[${board}.sheets.buck.parts.c_in]\nrotate = 180\n\n[${board}.sheets.buck.parts.c_out]\nmirror = true\n`;
  expect((await put(file)) === 200, 'the file is written');
  await waitHinted(buckCap, true);
  await page.waitForTimeout(600);
  expect((await hinted(buckCap)) === true && (await hinted(parts['buck.c_out'])) === true && (await hinted(buckIc)) === false, 'c_in and c_out of the module on show are marked, its regulator is not');
  const colours = await page.evaluate((r) => {
    const g = [...document.querySelectorAll(`svg.schematic g.part[data-ref="${r}"]`)].find((e) => e.getClientRects().length);
    const hint = g.previousElementSibling.previousElementSibling.previousElementSibling;
    const probe = document.createElement('i');
    probe.style.color = 'var(--sch-muted)';
    document.body.appendChild(probe);
    const token = getComputedStyle(probe).color;
    probe.remove();
    return { stroke: getComputedStyle(hint).stroke, token, dash: getComputedStyle(hint).strokeDasharray };
  }, buckCap);
  expect(colours.stroke === colours.token && colours.dash !== 'none', `the outline is dashed in --sch-muted (${colours.stroke})`);
  await shot('marked');

  step('the file is in the project tree under "Schematic layout" and opens in Source with TOML colouring');
  await section('source');
  const treeRow = page.locator('.sidebar .ft-row', { hasText: NAME }).first();
  await treeRow.waitFor({ timeout: 15000 });
  expect((await page.locator('.sidebar').innerText()).includes('Schematic layout'), 'a section "Schematic layout"');
  const tabOpen = (n) => page.evaluate((x) => [...document.querySelectorAll('[role=tab], .tab, .ftab')].some((t) => t.textContent.includes(x)), n);
  await treeRow.click();
  await page.waitForTimeout(1500);
  if (!(await tabOpen(NAME))) await treeRow.dblclick();
  await page.waitForFunction((x) => [...document.querySelectorAll('[role=tab], .tab, .ftab')].some((t) => t.textContent.includes(x)), NAME, { timeout: 20000 });
  await page.waitForSelector('.cm-content');
  const tab = await page.evaluate(() => [...document.querySelectorAll('[role=tab], .tab, .ftab')].map((t) => t.textContent).join(' | '));
  expect(tab.includes(NAME), `its tab is open (${tab})`);
  const coloured = await page.evaluate(() => document.querySelectorAll('.cm-content .cm-line span').length);
  expect(coloured > 3, `the text is coloured as TOML (${coloured} spans)`);
  await shot('in-source');
  await section('schematic');

  step('Clear the layout of this module asks first; Cancel keeps the file, the yes removes this module’s entries and no other');
  await canvas();
  await palette('clear the layout of this module');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="confirm"]');
  const q = await page.locator('[data-testid="confirm"]').innerText();
  expect(/2 entries/.test(q) && new RegExp(NAME.replace(/\./g, '\\.')).test(q), `the question says what goes (${q.replace(/\n/g, ' ')})`);
  await page.waitForTimeout(350);
  await shot('confirm');
  await page.locator('[data-testid="confirm"] button', { hasText: 'Cancel' }).click();
  expect(/buck\.parts\.c_in/.test(layout.text()), 'Cancel: the file is as it was');
  await palette('clear the layout of this module');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="confirm-yes"]');
  await page.locator('[data-testid="confirm-yes"]').click();
  await settle();
  await waitHinted(buckCap, false);
  const left = layout.text();
  expect(!/buck/.test(left) && /sheets\.adc\.parts\.c_filter/.test(left) && /\[Other\.parts\.x\]/.test(left), `this module's entries are gone and the others stay (${JSON.stringify(left)})`);
  expect((await hinted(buckCap)) === false && (await hinted(parts['buck.c_out'])) === false, 'the parts are not marked any more');

  step('Clear every layout entry of the file empties the file for every board');
  await canvas();
  await palette('clear every layout entry');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="confirm"]');
  const all = await page.locator('[data-testid="confirm"]').innerText();
  expect(/2 entries/.test(all) && /every board/.test(all), `the question says the file’s count (${all.replace(/\n/g, ' ')})`);
  await page.locator('[data-testid="confirm-yes"]').click();
  await settle();
  expect(layout.text() === 'version = 1\n', `the file keeps \`version = 1\` (${JSON.stringify(layout.text())})`);

  step('a file that does not read: its problem opens the file in Source at the line');
  layout.write(`version = 1\n\n[${board}.sheets.buck.parts.c_in]\nrotate = 180\nspin = 3\n`);
  await page.waitForFunction(async () => (await (await fetch('/api/analysis', { cache: 'no-store' })).json()).problems.some((p) => p.stage === 'layout'), null, { timeout: 30000 });
  await page.keyboard.press('Control+j');
  await page.waitForSelector('.dock .problem.warning');
  await page.locator('.dock .problem.warning').first().click();
  await page.waitForSelector('.cm-content');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('spin'), null, { timeout: 10000 });
  const where = await page.evaluate(() => document.querySelector('.cm-activeLine')?.textContent ?? '');
  expect(where.includes('spin'), `the cursor is on the line of the fault (${JSON.stringify(where)})`);
  await shot('problem-opens-the-file');
  await page.keyboard.press('Control+j');
  await section('schematic');

  step('the page and the file agree: another program’s edit redraws the schematic, and so does its deletion');
  layout.write(`version = 1\n\n[${board}.sheets.buck.parts.c_in]\nrotate = 90\n`);
  await waitHinted(buckCap, true);
  expect((await hinted(buckCap)) === true, 'the part is marked after an edit made outside the page');
  layout.remove();
  await waitHinted(buckCap, false);
  expect((await hinted(buckCap)) === false, 'and unmarked when the file is deleted');
}
