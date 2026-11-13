// The last two extensions of the comparison (ADR 0059, "Built in the extensions"): a part that is another library part says
// what it was and what it is, and the checker's warnings and notes are compared by severity, between a ref and the files on
// disk and between two refs. It needs `launch --git` (see compare.mjs and fixtures/gitrepo.mjs). The kind is changed in the
// work tree (a header becomes a socket); the warnings are on two branches the recipe makes, `warn-wide` and `warn-narrow`, a
// small board whose supply is drawn with a wide and with a narrow track (IPC-2221, ADR 0036), and removes at the end.
import { whenIdle } from '../settle.mjs';
import fs from 'node:fs';
import path from 'node:path';

import { git } from '../fixtures/gitrepo.mjs';

export const fixture = 'two_boards';
export const needs = 'git';

const heater = (width) => `import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Heater {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= 0A..2A
        .min_track &= ${width}..1m
    }
    .source: Rail {
        .pos.net &= "5V"
        .neg.net &= "GND"
        .current &= -2A..0A
    }
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
`;

export async function run({ page, shot, step, expect, project, resetRepo, allowConsole }) {
  // The recipe leaves the board `Heater` by checking out another branch while the page may still ask for it: the server
  // says 404, which is the recipe's own cleanup and not a fault of the page (it failed on main this way, once in a while).
  allowConsole(/api\/(board\?name|schematic-hints\?board|schematic\.svg\?board)=Heater/);
  const main = path.join(project, 'src/main.nsm');
  const tryGit = (...args) => {
    try {
      git(project, ...args);
    } catch {
      /* the branch is not there */
    }
  };
  const url = page.url();
  // The branches are made with the page away: it asks the server for the files it has open.
  await page.goto('about:blank');
  resetRepo();
  for (const b of ['warn-wide', 'warn-narrow']) tryGit('branch', '-D', b);
  git(project, 'checkout', '-q', '-b', 'warn-wide');
  fs.writeFileSync(main, heater('1mm'));
  git(project, 'commit', '-q', '-am', 'A board with a wide track');
  git(project, 'checkout', '-q', '-b', 'warn-narrow');
  fs.writeFileSync(main, heater('0.2mm'));
  git(project, 'commit', '-q', '-am', 'The same board with a narrow track');
  git(project, 'checkout', '-q', 'feature');
  try {
    await page.goto(url);
    await page.evaluate(() => (localStorage.clear(), sessionStorage.clear()));
    await page.reload();
    await page.waitForSelector('.sidebar > *', { state: 'attached' });
    await page.setViewportSize({ width: 1920, height: 1080 });
    const text = (sel) => page.locator(sel).first().innerText();
    const visible = (sel) => page.locator(`.section:not(.away) ${sel}`);
    const summary = async () => {
      await page.waitForSelector('[data-testid="cmp-summary"]');
      return text('[data-testid="cmp-summary"]');
    };
    const choose = async (slot, name) => {
      if (!(await page.locator('[data-testid="ref-popover"]').count())) await visible('[data-testid="ref-button"]').click();
      await page.waitForSelector('[data-testid="ref-popover"]');
      if (slot) await page.click(`[data-testid="ref-slot-${slot}"]`);
      await page.locator(`[data-testid="ref-item"][data-name="${name}"]`).click();
    };
    const sentenceIs = (re) => page.waitForFunction((src) => new RegExp(src).test(document.querySelector('[data-testid="cmp-summary"]')?.textContent ?? ''), re);
    const tab = async (id) => {
      await page.click(`.cmp-tab[data-tab="${id}"]`);
      await whenIdle(page);
    };
    const tile = async (id) => (await text(`[data-testid="tile-${id}"]`)).replace(/\s+/g, ' ');

    // ------------------------------------------------------------------ the kind of a part
    step('a header that became a socket: the sentence, the row and the tile say it is another library part');
    await page.keyboard.press('8');
    await page.waitForSelector('[data-testid="cmp-start"]');
    await choose(null, 'HEAD');
    await summary();
    fs.writeFileSync(main, fs.readFileSync(main, 'utf8').replace('.power_in: Header1x2', '.power_in: Socket1x2'));
    await sentenceIs("1 part's kind changed from Header1x2 to Socket1x2");
    const sentence = await text('[data-testid="cmp-summary"]');
    expect(!/value changed/.test(sentence), `the value that came with it is not counted again (${sentence})`);
    expect(/Parts 1/.test(await tile('parts')) && /1 kind changed/.test(await tile('parts')), `the Parts tile says a kind changed (${await tile('parts')})`);
    const row = page.locator('[data-testid="cmp-row"]', { has: page.locator('.cr-title', { hasText: /^power_in$/ }) });
    const rowText = (await row.innerText()).replace(/\s+/g, ' ');
    expect(/Socket1x2/.test(rowText) && /Header1x2/.test(rowText) && /kind/.test(rowText), `the row has the kind before and after (${rowText})`);
    expect((await row.locator('.vchip').allInnerTexts()).join() === 'Header1x2,Socket1x2', 'the chips are the two kinds');
    expect((await row.locator('[data-testid="kind-changed"]').count()) === 1, 'a pill says the kind changed');
    expect(/Changed/.test(rowText), 'and the badge says it changed');
    await shot('kind-changed');
    // A part whose value changed and kind did not has no such pill.
    fs.writeFileSync(main, fs.readFileSync(main, 'utf8').replace('.r1: Resistor {}', '.r1: Resistor { .resistance &= 150ohm +/- 1% }'));
    await page.waitForFunction(() => document.querySelectorAll('[data-testid="cmp-row"]').length >= 3, null);
    const r1 = page.locator('[data-testid="cmp-row"]', { has: page.locator('.cr-title', { hasText: /^r1$/ }) });
    expect(/Resistor/.test(await r1.innerText()) && (await r1.locator('[data-testid="kind-changed"]').count()) === 0, 'a row says what the part is, and no kind changed for a value');
    fs.writeFileSync(main, fs.readFileSync(main, 'utf8').replace('.power_in: Socket1x2', '.power_in: Header1x2').replace('.r1: Resistor { .resistance &= 150ohm +/- 1% }', '.r1: Resistor {}'));
    await sentenceIs('No circuit changes');

    // ------------------------------------------------------------------ warnings
    step('two refs: a narrow track is two new warnings, said by severity in the sentence, the tile and the tab');
    await choose('base', 'warn-wide');
    await choose('against', 'warn-narrow');
    await sentenceIs('2 new warnings');
    const warn = await summary();
    expect(/^2 new warnings/.test(warn.trim()) && !/error/.test(warn), `the sentence (${warn})`);
    const problems = await tile('problems');
    expect(/Problems 2 new/.test(problems) && /\+2 warnings/.test(problems), `the Problems tile (${problems})`);
    expect((await page.locator('[data-testid="tile-problems"] .tchip.del').count()) >= 1, 'a new warning is red');
    await tab('problems');
    const sev = await page.locator('[data-testid^="sev-"]').allInnerTexts();
    expect(sev.length === 4 && /^All \d+$/.test(sev[0]) && /^Warnings 2$/.test(sev[2]), `the severity filter has the counts (${sev.join(', ')})`);
    expect((await page.locator('[data-testid="cmp-row"][data-status="new"]').count()) >= 2, 'the new warnings are rows');
    expect((await page.locator('[data-testid="cmp-row"][data-kind="problem"] .cr-title').allInnerTexts()).some((t) => /IPC-2221/.test(t)), 'with the guideline that was broken');
    await shot('warnings-new');
    await page.click('[data-testid="sev-warning"]');
    await whenIdle(page);
    expect((await page.locator('[data-testid="cmp-row"]').count()) === 2, 'the filter leaves the two warnings');
    // the same triangle and the same colour as everywhere else a warning is shown (warning-marks)
    const mark = await page.evaluate(() => {
      const probe = document.createElement('i');
      probe.style.color = 'var(--warning)';
      document.body.append(probe);
      const token = getComputedStyle(probe).color;
      probe.remove();
      const icons = [...document.querySelectorAll('[data-testid="cmp-row"] svg[data-icon="warning"]')];
      return { n: icons.length, same: icons.every((i) => i.querySelector('path')?.getAttribute('d') === 'M8 2.2l6.2 11H1.8z' && getComputedStyle(i).color === token) };
    });
    expect(mark.n === 2 && mark.same, `the rows of the warnings carry the warning triangle in the --warning colour (${mark.n} icons)`);
    await page.click('[data-testid="sev-error"]');
    await whenIdle(page);
    expect((await page.locator('[data-testid="cmp-row"]').count()) === 0 && /No errors are new or resolved/.test(await text('.cmp-scroll')), 'no error is new: said, with the way back');
    await page.click('[data-testid="sev-all"]');

    step('the other way round the warnings are resolved: the sentence, the tile and the tab are green');
    await choose('base', 'warn-narrow');
    await choose('against', 'warn-wide');
    await sentenceIs('2 warnings resolved');
    const gone = await tile('problems');
    expect(/Problems 0 new/.test(gone) && /−2 warnings|-2 warnings/.test(gone), `the tile (${gone})`);
    expect((await page.locator('[data-testid="tile-problems"] .tchip.add').count()) >= 1, 'a resolved warning is green');
    await tab('problems');
    expect((await page.locator('[data-testid="cmp-row"][data-status="resolved"]').count()) >= 2, 'the resolved warnings are rows');
    await shot('warnings-resolved');
  } finally {
    await page.evaluate(() => (localStorage.clear(), sessionStorage.clear())).catch(() => {});
    await page.goto('about:blank').catch(() => {});
    for (const b of ['warn-wide', 'warn-narrow']) tryGit('branch', '-D', b);
    resetRepo();
    await page.goto(url).catch(() => {});
  }
}
