// The comparison beyond the circuit of one board (ADR 0059, "Built in the extensions"): what changed in the system (its
// boards, ports, joins, mates, ground groups and the rails only it explains) and on the PCB (parts moved, the outline, the
// layers, keepouts and cutouts, read from the layout file and the placed board). It needs `launch --git` (see compare.mjs and
// fixtures/gitrepo.mjs). The earlier design on `main` has a 30 mA output, so the system's port and the LED board's input differ.
// The PCB needs a layout file: the recipe commits one on a branch of its own, `pcb-layout`, puts the resistor somewhere else in
// the work tree, and removes the branch at the end, so the other recipes see the repository they expect.
import fs from 'node:fs';
import path from 'node:path';

import { git } from '../fixtures/gitrepo.mjs';

export const fixture = 'two_boards';
export const needs = 'git';

const layout = (at, rotation, side) =>
  `version = 1\n\n[LedBoard.parts."r1"]\nat = [${at}]\nrotation = ${rotation}\nside = "${side}"\nsource = "hand"\n\n[PowerBoard.parts."stage.output_cap"]\nat = [10.0, 4.0]\nrotation = 0.0\nside = "top"\nsource = "hand"\n`;

export async function run({ page, shot, step, expect, section, project, resetRepo }) {
  const file = path.join(project, 'src/main.pcb.toml');
  const tryGit = (...args) => {
    try {
      git(project, ...args);
    } catch {
      /* the branch is not there */
    }
  };
  // A branch with a layout file, made from `feature` and left again before the page looks. The page is away meanwhile: it asks the
  // server for the files it has open, and one that is not there for a moment is a 404 in the console.
  const url = page.url();
  await page.goto('about:blank');
  resetRepo();
  tryGit('branch', '-D', 'pcb-layout');
  git(project, 'checkout', '-q', '-b', 'pcb-layout');
  fs.writeFileSync(file, layout('5.0, 2.0', '0.0', 'top'));
  git(project, 'add', 'src/main.pcb.toml');
  git(project, 'commit', '-q', '-m', 'A layout: the resistor by hand');
  git(project, 'checkout', '-q', 'feature');
  // On disk the resistor is 2.5 mm to the right, turned, on the other face.
  fs.writeFileSync(file, layout('7.5, 2.0', '90.0', 'bottom'));
  try {
    await page.goto(url);
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await page.waitForSelector('.sidebar > *', { state: 'attached' });
    await page.setViewportSize({ width: 1920, height: 1080 });
    const settle = (ms = 300) => page.waitForTimeout(ms);
    const text = (sel) => page.locator(sel).first().innerText();
    const visible = (sel) => page.locator(`.section:not(.away) ${sel}`);
    const summary = async () => {
      await page.waitForSelector('[data-testid="cmp-summary"]', { timeout: 60000 });
      return text('[data-testid="cmp-summary"]');
    };
    const choose = async (slot, name) => {
      if (!(await page.locator('[data-testid="ref-popover"]').count())) await visible('[data-testid="ref-button"]').click();
      await page.waitForSelector('[data-testid="ref-popover"]');
      if (slot) await page.click(`[data-testid="ref-slot-${slot}"]`);
      await page.locator(`[data-testid="ref-item"][data-name="${name}"]`).click();
    };
    const tab = async (id) => {
      await page.click(`.cmp-tab[data-tab="${id}"]`);
      await settle(150);
    };
    const tile = async (id) => (await text(`[data-testid="tile-${id}"]`)).replace(/\s+/g, ' ');
    const rows = () => page.locator('[data-testid="cmp-row"]');
    const rowText = async (title) =>
      (await page.locator('[data-testid="cmp-row"]', { has: page.locator('.cr-title', { hasText: title }) }).first().innerText()).replace(/\s+/g, ' ');

    step('main: the sentence has the system, and the tiles and tabs have a Systems and a PCB one');
    await page.keyboard.press('8');
    await page.waitForSelector('[data-testid="cmp-start"]');
    await choose(null, 'main');
    const sentence = await summary();
    expect(/system port/.test(sentence), `the sentence says a system port changed (${sentence})`);
    const ids = await page.locator('[data-testid^="tile-"]').evaluateAll((els) => els.map((e) => e.getAttribute('data-testid')));
    expect(ids.join() === 'tile-parts,tile-nets,tile-rails,tile-bom,tile-systems,tile-pcb,tile-problems', `seven tiles (${ids.join()})`);
    const tabs = await page.locator('.cmp-tab').evaluateAll((els) => els.map((e) => e.getAttribute('data-tab')));
    expect(tabs.join() === 'parts,nets,rails,bom,modules,systems,pcb,problems,source', `the tabs (${tabs.join()})`);
    expect(Number((await text('[data-testid="count-systems"]')).trim()) >= 1, 'the Systems tab has a count');
    expect(/Systems [1-9]/.test(await tile('systems')), `the Systems tile has a figure (${await tile('systems')})`);

    step('the Systems tab: the port that changed, with the current before and after, and what the system joins');
    await page.click('[data-testid="tile-systems"]');
    await settle(200);
    expect((await page.getAttribute('.cmp-tab[data-tab="systems"]', 'aria-selected')) === 'true', 'a click on the tile opens the tab');
    const port = await rowText('power.output');
    expect(/current/.test(port) && /30/.test(port) && /50/.test(port) && /Changed/.test(port), `the port row says current 30 to 50 (${port})`);
    expect((await page.locator('[data-testid="cmp-row"][data-kind="port"]').count()) >= 1, 'it is a row of kind port');
    const board = await text('[data-testid="cmp-row"] .cr-board');
    expect(board.trim() === 'TwoBoards', `the row names its system (${board})`);
    await shot('systems');

    step('the layout file is new on disk for main: the two parts it places are moved from where the placer put them');
    expect(/2 parts moved on the PCB/.test(sentence), `the sentence counts them (${sentence})`);
    await tab('pcb');
    expect((await rows().count()) === 2, 'two rows on the PCB tab');
    expect(/Moved/.test(await rowText('r1')) && /Moved/.test(await rowText('stage.output_cap')), 'both are moved');

    step('pcb-layout: one part moved on the PCB, with where it was and where it is');
    await choose('base', 'pcb-layout');
    await page.waitForFunction(() => /1 part moved on the PCB/.test(document.querySelector('[data-testid="cmp-summary"]')?.textContent ?? ''), null, { timeout: 60000 });
    const pcbSentence = await text('[data-testid="cmp-summary"]');
    expect(pcbSentence.trim() === '1 part moved on the PCB', `the whole sentence is the PCB's (${pcbSentence})`);
    expect(/PCB 1/.test(await tile('pcb')) && /1 part moved/.test(await tile('pcb')), `the PCB tile counts it (${await tile('pcb')})`);
    await tab('pcb');
    expect((await rows().count()) === 1, 'one row');
    const moved = await rowText('r1');
    expect(/5, 2 mm/.test(moved) && /7\.5, 2 mm/.test(moved) && /Moved/.test(moved), `the row has the old and the new place (${moved})`);
    await rows().first().locator('.cr-chev').click();
    await page.waitForSelector('[data-testid="cmp-detail"]');
    const detail = (await text('[data-testid="cmp-detail"]')).replace(/\s+/g, ' ');
    expect(/rotation/.test(detail) && /90°/.test(detail) && /side/.test(detail) && /bottom/.test(detail), `the fields that differ are listed (${detail})`);
    await shot('pcb');
    await rows().first().locator('[data-testid="link-base"]').click();
    await page.waitForSelector('[data-testid="diff-panel"]');
    await settle(500);
    expect((await text('[data-testid="diff-path"]')).trim() === 'src/main.pcb.toml', 'Base opens the layout file at the ref beside the file on disk');
    const layoutDiff = await page.locator('[data-testid="diff-side"]').innerText();
    expect(/5\.0, 2\.0/.test(layoutDiff), 'the old position is in the file at the ref');
    expect(/7\.5, 2\.0/.test(layoutDiff) && !(await page.locator('.diff-panel .chip.bad', { hasText: 'deleted' }).count()), 'the new position is in the file on disk, which the editor does not open');
    await shot('pcb-layout-diff');
    await page.keyboard.press('Escape');
    await page.waitForSelector('[data-testid="diff-panel"]', { state: 'detached' });

    step('two refs: the systems of the commits are compared too');
    await choose('base', 'main');
    await page.waitForFunction(() => /system port/.test(document.querySelector('[data-testid="cmp-summary"]')?.textContent ?? ''), null, { timeout: 60000 });
    await choose('against', 'feature');
    await page.waitForSelector('[data-testid="cmp-against"]', { timeout: 60000 });
    expect(/system port/.test(await summary()), 'the sentence of two refs has the system');
    await tab('systems');
    expect(/power\.output/.test(await rowText('power.output')), 'the port row is there for two refs');
    await shot('systems-two-refs');
  } finally {
    // Leave the repository as the other recipes expect it, with the page away while the layout file goes.
    await page.evaluate(() => (localStorage.clear(), sessionStorage.clear())).catch(() => {});
    await page.goto('about:blank').catch(() => {});
    try {
      fs.rmSync(file, { force: true });
    } catch {
      /* gone */
    }
    tryGit('branch', '-D', 'pcb-layout');
    resetRepo();
    await page.goto(url).catch(() => {});
    await page.waitForSelector('.sidebar > *', { state: 'attached' }).catch(() => {});
  }
}
