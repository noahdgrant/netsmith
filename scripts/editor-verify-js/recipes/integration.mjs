// The shared selection, the command registry and the problems dock together: a part found with go
// to anything, the dock opened with Ctrl+J, F8 to a problem, the top bar chip and the dock row
// naming the same part, and the palette (and `!`) driving the dock. Parts are found by their path
// from /api/board; the mistake is put in the declaration of a part so that a row can select it.
import { whenIdle } from '../settle.mjs';
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, section }) {
  const file = path.join(project, 'src/main.nsm');
  const good = fs.readFileSync(file, 'utf8');
  const board = 'TwoBoards/led';
  const chip = async () => ((await page.locator('.selchip').count()) ? await page.locator('.selchip').innerText() : '');
  const palette = async (text) => {
    await page.keyboard.press('Control+Shift+P');
    await page.waitForSelector('.qp[data-mode=command]');
    await page.keyboard.type(text);
    await page.waitForSelector('.qp-row');
  };
  try {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.selectOption('.boardpick select', board);
    await section('schematic');
    await page.waitForSelector('svg.schematic');
    const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
    // The resistor is one line of the source: `.r1: Resistor {}`.
    const target = detail.parts.find((p) => p.kind === 'Resistor');
    expect(!!target && target.decl.line === target.decl.endLine, 'the board has a resistor declared on one line');
    const other = detail.parts.find((p) => p !== target && p.kind !== 'Resistor');

    step('go to anything finds another part; Enter selects it; the chip names it');
    await page.keyboard.press('Control+p');
    await page.waitForSelector('.qp[role=dialog]');
    await page.keyboard.type(other.refdes.toLowerCase());
    await page.waitForSelector('.qp-row[role=option]');
    await page.keyboard.press('Enter');
    await page.waitForFunction((r) => document.querySelector('.selchip')?.textContent.includes(r), other.refdes);
    expect((await chip()).includes(other.refdes), `the chip shows ${other.refdes}`);

    step('Ctrl+J opens the dock: nothing is wrong, and the selection stays');
    await page.keyboard.press('Control+j');
    await page.waitForSelector('.dock');
    expect((await page.locator('.dock-empty').innerText()).includes('No problems'), 'a calm "No problems"');
    expect((await chip()).includes(other.refdes), 'the chip is still there');
    await shot('dock-empty-selected');

    step('a mistake in the resistor, saved to disk: the dock lists it');
    fs.writeFileSync(file, good.replace('.r1: Resistor {}', '.r1: Resistor { .nonsense &= 1V }'));
    await page.waitForSelector('.dock .problem.error');
    const row = await page.locator('.dock .problem.error').first().innerText();
    expect(row.includes('nonsense') && row.includes(`main.nsm:${target.decl.line}`), `the row names the line (${row.replace(/\s+/g, ' ')})`);
    await shot('dock-with-error');

    step('F8 goes to the problem: the dock row, the source cursor and the chip agree');
    await page.keyboard.press('F8');
    await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('nonsense'), null);
    expect((await page.locator('.problem.active').count()) === 1, 'one dock row is marked');
    await page.waitForFunction((r) => document.querySelector('.selchip')?.textContent.includes(r), target.refdes);
    expect((await chip()).includes(target.refdes), `the chip names ${target.refdes}, the part whose line holds the mistake`);
    expect(!(await chip()).includes(other.refdes), `and no longer ${other.refdes}`);
    // At 1280 px the chip is squeezed: it shortens its name rather than run into its neighbours.
    const bar = await page.evaluate(() => {
      const r = (s) => document.querySelector(s)?.getBoundingClientRect();
      const c = r('.selchip');
      return { chip: c, search: r('.cmdk'), board: r('.boardpick'), main: r('.selchip-main'), btn: r('.selchip-btn') };
    });
    expect(bar.chip.left >= bar.board.right && bar.chip.right <= bar.search.left + 1, 'the chip sits between the board picker and the search box');
    expect(bar.main.right <= bar.btn.left + 1, 'its name stops before its buttons');
    await shot('f8-source');
    await page.keyboard.press('Shift+F8');
    expect((await page.locator('.problem.active').count()) === 1, 'Shift+F8 with one problem stays on it');

    step('the same selection is in the schematic after the build is fixed');
    fs.writeFileSync(file, good);
    await section('schematic');
    await page.waitForSelector('.section:not(.away) .stale-strip', { state: 'detached' });
    await page.waitForSelector('.dock-empty');
    expect((await chip()).includes(target.refdes), 'the chip keeps the part after the fix');
    expect(await page.evaluate((r) => !!document.querySelector(`svg.schematic .part.mark:not(.linked)[data-ref="${r}"]`), target.refdes), 'the schematic marks it');
    await shot('fixed-selected');

    step('the palette toggles the dock: Toggle Problems closes it, again opens it');
    await palette('toggle problems');
    expect((await page.locator('.qp-row').first().innerText()).includes('Toggle Problems'), 'Toggle Problems is the first result');
    expect((await page.locator('.qp-row').first().locator('kbd').allInnerTexts()).join('+') === 'Ctrl+J', 'it shows Ctrl+J');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.dock', { state: 'detached' });
    expect(true, 'the dock closed');
    await palette('toggle problems');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.dock');
    expect(true, 'the dock opened again');

    step('the palette lists Next and Previous problem with F8 and Shift+F8');
    await page.keyboard.press('Control+Shift+P');
    await page.waitForSelector('.qp[data-mode=command]');
    await page.keyboard.type('problem');
    await page.waitForSelector('.qp-row');
    const rows = await page.locator('.qp-row').allInnerTexts();
    const has = (t, k) => rows.some((r) => r.includes(t) && r.replace(/\s+/g, '').includes(k));
    expect(has('Next problem', 'F8'), 'Next problem, F8');
    expect(has('Previous problem', 'Shift'), 'Previous problem, Shift F8');
    expect(has('Show problems of this view only', ''), 'Show problems of this view only');
    await shot('palette-problems');
    await page.keyboard.press('Escape');

    step('"Show problems of this view only" turns the dock\'s filter on');
    await palette('this view only');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.dock-check input:checked');
    expect(true, 'the checkbox is checked');
    await palette('whole project');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.dock-check input:not(:checked)');
    expect(true, 'and "the whole project" turns it off');

    step('the cheat sheet lists the problems group');
    await page.keyboard.press('Escape');
    await page.locator('body').click({ position: { x: 5, y: 400 } });
    await page.keyboard.press('?');
    await page.waitForSelector('.cheat, [role=dialog][aria-label*=hortcut]');
    const sheet = await page.locator('.cheat, [role=dialog][aria-label*=hortcut]').first().innerText();
    expect(/Problems/.test(sheet) && /F8/.test(sheet) && /Ctrl/.test(sheet) && /Toggle Problems/.test(sheet), 'Problems, Toggle Problems, F8 are listed');
    await whenIdle(page);
    await shot('cheat-sheet');
    await page.keyboard.press('Escape');

    step('! in go to anything lists the problems; a pick goes there');
    fs.writeFileSync(file, good.replace('.r1: Resistor {}', '.r1: Resistor { .nonsense &= 1V }'));
    await page.waitForSelector('.dock .problem.error');
    await page.keyboard.press('Control+p');
    await page.waitForSelector('.qp[role=dialog]');
    await page.keyboard.press('Control+a');
    await page.keyboard.type('!nonsense');
    await page.waitForSelector('.qp-row');
    expect((await page.locator('.qp-row').first().innerText()).includes('nonsense'), 'the problem is listed');
    await shot('goto-problem');
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('nonsense'), null);
    expect((await page.locator('.problem.active').count()) === 1, 'the source is at the mistake and its row is marked');

    step('at 1920x1080 the top bar, tabs, view, dock and status bar fit without overlap');
    await page.setViewportSize({ width: 1920, height: 1080 });
    await section('schematic');
    await whenIdle(page);
    const boxes = await page.evaluate(() => {
      const r = (s) => document.querySelector(s)?.getBoundingClientRect();
      return { top: r('.topbar'), strip: r('.pane-strip'), dock: r('.dock'), status: r('.statusbar'), vh: innerHeight };
    });
    expect(boxes.top.bottom <= boxes.strip.top + 1 && boxes.dock.bottom <= boxes.status.top + 1 && boxes.status.bottom <= boxes.vh + 1, 'the bands stack in order inside the window');
    await shot('wide-with-dock');
  } finally {
    fs.writeFileSync(file, good);
  }
}
