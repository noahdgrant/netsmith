// Clearing and finding the layout file (ADR 0061, stage 4): the palette clears a module's entries or the file's, after a question; a
// part with an entry is marked in the drawing; the file is in the project tree and opens in Source; a problem of the layout stage
// opens the file at its line; and a change by another program redraws the page.
import { BOARD, DEFINITION, layoutFile, openLedBoard, partStates } from '../layout-helpers.mjs';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, section }) {
  const layout = layoutFile(project);
  layout.remove();
  await page.setViewportSize({ width: 1400, height: 900 });
  const parts = await openLedBoard(page);
  await page.waitForTimeout(400);
  const put = (text) =>
    page.evaluate(
      async ([b, body]) => {
        const r = await fetch(`/api/schematic-hints?board=${encodeURIComponent(b)}`, { method: 'PUT', headers: { 'X-Netsmith': '1', Accept: 'application/json', 'Content-Type': 'text/plain' }, body });
        return r.status;
      },
      [BOARD, text],
    );
  const settle = async () => {
    await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
    await page.waitForTimeout(500);
  };
  const palette = async (text) => {
    await page.keyboard.press('Control+Shift+P');
    await page.waitForSelector('.qp');
    await page.keyboard.press('Control+a');
    await page.keyboard.type(`>${text}`);
    await page.waitForTimeout(200);
  };
  const canvas = () => page.locator('.schematic-host').click({ position: { x: 6, y: 6 } });

  step('with no entry the clear commands are not offered');
  await canvas();
  await palette('clear');
  const none = (await page.locator('.qp-row').allInnerTexts()).join(' ');
  expect(!/layout of this module|every layout entry/.test(none), `neither is in the palette (${none.replace(/\n/g, ' ')})`);
  await page.keyboard.press('Escape');

  step('a part with an entry wears the hand-arranged outline, in the theme’s own token');
  expect((await put(`version = 1\n\n[${DEFINITION}.parts.r1]\ndx = 2.54\ndy = 0\n\n[${DEFINITION}.parts.led]\ndy = 2.54\ndx = 0\n\n[Other.parts.x]\ndx = 1.27\ndy = 0\n`)) === 200, 'the file is written');
  await page.waitForFunction((r) => document.querySelector(`svg.schematic g.part[data-ref="${r}"]`)?.previousElementSibling?.previousElementSibling?.previousElementSibling?.classList.contains('on'), parts.r1, { timeout: 20000 });
  await page.waitForTimeout(500);
  const marked = await partStates(page);
  expect(marked[parts.r1].hinted && marked[parts.led].hinted && !marked[parts.power_in].hinted, 'r1 and led are marked, the connector is not');
  const colours = await page.evaluate((r) => {
    const g = document.querySelector(`svg.schematic g.part[data-ref="${r}"]`);
    const hint = g.previousElementSibling.previousElementSibling.previousElementSibling;
    const probe = document.createElement('i');
    probe.style.color = 'var(--sch-muted)';
    document.body.appendChild(probe);
    const token = getComputedStyle(probe).color;
    probe.remove();
    return { stroke: getComputedStyle(hint).stroke, token, dash: getComputedStyle(hint).strokeDasharray };
  }, parts.r1);
  expect(colours.stroke === colours.token && colours.dash !== 'none', `the outline is dashed in --sch-muted (${colours.stroke})`);
  await shot('marked');

  step('the file is in the project tree under "Schematic layout", and it opens in Source with TOML colouring');
  await page.waitForSelector('.sidebar [data-role="schematic_layout"], .sidebar >> text=Schematic layout', { timeout: 15000 });
  const treeRow = page.locator('.sidebar .ft-row', { hasText: 'main.sch.toml' }).first();
  expect((await treeRow.count()) === 1, 'a row for main.sch.toml');
  await treeRow.dblclick().catch(() => treeRow.click());
  await page.waitForTimeout(300);
  await section('source');
  await page.waitForSelector('.cm-content');
  const crumbs = await page.locator('.filetabs, .tabs').first().innerText();
  expect(crumbs.includes('main.sch.toml'), `its tab is open (${crumbs.replace(/\n/g, ' ')})`);
  const coloured = await page.evaluate(() => [...document.querySelectorAll('.cm-content .cm-line span')].length);
  expect(coloured > 3, `the text is coloured as TOML (${coloured} spans)`);
  await shot('in-source');
  await section('schematic');

  step('Clear the layout of this module asks first; Cancel keeps the file, the question’s yes removes this board’s own entries only');
  await canvas();
  await palette('clear the layout of this module');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="confirm"]');
  const q = await page.locator('[data-testid="confirm"]').innerText();
  expect(/2 entries/.test(q) && /main\.sch\.toml/.test(q), `the question says what goes (${q.replace(/\n/g, ' ')})`);
  await shot('confirm');
  await page.locator('[data-testid="confirm"] button', { hasText: 'Cancel' }).click();
  expect(/\[LedBoard\.parts\.r1\]/.test(layout.text()), 'Cancel: the file is as it was');
  await palette('clear the layout of this module');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="confirm-yes"]');
  await page.locator('[data-testid="confirm-yes"]').click();
  await settle();
  const left = layout.text();
  expect(!/LedBoard/.test(left) && /\[Other\.parts\.x\]/.test(left), `this board’s entries are gone and another board’s stay (${JSON.stringify(left)})`);
  expect(!Object.values(await partStates(page)).some((s) => s.hinted), 'no part is marked any more');

  step('Clear every layout entry of the file empties the file for every board');
  await canvas();
  await palette('clear every layout entry');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="confirm"]');
  const all = await page.locator('[data-testid="confirm"]').innerText();
  expect(/1 entry/.test(all) && /every board/.test(all), `the question says the file’s count (${all.replace(/\n/g, ' ')})`);
  await page.locator('[data-testid="confirm-yes"]').click();
  await settle();
  expect(layout.text() === 'version = 1\n', `the file keeps \`version = 1\` (${JSON.stringify(layout.text())})`);

  step('a file that does not read: its problem opens the file in Source at the line');
  layout.write('version = 1\n\n[LedBoard.parts.r1]\ndx = 2.54\ndy = 0\nspin = 3\n');
  await page.waitForFunction(async () => (await (await fetch('/api/analysis', { cache: 'no-store' })).json()).problems.some((p) => p.stage === 'layout'), null, { timeout: 15000 });
  await page.keyboard.press('Control+j');
  await page.waitForSelector('.dock .problem.warning');
  await page.locator('.dock .problem.warning').first().click();
  await page.waitForSelector('.cm-content');
  await page.waitForTimeout(500);
  const where = await page.evaluate(() => {
    const line = document.querySelector('.cm-activeLine');
    return { text: line?.textContent ?? '', tab: document.querySelector('.filetabs .active, .tabs .active, [role=tab][aria-selected=true]')?.textContent ?? '' };
  });
  expect(where.text.includes('spin'), `the cursor is on the line of the fault (${JSON.stringify(where.text)})`);
  expect(where.tab.includes('main.sch.toml'), `in the file's tab (${where.tab})`);
  await shot('problem-opens-the-file');
  await page.keyboard.press('Control+j');
  await section('schematic');

  step('the page and the file agree: another program’s edit redraws the schematic');
  layout.write(`version = 1\n\n[${DEFINITION}.parts.r1]\ndx = 5.08\ndy = 0\n`);
  await page.waitForFunction((r) => document.querySelector(`svg.schematic g.part[data-ref="${r}"]`)?.previousElementSibling?.previousElementSibling?.previousElementSibling?.classList.contains('on'), parts.r1, { timeout: 20000 });
  expect((await partStates(page))[parts.r1].hinted, 'the part is marked after an edit made outside the page');
  layout.remove();
  await page.waitForFunction((r) => !document.querySelector(`svg.schematic g.part[data-ref="${r}"]`)?.previousElementSibling?.previousElementSibling?.previousElementSibling?.classList.contains('on'), parts.r1, { timeout: 20000 });
  expect(!(await partStates(page))[parts.r1].hinted, 'and unmarked when the file is deleted');
}
