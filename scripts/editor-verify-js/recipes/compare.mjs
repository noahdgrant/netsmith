// Compare the design with a git ref (ADR 0059): the section, the ref picker, the summary, the tabs, the
// click-through and the Base and Head links, the read-only diff view, the switch that marks the schematic,
// the bill of materials, the power tree and the source, a ref that does not analyse, a progress state and a
// project that is not in a repository. It needs `launch --git`: the project is a throwaway repository whose
// first commit (`main`, tag `v1.0`) is an earlier design (see fixtures/gitrepo.mjs).
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';

import { expectFrameFits } from '../flow-helpers.mjs';
import { noteFacts, overlaps } from '../schematic-helpers.mjs';

export const fixture = 'two_boards';
export const needs = 'git';

const freePort = () =>
  new Promise((resolve, reject) => {
    const s = net.createServer();
    s.listen(0, '127.0.0.1', () => {
      const { port } = s.address();
      s.close(() => resolve(port));
    });
    s.on('error', reject);
  });

export async function run({ page, shot, step, expect, section, project, resetRepo, binary, allowConsole }) {
  resetRepo();
  await page.reload();
  await page.waitForSelector('.sidebar > *', { state: 'attached' });
  await page.setViewportSize({ width: 1920, height: 1080 });
  const text = (sel) => page.locator(sel).first().innerText();
  const settle = (ms = 300) => page.waitForTimeout(ms);
  const visible = (sel) => page.locator(`.section:not(.away) ${sel}`);
  const pick = async (name) => {
    if (!(await page.locator('[data-testid="ref-popover"]').count())) await visible('[data-testid="ref-button"]').click();
    await page.waitForSelector('[data-testid="ref-popover"]');
    await page.locator(`[data-testid="ref-item"][data-name="${name}"]`).click();
  };
  const summary = async () => {
    await page.waitForSelector('[data-testid="cmp-summary"]', { timeout: 60000 });
    return text('[data-testid="cmp-summary"]');
  };
  const tab = async (id) => {
    await page.click(`.cmp-tab[data-tab="${id}"]`);
    await settle(150);
  };
  const rows = () => page.locator('.cmp-group [data-testid="cmp-row"]');
  const rowOf = (title) => page.locator('[data-testid="cmp-row"]', { has: page.locator('.cr-title', { hasText: new RegExp(`^${title}$`) }) });
  const markedWith = (sel) => page.evaluate((s) => document.querySelectorAll(s).length, sel);

  // ------------------------------------------------------------------ the start
  step('the section: key 8, a tab with an icon, nothing chosen yet, so no chip in the status bar');
  await page.keyboard.press('8');
  await page.waitForSelector('.section.sec-compare:not(.away)');
  await page.waitForSelector('[data-testid="cmp-start"]');
  expect((await page.locator('.pane-strip .tab[data-section="compare"] svg').count()) === 1, 'the Compare tab has an icon');
  expect((await page.locator('[data-testid="compare-chip"]').count()) === 0, 'no chip in the status bar while no ref is chosen');
  expect((await text('[data-testid="cmp-start"]')).includes('Compare the circuit with a git ref'), 'it says what the section does');
  expect((await page.locator('[data-testid="compare-suggested"]').innerText()).includes('main'), 'it offers the default branch');
  await shot('start');

  step('the palette has "Compare with a ref…"');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('compare with');
  await page.waitForTimeout(250);
  expect((await page.locator('.qp-row', { hasText: 'Compare with a ref' }).count()) === 1, 'the command is listed');
  await page.keyboard.press('Escape');

  // ------------------------------------------------------------------ the ref picker
  step('the ref picker: HEAD, the default branch, branches, tags and recent commits, with the state of the work tree');
  await visible('[data-testid="ref-button"]').click();
  await page.waitForSelector('[data-testid="ref-popover"]');
  const names = await page.locator('[data-testid="ref-item"]').evaluateAll((els) => els.map((e) => e.getAttribute('data-name')));
  expect(names[0] === 'HEAD' && names[1] === 'main', `HEAD first, then the default branch (${names.slice(0, 3).join(', ')})`);
  expect(['broken', 'feature', 'v1.0'].every((n) => names.includes(n)), 'the branches and the tag are listed');
  expect(names.filter((n) => /^[0-9a-f]{40}$/.test(n)).length === 2, 'the two commits are listed by hash');
  const popover = await text('[data-testid="ref-popover"]');
  expect(popover.includes('checked out') && popover.includes('default branch'), 'the checked out branch and the default branch are said');
  expect(popover.includes('Ada Verify') && /\d+ (min|h) ago|just now/.test(popover), 'a commit has its author and how long ago');
  expect(/uncommitted changes \(compared as is\)/.test(await text('[data-testid="ref-dirty"]')), 'the sentence about the work tree');
  await shot('picker');
  await page.keyboard.type('v1');
  await page.waitForTimeout(150);
  expect((await page.locator('[data-testid="ref-item"]').count()) === 1, 'the search narrows the list to the tag');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid="ref-popover"]', { state: 'detached' });
  expect(true, 'Escape closes the picker');

  // ------------------------------------------------------------------ the comparison
  step('choose main: the sentence from the server, the tiles, the boards, the chip');
  await pick('main');
  const sentence = await summary();
  expect(/1 part added, 1 removed, 2 values changed/.test(sentence) && /headroom shrank from 70 mA to 50 mA/.test(sentence), `the summary sentence (${sentence})`);
  // One model of the counts: the section starts on all the boards, so the sentence, the tiles, the tabs, the chip and the total say one thing.
  expect((await page.locator('[data-testid="cmp-board"]').inputValue()) === 'all', 'the section starts on All boards, not the board in front');
  await settle();
  const tiles = await page.locator('[data-testid^="tile-"]').evaluateAll((els) => els.map((e) => [e.getAttribute('data-testid'), e.innerText.replace(/\s+/g, ' ')]));
  expect(tiles.length === 5, 'five tiles');
  const tile = (id) => tiles.find((t) => t[0] === `tile-${id}`)[1];
  expect(/Parts 4/.test(tile('parts')) && /\+1/.test(tile('parts')) && /−1|-1/.test(tile('parts')) && /renumbered/.test(tile('parts')), `parts tile (${tile('parts')})`);
  expect(/Rails/.test(tile('rails')) && /1 headroom shrank/.test(tile('rails')), `rails tile says a headroom shrank (${tile('rails')})`);
  expect(/BOM cost/.test(tile('bom')) && /lines differ/.test(tile('bom')), `BOM tile (${tile('bom')})`);
  expect(/Problems None/.test(tile('problems')), `problems tile (${tile('problems')})`);
  expect(/uncommitted changes \(compared as is\)/.test(await text('.cmp-meta')), 'the work tree has changes: said under the sentence');
  const chip = await text('[data-testid="compare-chip"]');
  expect(/vs\s+main/.test(chip) && /\d+ circuit changes/.test(chip), `the status bar chip says circuit changes (${chip.replace(/\s+/g, ' ')})`);
  const chipN = Number(chip.match(/(\d+) circuit changes?/)[1]);
  const totalN = Number((await text('[data-testid="cmp-total"]')).match(/\d+/)[0]);
  const tileSum = ['parts', 'nets', 'rails'].reduce((n, id) => n + Number(tile(id).match(/ (\d+) /)?.[1] ?? NaN), 0);
  const tabSum = (await Promise.all(['parts', 'nets', 'rails', 'modules'].map(async (id) => Number(await text(`[data-testid="count-${id}"]`))))).reduce((a, b) => a + b, 0);
  expect(chipN === totalN && totalN === tileSum && tileSum === tabSum, `one number everywhere: the chip ${chipN}, the section's total ${totalN}, the tiles ${tileSum}, the tabs ${tabSum}`);
  expect(/line/.test((await page.locator('[data-testid="compare-chip"]').getAttribute('data-tip')) ?? ''), 'the chip\'s tooltip has the source lines');
  const boardSentence = async (name) => {
    await page.selectOption('[data-testid="cmp-board"]', name);
    await settle(250);
    return { sentence: await text('[data-testid="cmp-summary"]'), total: Number((await text('[data-testid="cmp-total"]')).match(/\d+/)[0]), parts: Number((await text('[data-testid="tile-parts"]')).replace(/\s+/g, ' ').match(/ (\d+) /)?.[1] ?? NaN) };
  };
  const ledOnly = await boardSentence('TwoBoards/led');
  expect(ledOnly.sentence !== sentence && ledOnly.total < chipN && ledOnly.total >= 1, `one board selected: its own sentence (${ledOnly.sentence}) and its own total (${ledOnly.total} of ${chipN})`);
  const powerOnly = await boardSentence('TwoBoards/power');
  expect(ledOnly.total + powerOnly.total === chipN && ledOnly.parts + powerOnly.parts === 4, `the boards add up to the whole (${ledOnly.total} + ${powerOnly.total} = ${chipN}; parts ${ledOnly.parts} + ${powerOnly.parts})`);
  expect((await boardSentence('all')).sentence === sentence, 'All boards says the server\'s sentence again');
  const options = await page.locator('[data-testid="cmp-board"] option').allInnerTexts();
  expect(options.length === 3 && /All boards/.test(options[0]) && /TwoBoards\/led \(\d+ changes?\)/.test(options[1]), `the board selector (${options.join(' | ')})`);
  await shot('summary-and-parts');

  step('Parts: an added part, a removed one, two values changed, a renumbered one, with before and after');
  const parts = await rows().evaluateAll((els) => els.map((e) => ({ status: e.dataset.status, text: e.innerText.replace(/\s+/g, ' ') })));
  expect(parts.length === 4, `four rows (${parts.map((p) => p.status).join(',')})`);
  const added = parts.find((p) => p.status === 'added');
  expect(/stage\.input_cap/.test(added.text) && /Capacitor/.test(added.text) && /Added/.test(added.text), `the added capacitor (${added.text})`);
  const removed = parts.find((p) => p.status === 'removed');
  expect(/debug/.test(removed.text) && /Removed/.test(removed.text) && /Header/.test(removed.text), `the removed header (${removed.text})`);
  const r1 = rowOf('r1');
  const chips = await r1.locator('.vchip').allInnerTexts();
  expect(chips.join() === '150,100', `r1's value, before and after (${chips.join(' -> ')})`);
  expect((await r1.locator('.cr-des').innerText()).trim() === 'R1', 'its designator');
  expect(/Changed/.test(await r1.innerText()), 'badged as changed');
  expect((await rowOf('led').locator('.vchip').count()) === 2, 'the LED the solver then picked is another part');
  expect(!(await page.locator('[data-testid="cmp-renumbered"] [data-testid="cmp-row"]').first().isVisible()), 'the renumbered parts are folded away');
  await page.locator('[data-testid="cmp-renumbered"] summary').click();
  const renum = await page.locator('[data-testid="cmp-renumbered"] [data-testid="cmp-row"]').evaluateAll((els) => els.map((e) => e.innerText.replace(/\s+/g, ' ')));
  expect(renum.length === 2 && renum.every((t) => /Renumbered/.test(t)), `two renumbered parts, listed apart (${renum.join(' | ')})`);
  expect(/J2 → J1|J2\s*J1/.test(renum.join(' ')) || /J2/.test(renum.join(' ')), 'with the designators of each side');
  await page.locator('[data-testid="cmp-renumbered"] summary').click();

  step('the list is as tall as its rows with a foot that counts right under the last one, and scrolls inside only past the height the page has; the summary and the tabs stay put; a column of help stands in the margin on a wide window');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await settle(400);
  const cmpSel = { frame: '.cmp-scroll', scroller: '.cmp-scroll', foot: '.cmp-foot', section: '.section.sec-compare' };
  const wide = await page.evaluate(() => {
    const foot = document.querySelector('.cmp-foot');
    const vh = document.querySelector('.section.sec-compare .vh');
    return {
      footText: foot?.innerText.replace(/\s+/g, ' '),
      tilesOutside: !document.querySelector('.cmp-scroll').contains(document.querySelector('.cmp-tiles')) && !document.querySelector('.cmp-scroll').contains(document.querySelector('.cmp-tabs')),
      help: vh ? { left: vh.getBoundingClientRect().left, text: vh.innerText.replace(/\s+/g, ' '), label: vh.getAttribute('aria-label') } : null,
      cardRight: document.querySelector('.cmp-scroll').getBoundingClientRect().right,
    };
  });
  const partsFit = await expectFrameFits(page, expect, 'Compare, parts', cmpSel);
  expect(!partsFit.scrolls && /\d+ rows?/.test(wide.footText ?? ''), `the foot counts the rows (${wide.footText}) and a short list does not scroll`);
  expect(wide.tilesOutside, 'the tiles and the tabs are outside the scrolling list');
  expect(wide.help && wide.help.left >= wide.cardRight + 8 && /Base|Head/.test(wide.help.text) && wide.help.label === 'About this view', `the help column is beside the list (${wide.help?.text.slice(0, 50)}...)`);
  await shot('compare-wide');
  for (const tab of ['bom', 'nets', 'rails']) {
    await page.click(`#cmp-tab-${tab}`);
    await settle(300);
    if (await page.locator('.cmp-foot').count()) await expectFrameFits(page, expect, `Compare, ${tab}`, cmpSel);
  }
  await page.click('#cmp-tab-parts');
  await settle(300);
  await page.setViewportSize({ width: 1280, height: 520 });
  await settle(400);
  const tight = await expectFrameFits(page, expect, 'Compare, parts at 1280 x 520', cmpSel);
  expect(tight.scrolls, 'a short window makes the list scroll inside its frame, the foot kept at its end');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await settle(400);

  step('expand a row: the fields that changed, each with its old and new value');
  await r1.locator('.cr-chev').click();
  const fields = await r1.locator('.cr-field dt').allInnerTexts();
  expect(fields.includes('value') && fields.includes('lcsc'), `the changed fields (${fields.join(', ')})`);
  await shot('parts-expanded');
  await r1.locator('.cr-chev').click();

  step('filter, and group by module');
  await page.fill('.cmp-filter input', 'cap');
  await settle(150);
  expect((await rows().count()) === 1, 'the filter finds the capacitor');
  await page.fill('.cmp-filter input', '');
  await page.click('.seg-btn:has-text("By module")');
  await settle(150);
  const heads = await page.locator('.cmp-group-head').allInnerTexts();
  expect(heads.some((h) => /On the board/.test(h)) && heads.some((h) => /stage/.test(h)), `groups by module (${heads.map((h) => h.replace(/\s+/g, ' ')).join(' | ')})`);
  await page.click('.seg-btn:has-text("Flat")');

  step('Nets: a renamed net with both names, nets that gained a pin');
  await tab('nets');
  const nets = await rows().evaluateAll((els) => els.map((e) => ({ status: e.dataset.status, text: e.innerText.replace(/\s+/g, ' ') })));
  const renamed = nets.find((n) => n.status === 'renamed');
  expect(/3V3/.test(renamed.text) && /VOUT/.test(renamed.text) && /was VOUT/.test(renamed.text) && /Renamed/.test(renamed.text), `the renamed net (${renamed.text})`);
  const vin = rowOf('VIN');
  await vin.locator('.cr-chev').click();
  expect(/\+/.test(await vin.locator('.cr-pins').innerText()) && /\.\d/.test(await vin.locator('.cr-pins').innerText()), 'the pin that joined, with its designator');
  await shot('nets');

  step('Rails: the headroom shrank, from what to what');
  await tab('rails');
  const rail = rowOf('3V3').filter({ hasText: 'was VOUT' });
  const railChips = await rail.locator('.vchip').allInnerTexts();
  expect(railChips.join() === '70 mA,50 mA', `the headroom before and after (${railChips.join(' -> ')})`);
  expect(/headroom/.test(await rail.locator('.cr-chips').innerText()), 'named as the headroom');
  expect((await rail.locator('.vchip.vt-shrank').count()) === 1, 'the new value is marked as smaller');
  await shot('rails');

  step('BOM, Modules and interfaces, Problems and Source');
  await tab('bom');
  const bom = await page.locator('[data-testid="cmp-list"]').innerText();
  expect(/Added/.test(bom) && /Removed/.test(bom) && /qty 1/.test(bom), 'lines added and removed with their quantity');
  await shot('bom');
  await tab('modules');
  expect((await page.locator('.sec-compare .empty.compact').innerText()).includes('No module or interface changed'), 'no module changed: an empty state with a check');
  await tab('problems');
  expect((await page.locator('.sec-compare .empty.compact').innerText()).includes('No problem is new or resolved'), 'no problem is new or resolved');
  await tab('source');
  const files = await page.locator('[data-testid="cmp-file"]').evaluateAll((els) => els.map((e) => e.innerText.replace(/\s+/g, ' ')));
  expect(files.length === 2 && files.some((f) => /src\/main\.nsm/.test(f) && /Modified/.test(f)) && files.some((f) => /NOTES\.md/.test(f) && /Untracked/.test(f)), `the files that differ (${files.join(' | ')})`);
  expect(/2 files, \+\d+ −\d+ lines/.test(await text('[data-testid="source-total"]')), 'with the totals of git diff --stat');
  await shot('source-tab');
  await page.keyboard.press('ArrowLeft');
  expect((await page.locator('.cmp-tab.active').getAttribute('data-tab')) === 'problems', 'the arrow keys move between the tabs');
  await tab('parts');

  // ------------------------------------------------------------------ click-through
  step('click a row: it is picked in every view; hovering lights it through the shared hover');
  await rowOf('r1').locator('.cr-main').hover();
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => document.documentElement.classList.contains('has-hover')), 'the shared hover is on while the pointer is on the row');
  await rowOf('r1').locator('.cr-main').click();
  await page.waitForSelector('.selchip');
  expect(/r1/.test(await text('.selchip')), 'the selection chip says r1');
  expect((await rowOf('r1').locator('.cr-main').getAttribute('aria-selected')) === 'true', 'the row is the selected one');
  await rowOf('r1').locator('.cr-link[aria-label^="Show in the Schematic"]').click();
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  await page.waitForSelector('.schematic');
  expect(true, 'the schematic is shown');
  await section('compare');
  await page.keyboard.press('Escape');

  step('Head opens the source at the line it is declared on; Base opens the file at the ref beside the file on disk');
  const head = rowOf('stage.input_cap').locator('[data-testid="link-head"]');
  expect(/src\/main\.nsm:\d+/.test((await head.getAttribute('aria-label')) ?? ''), 'the Head link names the file and line');
  expect((await rowOf('stage.input_cap').locator('[data-testid="link-base"]').isDisabled()) === true, 'an added part has no Base');
  expect((await rowOf('debug').locator('[data-testid="link-head"]').isDisabled()) === true, 'a removed part has no Head');
  await rowOf('r1').locator('[data-testid="link-base"]').click();
  await page.waitForSelector('[data-testid="diff-panel"] .cm-editor');
  await page.waitForTimeout(500);
  expect((await text('[data-testid="diff-path"]')) === 'src/main.nsm', 'the diff view is of the file');
  expect(/main/.test(await text('.diff-title')) || /[0-9a-f]{7}/.test(await text('.diff-title')), 'it says the ref and "working file"');
  expect(/working file/.test(await text('.diff-title')) && /read only/.test(await text('.diff-title')), 'the working file against the ref, read only');
  const lines = await page.evaluate(() => ({ add: document.querySelectorAll('.cm-dl-add').length, del: document.querySelectorAll('.cm-dl-del').length, filler: document.querySelectorAll('.cm-dl-filler').length, marks: document.querySelectorAll('.cm-di').length }));
  expect(lines.add > 0 && lines.del > 0, `added and removed lines are coloured (${lines.add} added, ${lines.del} removed)`);
  expect(lines.marks > 0, 'the words that changed are marked inside a line');
  expect(lines.filler > 0, 'a side that has no line is hatched, so the sides line up');
  expect((await page.locator('.cm-editor[data-testid="diff-view"]').count()) === 2, 'two views, side by side');
  const count = await text('[data-testid="diff-count"]');
  expect(/1 of \d+ changes?/.test(count), `the number of changes (${count})`);
  expect((await page.locator('.diff-col .cm-content').first().innerText()).includes('150ohm'), 'the base side has the old line');
  expect((await page.locator('.diff-col .cm-content').nth(1).innerText()).includes('.r1: Resistor {}'), 'the head side has the new line');
  await shot('diff-side');
  await page.click('[data-testid="diff-next"]');
  expect(/2 of/.test(await text('[data-testid="diff-count"]')), 'next change');
  await page.click('[data-testid="diff-mode-inline"]');
  await page.waitForSelector('[data-testid="diff-inline"] .cm-editor');
  expect((await page.locator('.cm-editor[data-testid="diff-view"]').count()) === 1, 'inline: one view');
  const numbers = await page.evaluate(() => ({ a: [...document.querySelectorAll('.diff-panel .cm-dn-a')].filter((e) => /^\d+$/.test(e.textContent)).length, b: [...document.querySelectorAll('.diff-panel .cm-dn-b')].filter((e) => /^\d+$/.test(e.textContent)).length }));
  expect(numbers.a > 10 && numbers.b > 10, `with the line numbers of both sides (${numbers.a} and ${numbers.b})`);
  await shot('diff-inline');
  await page.click('[data-testid="diff-mode-side"]');
  await page.waitForSelector('[data-testid="diff-side"] .cm-editor');
  const sel = await page.evaluate(() => { const c = document.querySelector('.diff-panel .cm-content'); return c?.getAttribute('aria-readonly'); });
  expect(sel === 'true', 'the content is read only');

  step('the unchanged lines are hidden behind slim bars, in both views alike, with 3 lines of context round each change (a file over 60 lines does it by itself)');
  const col = (n) => page.locator('.diff-col').nth(n);
  const barsOf = (n) => col(n).locator('.cm-diff-bar');
  expect((await page.locator('[data-testid="diff-hide"]').getAttribute('aria-pressed')) === 'true', 'the switch "Hide unchanged lines" is on without being asked, for a file of 110 lines');
  const counts = async (n) => (await barsOf(n).evaluateAll((els) => els.map((e) => Number(e.getAttribute('data-count')))));
  const c0 = await counts(0);
  const c1 = await counts(1);
  expect(c0.length >= 3 && JSON.stringify(c0) === JSON.stringify(c1), `both views hide the same runs (${c0.join(', ')} lines)`);
  expect((await barsOf(0).first().evaluate((e) => e.tagName === 'BUTTON' && /^Show \d+ unchanged lines/.test(e.getAttribute('aria-label') ?? ''))), 'a bar is a button that says what it shows');
  expect(/^\d+ unchanged lines$/.test((await barsOf(0).first().innerText()).trim()), `and its words are "N unchanged lines" (${(await barsOf(0).first().innerText()).trim()})`);
  const shownText = await col(0).locator('.cm-content').innerText();
  expect(!shownText.includes('import std::sources::Rail') && shownText.includes('150ohm'), 'a far line is not drawn, the changed one is');
  const hiddenTotal = c0.reduce((a, b) => a + b, 0);
  const rowsDrawn = await col(0).locator('.cm-line').count();
  expect(rowsDrawn + hiddenTotal >= 108 && rowsDrawn < 70, `${rowsDrawn} lines drawn and ${hiddenTotal} hidden`);
  await shot('diff-collapsed');

  step('a click on a bar shows its lines in both views; Enter on another does the same from the keyboard; the choice is kept');
  await barsOf(0).first().click();
  await settle();
  expect(JSON.stringify(await counts(0)) === JSON.stringify(c0.slice(1)) && JSON.stringify(await counts(1)) === JSON.stringify(c0.slice(1)), 'the first run is open in both views');
  expect((await col(0).locator('.cm-content').innerText()).includes('Two boards in one system'), 'its lines are there');
  await barsOf(0).first().focus();
  await page.keyboard.press('Enter');
  await settle();
  expect((await counts(0)).length === c0.length - 2 && (await counts(1)).length === c0.length - 2, 'Enter opened the next one');
  await page.locator('[data-testid="diff-hide"]').click();
  await settle();
  expect((await page.locator('.cm-diff-bar').count()) === 0 && (await page.locator('[data-testid="diff-hide"]').getAttribute('aria-pressed')) === 'false', 'the switch off: every line is drawn');
  await page.locator('[data-testid="diff-hide"]').click();
  await settle();
  expect((await counts(0)).length === c0.length - 2, 'on again: the two bars that were opened stay open');
  await page.click('[data-testid="diff-mode-inline"]');
  await page.waitForSelector('[data-testid="diff-inline"] .cm-editor');
  await settle();
  expect((await page.locator('.diff-inline .cm-diff-bar, [data-testid="diff-inline"] .cm-diff-bar').count()) === c0.length - 2, 'the inline view hides the same runs');
  await page.click('[data-testid="diff-mode-side"]');
  await page.waitForSelector('[data-testid="diff-side"] .cm-editor');

  step('F7 and Shift+F7 step the changes from anywhere in the page, Esc closes, and the palette and the cheat sheet list them');
  const at = async () => Number((await text('[data-testid="diff-count"]')).match(/^(\d+) of/)[1]);
  await col(0).locator('.cm-content').focus();
  const before = await at();
  await page.keyboard.press('F7');
  await settle();
  expect((await at()) === before + 1, `F7 goes to the next change (${before} -> ${await at()})`);
  await page.keyboard.press('Shift+F7');
  await settle();
  expect((await at()) === before, 'Shift+F7 goes back');
  const pressed = () => page.locator('[data-testid="diff-hide"]').getAttribute('aria-pressed');
  const was = await pressed();
  await page.keyboard.press('Control+Alt+h');
  await settle();
  expect((await pressed()) !== was, 'Ctrl+Alt+H turns the hiding of unchanged lines on or off');
  await page.keyboard.press('Control+Alt+h');
  await settle();
  expect((await pressed()) === was, 'and back');
  await col(0).locator('.cm-content').focus();
  await page.keyboard.press('Control+/');
  await page.waitForSelector('.cheat');
  const cheat = await page.locator('.cheat').innerText();
  expect(/(Hide|Show) unchanged lines in the comparison/.test(cheat), 'the cheat sheet lists the switch too');
  expect(/Next change in the comparison/.test(cheat) && /F7/.test(cheat) && /Previous change in the comparison/.test(cheat) && /Shift\s*\+?\s*F7/.test(cheat), 'the cheat sheet lists F7 and Shift+F7');
  expect(/Comparison of a file/.test(cheat) && /close the comparison/.test(cheat) && /on a bar of unchanged lines/.test(cheat), 'and Esc, and Enter or Space on a bar');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.cheat', { state: 'detached' });
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('unchanged lines');
  await settle(300);
  expect(/(Hide|Show) unchanged lines in the comparison/.test(await page.locator('.qp').innerText()), 'the palette has the switch, worded as what it does next');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.qp', { state: 'detached' });
  await settle(300);
  expect(await page.evaluate(() => !!document.activeElement?.closest('.diff-panel')), 'the focus is back in the comparison');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid="diff-panel"]', { state: 'detached' });
  expect(true, 'Escape closes the diff view');

  step('"Show changes in source" opens the same view over the editor of the Source section');
  await tab('source');
  await page.locator('[data-testid="cmp-file"]', { hasText: 'src/main.nsm' }).click();
  await page.waitForSelector('[data-testid="diff-panel"] .cm-editor');
  await page.click('[data-testid="diff-in-source"]');
  await page.waitForSelector('.section.sec-source:not(.away) .diff-panel');
  expect((await page.locator('.sec-source .diff-panel').count()) === 1, 'in the Source section');
  expect((await page.locator('.sec-compare .diff-panel').count()) === 0, 'and no longer in Compare');
  await shot('diff-in-source');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.diff-panel', { state: 'detached' });

  step('the file tab\'s context menu has "Show changes vs main"');
  await page.locator('.tabbar.files .tab[data-path="src/main.nsm"]').click({ button: 'right' });
  await page.waitForSelector('[data-testid="tab-show-changes"]');
  expect(/vs main/.test(await text('[data-testid="tab-show-changes"]')), 'the menu says the ref');
  await page.click('[data-testid="tab-show-changes"]');
  await page.waitForSelector('.sec-source .diff-panel .cm-editor');
  expect(true, 'it opens the comparison of that file');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.diff-panel', { state: 'detached' });

  // ------------------------------------------------------------------ the marks
  step('the switch: the schematic outlines changed parts, lists what was removed and shows a legend');
  await section('compare');
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await page.evaluate(() => {
    window.__swapped = 0;
  });
  await section('schematic');
  await page.waitForSelector('svg.schematic .part');
  await page.evaluate(() => {
    const svg = document.querySelector('.schematic-host svg');
    svg.dataset.keep = 'yes';
    window.__childList = 0;
    new MutationObserver((m) => (window.__childList += m.filter((x) => x.type === 'childList').length)).observe(svg, { childList: true, subtree: true });
  });
  expect((await markedWith('svg.schematic .part[class*="cmp-"]')) === 0, 'no marks while the switch is off');
  await visible('[data-testid="changes-switch"]').click();
  await page.waitForTimeout(500);
  expect(await page.evaluate(() => document.documentElement.classList.contains('cmp-on')), 'the page says marks are on');
  const changedParts = await page.evaluate(() => [...document.querySelectorAll('svg.schematic .part.cmp-changed')].map((e) => e.getAttribute('data-ref')));
  expect(changedParts.length === 2, `the resistor and the LED are outlined as changed (${changedParts.join(', ')})`);
  expect((await markedWith('svg.schematic .part.cmp-renumbered')) === 1, 'the renumbered socket is outlined apart');
  const colours = await page.evaluate(() => {
    const token = (name) => {
      const e = document.createElement('i');
      e.style.color = `var(${name})`;
      document.body.appendChild(e);
      const c = getComputedStyle(e).color;
      e.remove();
      return c;
    };
    // The mark is the rectangle outside the symbol (schematic/halo.ts); the symbol's own strokes stay as drawn.
    const stroke = (sel) => getComputedStyle(document.querySelector(sel).previousElementSibling.previousElementSibling).stroke;
    return { changed: stroke('svg.schematic .part.cmp-changed'), renumbered: stroke('svg.schematic .part.cmp-renumbered'), tokenChanged: token('--cmp-changed'), tokenRenumbered: token('--cmp-renumbered') };
  });
  expect(colours.changed === colours.tokenChanged && colours.renumbered === colours.tokenRenumbered && colours.changed !== colours.renumbered, `the marks are the comparison's colours: changed ${colours.changed}, renumbered ${colours.renumbered}`);
  // The notes (ADR 0060) are not compared: the marks leave their colours and classes alone, and no outline of a mark lies on one.
  const notesNow = await noteFacts(page);
  const outlines = await page.evaluate(() =>
    [...document.querySelectorAll('svg.schematic .part-cmp')].filter((r) => r.getClientRects().length && getComputedStyle(r).stroke !== 'none').map((r) => { const b = r.getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; }),
  );
  expect(notesNow.notes.length >= 1 && outlines.length >= 3, `the board draws ${notesNow.notes.length} notes beside ${outlines.length} outlined parts`);
  expect(notesNow.notes.every((n) => !/cmp-/.test(n.cls) && n.frameFill === notesNow.tokens.fill && n.textFill === notesNow.tokens.text), 'a note keeps its card and its words: no comparison class, no mark colour');
  expect(notesNow.notes.every((n) => outlines.every((o) => !overlaps(n.box, o))), 'and no outline of a changed part lies on a note');
  const kept = await page.evaluate(() => ({ svg: document.querySelector('.schematic-host svg').dataset.keep, childList: window.__childList }));
  expect(kept.svg === 'yes' && kept.childList === 0, `no drawing was replaced, no node added or removed (${kept.childList})`);
  expect(/changed/.test(await text('[data-testid="cmp-legend"]')) && /removed/.test(await text('[data-testid="cmp-legend"]')), 'a legend chip in the toolbar');
  const gone = await text('[data-testid="removed-parts"]');
  expect(/Header/.test(gone), `the removed header is listed in the inspector with its base value (${gone.replace(/\s+/g, ' ')})`);
  expect(/Removed from this board/.test(await text('.sinspector')), 'under "Removed from this board"');
  await shot('schematic-marks');

  step('the bill of materials: a Change column, new lines marked, removed lines struck through at the end');
  await section('bom');
  await page.waitForSelector('.bom-table');
  expect(/Change/.test(await text('.bom-table thead')), 'a Change column');
  expect((await markedWith('.bom-table tr.cmp-row-added')) === 2, 'the two new lines are marked');
  expect(/\+1/.test(await text('.bom-table tr.cmp-row-added td.delta')), 'with +1');
  const goneRows = await page.locator('[data-testid="bom-removed-row"]').evaluateAll((els) => els.map((e) => e.innerText.replace(/\s+/g, ' ')));
  expect(goneRows.length === 3 && goneRows.every((r) => /-1|−1/.test(r)), `three removed lines at the end (${goneRows.join(' | ')})`);
  expect(/Removed since main/.test(await text('[data-testid="bom-removed"]')), 'under their own heading');
  const struck = await page.evaluate(() => getComputedStyle(document.querySelector('[data-testid="bom-removed-row"] td:not(.delta)')).textDecorationLine);
  expect(struck.includes('line-through'), 'struck through');
  await shot('bom-delta');

  step('the power tree: the rail whose headroom shrank has a badge, before to after');
  await section('power');
  await page.waitForSelector('.pt-rail');
  await page.waitForTimeout(400);
  const badge = await page.evaluate(() => [...document.querySelectorAll('.pt-rail.cmp-shrank')].map((e) => e.getAttribute('data-cmp-delta')));
  expect(badge.length === 1 && /70 mA.*50 mA/.test(badge[0]), `the delta badge (${badge.join(', ')})`);
  const badgeText = await page.evaluate(() => getComputedStyle(document.querySelector('.pt-rail.cmp-shrank'), '::after').content);
  expect(/70 mA/.test(badgeText), 'drawn from the attribute, in red');
  expect((await markedWith('.pt-rail.cmp-changed')) >= 2, 'the changed rails of both boards are marked');
  await shot('power-badges');

  step('the system: the boards that changed are marked');
  await section('system');
  await page.waitForSelector('.sy-card');
  await page.waitForTimeout(400);
  expect((await markedWith('.sy-card.cmp-board-changed')) === 2, 'both boards are marked as changed');
  await shot('system-marks');

  step('the source: marks in the margin and "N changes" in the status bar');
  await section('source');
  await page.waitForSelector('.cm-content');
  // (the gutter keeps a hidden copy of its widest mark to size itself: only a mark with a height is a line's)
  const shown = (kind) => page.evaluate((k) => [...document.querySelectorAll(`[data-testid="chg-${k}"]`)].filter((e) => e.offsetHeight > 0).length, kind);
  await page.waitForFunction(() => [...document.querySelectorAll('[data-testid^="chg-"]')].some((e) => e.offsetHeight > 0), null, { timeout: 15000 });
  const bars = { added: await shown('added'), changed: await shown('changed'), deleted: await shown('deleted') };
  expect(bars.added + bars.changed > 0, `a bar beside lines that are new or changed (${JSON.stringify(bars)})`);
  expect(/\d+ lines? changed in this file/.test(await text('[data-testid="source-changes"]')), `the number of lines changed, in this file, in the status bar (${await text('[data-testid="source-changes"]')})`);
  await shot('source-gutter');

  step('Next change and Previous change (Alt+F5, Shift+Alt+F5) step the runs of changed lines of the open file from the cursor, and the overview ruler draws the changes as thin bars');
  const gutterLine = () => page.evaluate(() => Number(/Ln (\d+), Col/.exec(document.body.innerText)?.[1] ?? 0));
  const ticks = await page.evaluate(() => [...document.querySelectorAll('.cm-ruler-tick.lane-change')].map((t) => ({ line: Number(t.getAttribute('data-line')), kind: t.dataset.kind, w: t.getBoundingClientRect().width, h: t.getBoundingClientRect().height })));
  expect(ticks.length >= 2 && ticks.every((t) => t.w <= 6 && t.h >= 2 && t.w < 8), `the ruler has a bar for each run of changes, thin (${ticks.map((t) => `${t.kind}@${t.line}`).join(', ')})`);
  const starts = [...new Set(ticks.map((t) => t.line))].sort((a, b) => a - b);
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  await settle(200);
  await page.keyboard.press('Alt+F5');
  await settle(300);
  const first = await gutterLine();
  expect(first === starts[0], `Alt+F5 goes to the first change (line ${first}, the ruler's first bar is at ${starts[0]})`);
  await page.keyboard.press('Alt+F5');
  await settle(300);
  const second = await gutterLine();
  expect(second > first && (starts.length < 2 || second === starts[1] || second <= starts[starts.length - 1]), `Alt+F5 again goes on to the next one (line ${second})`);
  await page.keyboard.press('Shift+Alt+F5');
  await settle(300);
  expect((await gutterLine()) === first, 'Shift+Alt+F5 goes back to the one before');
  let wrapped = first;
  for (let i = 0; i < starts.length + 2 && !(await page.locator('.toast', { hasText: 'Back at the first change' }).count()); i++) {
    await page.keyboard.press('Alt+F5');
    await settle(200);
    wrapped = await gutterLine();
  }
  expect(wrapped === first && (await page.locator('.toast', { hasText: 'Back at the first change' }).count()) > 0, 'past the last change it comes round to the first, and a toast says so');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('next change');
  await settle(250);
  const listed = await page.locator('.qp').innerText();
  expect(/Next change/.test(listed) && /Alt\s*\+?\s*F5/.test(listed), `the palette lists Next change with its key (${listed.replace(/\s+/g, ' ').slice(0, 120)})`);
  await page.keyboard.press('Escape');
  await page.waitForSelector('.qp', { state: 'detached' });
  await page.keyboard.press('Control+/');
  await page.waitForSelector('.cheat');
  const sheet = await page.locator('.cheat').innerText();
  expect(/Next change\b/.test(sheet) && /Previous change\b/.test(sheet) && /Alt\s*\+?\s*F5/.test(sheet), 'the cheat sheet lists Next change and Previous change');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.cheat', { state: 'detached' });
  await shot('source-ruler-changes');

  step('turn it off with the palette command: every mark goes');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('hide changes');
  await page.waitForTimeout(250);
  await page.keyboard.press('Enter');
  await page.waitForTimeout(500);
  expect(!(await page.evaluate(() => document.documentElement.classList.contains('cmp-on'))), 'the page says marks are off');
  expect((await markedWith('[class*="cmp-changed"], [class*="cmp-added"], [class*="cmp-renumbered"], [data-cmp-delta]')) === 0, 'no class of a mark is left in the page');
  expect((await shown('added')) + (await shown('changed')) + (await shown('deleted')) === 0, 'and none in the margin');
  await section('bom');
  expect(!/Change/.test(await text('.bom-table thead')), 'the Change column is gone');

  step('the chip\'s menu turns them on again');
  await page.click('[data-testid="compare-chip"]');
  await page.waitForSelector('[data-testid="chip-marks"]');
  await shot('chip-menu');
  await page.click('[data-testid="chip-marks"]');
  await page.waitForTimeout(400);
  expect(await page.evaluate(() => document.documentElement.classList.contains('cmp-on')), 'on again');
  await page.click('.section:not(.away) [data-testid="changes-switch"]');
  await page.waitForTimeout(300);

  // ------------------------------------------------------------------ remembered, and a change on disk
  step('the ref is remembered for the project: after a reload the section opens on the comparison');
  await page.reload();
  await page.waitForSelector('.sidebar > *', { state: 'attached' });
  await section('compare');
  expect(/headroom shrank/.test(await summary()), 'the same comparison is there with no choice made');

  step('an error written to disk is a new problem, and the comparison follows the files');
  const file = path.join(project, 'src/main.nsm');
  const original = fs.readFileSync(file, 'utf8');
  fs.writeFileSync(file, original.replace('.led: Led {', '.led: Led { .color &= "red"'));
  await page.waitForSelector('[data-testid="cmp-head-errors"]', { timeout: 30000 });
  await tab('problems');
  await page.waitForSelector('.cr-kind.k-problem');
  expect(/catalog has nothing/.test(await text('[data-testid="cmp-list"]')) && /New/.test(await text('[data-testid="cmp-list"]')), 'the new error is listed');
  expect(/\d+ new/.test(tile_text(await text('[data-testid="tile-problems"]'))), 'and the tile counts it');
  await shot('problems');
  fs.writeFileSync(file, original);
  await page.waitForSelector('[data-testid="cmp-head-errors"]', { state: 'detached', timeout: 30000 });
  expect(true, 'it is gone when the file is right again');
  await tab('parts');

  // ------------------------------------------------------------------ HEAD: only the source differs
  step('HEAD: no change to the circuit, and the source that differs is said');
  await visible('[data-testid="ref-button"]').click();
  await pick('HEAD');
  await page.waitForSelector('[data-testid="cmp-empty"]', { timeout: 60000 });
  expect(/No changes in the circuit/.test(await text('[data-testid="cmp-empty"]')), 'No changes in the circuit');
  expect(/Only the source differs/.test(await text('[data-testid="cmp-empty"]')) && /NOTES\.md/.test(await text('[data-testid="cmp-source-only"]')), 'with what did differ: NOTES.md, which is not part of the circuit');
  expect((await page.locator('[data-testid="cmp-empty"] .empty-mark svg').count()) === 1, 'with a check mark');
  await shot('no-changes');

  // ------------------------------------------------------------------ progress and a ref that does not analyse
  step('progress: the stage and the time, no percentage');
  await page.route('**/api/compare**', async (route) => {
    const response = await route.fetch();
    await new Promise((r) => setTimeout(r, 2500));
    await route.fulfill({ response });
  });
  await visible('[data-testid="ref-button"]').click();
  await pick('broken');
  await page.waitForSelector('[data-testid="cmp-progress"]');
  await page.waitForTimeout(1000);
  const stage = await text('[data-testid="cmp-stage"]');
  expect(stage.trim().length > 3 && stage.trim() !== 'Working', `the stage the server is in (${stage})`);
  expect(/\d+(\.\d)? s/.test(await text('[data-testid="cmp-elapsed"]')), 'the time it has taken');
  expect(!/%/.test(await text('[data-testid="cmp-progress"]')), 'never a percentage');
  expect((await page.locator('[data-testid="compare-chip"]').innerText()).includes('comparing'), 'the chip says it is comparing');
  await shot('progress');
  step('a ref that does not analyse: the sentence and the ref\'s own problems, and a way out');
  await page.waitForSelector('[data-testid="cmp-broken"]', { timeout: 60000 });
  await page.unroute('**/api/compare**');
  expect(/does not analyse/.test(await text('[data-testid="cmp-error"]')), `the sentence (${await text('[data-testid="cmp-error"]')})`);
  expect(/unknown type/.test(await text('.cmp-baseproblems')), 'the error of the ref');
  expect(/cannot compare/.test(await text('[data-testid="compare-chip"]')), 'the chip says it cannot compare');
  await shot('broken-ref');
  await page.click('[data-testid="another-ref"]');
  await page.waitForSelector('[data-testid="ref-popover"]');
  expect(true, 'Compare with another ref opens the picker');
  await page.keyboard.press('Escape');
  // The picker is closed before the button is clicked again: on a slow runner the click came while it was
  // still closing and shut it instead of opening it (CI, ref-custom never appeared).
  await page.waitForSelector('[data-testid="ref-popover"]', { state: 'hidden' });

  step('a name that is no ref: the page says so');
  // The server answers 404 and the browser logs that as an error; it is the request this step is for.
  allowConsole(/Failed to load resource: the server responded with a status of 404/);
  await visible('[data-testid="ref-button"]').click();
  await page.waitForSelector('[data-testid="ref-popover"]');
  await page.keyboard.type('no-such-branch');
  await page.waitForSelector('[data-testid="ref-custom"]');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="cmp-failed"]', { timeout: 30000 });
  expect(/no ref called/.test(await text('[data-testid="cmp-failed"]')), 'there is no ref called that');
  await page.click('[data-testid="another-ref"]');
  await page.keyboard.press('Escape');
  await pick('main');
  await summary();

  // ------------------------------------------------------------------ not a repository
  step('not a repository: a state of its own, with the reason');
  const copy = fs.mkdtempSync(path.join(os.tmpdir(), 'nsm-nogit-'));
  fs.cpSync(project, copy, { recursive: true, filter: (src) => !src.split(path.sep).includes('.git') });
  const port = await freePort();
  const server = spawn(binary, ['editor', '--port', String(port)], { cwd: copy, stdio: 'ignore' });
  try {
    let reason = null;
    for (let i = 0; i < 100 && !reason; i++) {
      try {
        const r = await fetch(`http://127.0.0.1:${port}/api/git`);
        if (r.ok) reason = await r.text();
      } catch {
        await new Promise((r) => setTimeout(r, 150));
      }
    }
    expect(!!reason && JSON.parse(reason).available === false, 'the server over a folder that is no repository says so');
    await page.route('**/api/git', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: reason }));
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await page.waitForSelector('.sidebar > *', { state: 'attached' });
    await section('compare');
    await page.waitForSelector('[data-testid="cmp-no-git"]');
    expect(/not in a git repository|git is not installed/.test(await text('[data-testid="cmp-no-git"]')), 'it says the project is not in a repository');
    expect((await text('[data-testid="cmp-reason"]')).length > 5, `with the reason (${await text('[data-testid="cmp-reason"]')})`);
    expect((await page.locator('[data-testid="ref-button"]').count()) === 0, 'no ref picker');
    await shot('not-a-repo');
    await page.unroute('**/api/git');
  } finally {
    server.kill();
    fs.rmSync(copy, { recursive: true, force: true });
  }
}

function tile_text(t) {
  return t.replace(/\s+/g, ' ');
}
