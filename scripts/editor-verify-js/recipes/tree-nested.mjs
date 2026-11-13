// The file tree of an invented greenhouse project (`launch --fixture tree`): folders nested in
// folders with guide lines, folder and file icons, the keyboard, the filter, collapse and expand
// all, what is remembered, the width, and a click on a definition (the cursor goes to its name,
// and the editor does not mark every letter of the keyword).
import { analysedSince, pageVersion, waitFor, whenIdle } from '../settle.mjs';
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'tree';

export async function run({ page, shot, step, expect, section, project }) {
  const row = (id) => page.locator(`.ft-row[data-id="${id}"]`);
  const focused = () => page.evaluate(() => document.activeElement?.dataset?.id ?? null);
  const press = async (key) => {
    await page.keyboard.press(key);
    await whenIdle(page);
  };
  // What a key does shows when the page has handled it, not after a fixed pause: wait for the state to be the one wanted.
  // `want` is a value or a test of the value.
  const until = async (read, want, label) => {
    const ok = (got) => (typeof want === 'function' ? want(got) : got === want);
    let got;
    await waitFor(label, async () => ok((got = await read())));
    expect(ok(got), `${label} (got ${got})`);
  };
  const expanded = (id) => () => row(id).getAttribute('aria-expanded');
  const open = async () => {
    await section('source');
    await page.waitForSelector('.ft-row[role=treeitem]');
  };

  await page.setViewportSize({ width: 1366, height: 768 });
  await open();
  await whenIdle(page);
  await shot('1366x768');

  step('sections, nested folders and guide lines');
  const heads = await page.locator('.ft-section .ft-label').allInnerTexts();
  expect(heads.join('|') === 'Top level|Imported files|Physical files|Project|Standard library', `the five sections, got ${heads.join('|')}`);
  const level = async (id) => Number(await row(id).getAttribute('aria-level'));
  expect((await level('d:module:proposed')) === 2, 'proposed is directly in its section (level 2)');
  expect((await level('d:module:proposed/superseded')) === 3, 'superseded is inside proposed (level 3)');
  expect((await level('f:src/proposed/superseded/relay_a.nsm')) === 4, 'a file of superseded is at level 4');
  const labels = await page.locator('.ft-folder .ft-label').allInnerTexts();
  expect(!labels.some((t) => t.replace(/\s/g, '') === 'proposed/superseded'), 'no flat `proposed/superseded` row');
  expect(labels.some((t) => t.replace(/\s/g, '') === 'legacy/vendor/archive'), 'a chain of single folders is one row with every folder in it');
  const guides = await row('f:src/proposed/superseded/relay_a.nsm').locator('.ft-guide').count();
  expect(guides === 2, `two guide lines on a file three folders deep, got ${guides}`);
  const guideBox = await row('f:src/proposed/superseded/relay_a.nsm').locator('.ft-guide').first().boundingBox();
  expect(guideBox.width >= 1 && guideBox.height >= 24, 'a guide line spans the whole row');

  step('folders sort before files, each by name');
  const order = await page.evaluate(() => {
    const start = [...document.querySelectorAll('.ft-row')].findIndex((r) => r.dataset.id === 'd:module:proposed');
    return [...document.querySelectorAll('.ft-row')].slice(start, start + 9).map((r) => r.dataset.id.split('/').pop().replace(/^[a-z]:(module:)?/, ''));
  });
  expect(order.join() === 'proposed,superseded,relay_a.nsm,relay_b_with_snubber.nsm,relay_c_latching.nsm,alarm_relay.nsm,door_contact.nsm,mains_sense.nsm,smoke_input.nsm', `proposed: its folder first, then its files by name (${order.join()})`);

  step('icons and chevrons');
  expect((await row('d:module:proposed').locator('.ft-icon.folder').count()) === 1, 'a folder icon on a folder');
  expect((await row('f:src/main.nsm').locator('.ft-icon.folder').count()) === 0, 'none on a file');
  expect((await row('f:src/main.nsm').locator('.ft-icon.nsm').count()) === 1, 'the netsmith icon on an .nsm file');
  expect((await row('f:src/greenhouse.physical.nsm').locator('.ft-icon.physical').count()) === 1, 'its own icon on a .physical.nsm file');
  expect((await row('f:netsmith.toml').locator('.ft-icon.toml').count()) === 1, 'a gear on netsmith.toml');
  expect((await row('d:module:proposed').getAttribute('aria-expanded')) === 'true', 'an open folder says so');
  expect((await row('d:module:parts').getAttribute('aria-expanded')) === 'false', 'the library of parts is closed');
  expect((await row('s:std').getAttribute('aria-expanded')) === 'false', 'the standard library is closed');
  expect((await row('d:module:proposed').locator('.ft-chevron.open').count()) === 1, 'the chevron of an open folder points down');
  expect((await row('d:module:parts').locator('.ft-chevron.open').count()) === 0, 'and of a closed one to the right');
  const fw = await page.evaluate(() => [...document.querySelectorAll('.ft-folder .ft-label, .ft-file:not(.active) .ft-label')].map((e) => [e.closest('.ft-row').className.includes('ft-folder'), Number(getComputedStyle(e).fontWeight)]));
  expect(Math.min(...fw.filter((f) => f[0]).map((f) => f[1])) > Math.max(...fw.filter((f) => !f[0]).map((f) => f[1])), `folders are bolder than files (${[...new Set(fw.map((f) => f.join(':')))]})`);
  const rowBox = await row('d:module:proposed').boundingBox();
  const fontPx = await row('d:module:proposed').evaluate((e) => parseFloat(getComputedStyle(e).fontSize));
  expect(rowBox.height >= 24 && fontPx >= 13, `rows are ${rowBox.height} px tall with ${fontPx} px text`);

  step('badges explain themselves');
  expect((await row('d:module:parts').locator('.ft-count').getAttribute('title')) === '17 files, all of them parts: a library', 'the count of a library says what it counts');
  expect((await row('d:module:proposed').locator('.ft-count').getAttribute('title')) === '7 files', 'the count of a folder is a pill with a tooltip');
  const lock = await row('f:netsmith.toml').locator('.ft-lock').getAttribute('title');
  expect(lock === 'Read-only: edit it outside the editor', `the lock on the manifest says why (${lock})`);
  const kind = await page.locator('.ft-def', { hasText: 'Greenhouse' }).getAttribute('title');
  expect(/^Board Greenhouse, line 12: a printed circuit board/.test(kind), `a definition row has a kind tooltip (${kind})`);
  expect((await page.locator('.ft-def .ft-icon.kind-board').count()) >= 2 && (await page.locator('.ft-def .ft-icon.kind-system').count()) === 1, 'board and system definitions have their own icons');
  expect((await page.locator('.ft-def .badge').count()) === 0, 'no lone-letter badges');

  step('the active file has a bar in the selection colour; an open file is marked; unsaved changes show a dot');
  await row('f:src/greenhouse.nsm').click();
  await page.waitForSelector('.tab.active:has-text("greenhouse.nsm")');
  expect((await row('f:src/greenhouse.nsm').getAttribute('aria-selected')) === 'true', 'the open file is selected');
  const bar = await row('f:src/greenhouse.nsm').evaluate((e) => { const s = getComputedStyle(e, '::before'); return [s.width, s.backgroundColor]; });
  // The selection colour is a token and the theme sets it: ask the page what it is.
  const accent = await page.evaluate(() => {
    const probe = document.createElement('i');
    probe.style.color = 'var(--sel-bar)';
    document.body.append(probe);
    const colour = getComputedStyle(probe).color;
    probe.remove();
    return colour;
  });
  expect(bar[0] === '2px' && bar[1] === accent, `a 2 px bar in the selection colour (${bar}, selection ${accent})`);
  await row('f:src/irrigation.nsm').click();
  await page.waitForSelector('.tab.active:has-text("irrigation.nsm")');
  expect((await row('f:src/greenhouse.nsm').evaluate((e) => e.classList.contains('opened'))) === true, 'a file that is open but not active is marked');
  await page.locator('.cm-content').click();
  await page.keyboard.type('// x');
  await page.waitForSelector('.ft-row.active .ft-dot');
  expect((await row('f:src/irrigation.nsm').locator('.ft-dot').getAttribute('title')) === 'unsaved changes', 'the dot says "unsaved changes"');
  await shot('unsaved-1366x768');
  await page.keyboard.press('Control+s');
  await page.waitForSelector('.ft-row.active .ft-dot', { state: 'detached' });
  expect(true, 'it goes when the file is saved');
  await shot('active-1366x768');

  step('keyboard: Up, Down, Home, End, Left, Right, Enter');
  await row('s:top_level').focus();
  await press('ArrowDown');
  await until(focused, 'f:src/greenhouse.nsm', 'Down moves to the next row');
  await press('ArrowDown');
  await until(focused, (id) => !!id && id.startsWith('e:src/greenhouse.nsm:'), 'and into the definitions');
  await press('ArrowUp');
  await until(focused, 'f:src/greenhouse.nsm', 'Up moves back');
  await press('ArrowLeft');
  await until(expanded('f:src/greenhouse.nsm'), 'false', 'Left closes an open file');
  await press('ArrowLeft');
  await until(focused, 's:top_level', 'Left on a closed one goes to the parent');
  await press('ArrowLeft');
  await until(expanded('s:top_level'), 'false', 'Left closes the section');
  await press('ArrowRight');
  await until(expanded('s:top_level'), 'true', 'Right opens it');
  await press('ArrowRight');
  await until(focused, 'f:src/greenhouse.nsm', 'Right on an open one goes to its first child');
  await press('End');
  await until(focused, 's:std', 'End goes to the last row');
  await press('Home');
  await until(focused, 's:top_level', 'Home goes to the first');
  await row('f:src/main.nsm').focus();
  await press('Enter');
  await page.waitForSelector('.tab.active:has-text("main.nsm")');
  expect(true, 'Enter opens a file');
  await row('d:module:proposed').focus();
  await press('Enter');
  await until(expanded('d:module:proposed'), 'false', 'Enter closes a folder');
  await press('Enter');
  await until(expanded('d:module:proposed'), 'true', 'and opens it again');
  expect((await page.locator('.ft-tree[role=tree]').count()) === 1 && (await page.locator('.ft-tree [role=treeitem]').count()) > 20, 'a tree with tree items (the outline below it is a tree of its own)');

  step('a section the person closed stays closed when the project is read again');
  // main.nsm is the active file, in the section Top level: reading the project again (a file changed on disk) must not open it.
  await row('s:top_level').focus();
  await press('ArrowLeft');
  await until(expanded('s:top_level'), 'false', 'Left closes the section that holds the active file');
  const main = path.join(project, 'src/main.nsm');
  const seen = await pageVersion(page);
  fs.appendFileSync(main, '\n// the project is read again\n');
  await analysedSince(page, seen);
  expect((await row('s:top_level').getAttribute('aria-expanded')) === 'false', 'the section is still closed after the project was read again');
  await press('ArrowRight');
  await until(expanded('s:top_level'), 'true', 'and Right opens it');

  step('collapse all and expand all');
  const count = () => page.locator('.ft-row').count();
  const before = await count();
  await page.locator('button[aria-label="Collapse all"]').click();
  const closed = await count();
  expect(closed < before && (await row('d:module:proposed').getAttribute('aria-expanded')) === 'false', `collapse all: ${before} rows became ${closed}`);
  await shot('collapsed-1366x768');
  await page.locator('button[aria-label="Expand all"]').click();
  const opened = await count();
  expect(opened > before && (await row('d:module:parts').getAttribute('aria-expanded')) === 'true', `expand all: ${opened} rows`);
  await page.locator('button[aria-label="Collapse all"]').click();

  step('the scroll stays where it is when a file or folder opens or closes, a file is opened, or the filter is used');
  await page.locator('button[aria-label="Expand all"]').click();
  const treeBox = page.locator('.ft-tree');
  const scrollTop = () => treeBox.evaluate((e) => e.scrollTop);
  await treeBox.evaluate((e) => { e.scrollTop = 400; });
  await whenIdle(page);
  const at = await scrollTop();
  expect(at === 400, `the list is scrolled 400 px down (${at})`);
  // A file with definitions, near the bottom of what is on screen.
  const visibleFiles = () => treeBox.evaluate((e) => {
    const box = e.getBoundingClientRect();
    return [...e.querySelectorAll('.ft-file')].filter((r) => { const b = r.getBoundingClientRect(); return b.top > box.top + 40 && b.bottom < box.bottom && r.querySelector('.ft-twisty:not(.none)'); }).map((r) => r.dataset.id);
  });
  const lowFile = (await visibleFiles()).at(-1);
  expect(!!lowFile, `a file with definitions is in view (${lowFile})`);
  await row(lowFile).locator('.ft-twisty').click();
  await whenIdle(page);
  expect(Math.abs((await scrollTop()) - at) <= 1, `closing a file near the bottom keeps the scroll (${await scrollTop()})`);
  await row(lowFile).locator('.ft-twisty').click();
  await whenIdle(page);
  expect(Math.abs((await scrollTop()) - at) <= 1, `and opening it again (${await scrollTop()})`);
  const folderId = await treeBox.evaluate((e) => { const box = e.getBoundingClientRect(); const r = [...e.querySelectorAll('.ft-folder')].find((x) => { const b = x.getBoundingClientRect(); return b.top > box.top + 40 && b.bottom < box.bottom - 60; }); return r?.dataset.id; });
  await row(folderId).click();
  await whenIdle(page);
  expect(Math.abs((await scrollTop()) - at) <= 1, `closing a folder (${folderId}) keeps the scroll (${await scrollTop()})`);
  await row(folderId).click();
  await whenIdle(page);
  expect(Math.abs((await scrollTop()) - at) <= 1, `and opening it (${await scrollTop()})`);
  // A click on a file below the top: the active file changes, and the list stays.
  const another = (await visibleFiles()).at(0);
  await row(another).click();
  await page.waitForSelector('.ft-row.active');
  await whenIdle(page);
  expect(Math.abs((await scrollTop()) - at) <= 1, `opening another file keeps the scroll (${await scrollTop()})`);
  // The filter: typing leaves the place, and clearing it goes back to it.
  await page.fill('.tree-filter', 'adc');
  await page.fill('.tree-filter', '');
  await whenIdle(page);
  expect(Math.abs((await scrollTop()) - at) <= 1, `a filter typed and cleared (${await scrollTop()})`);
  // The tree is drawn again, as when the files are read again: the same.
  await page.locator('.ft-tree').evaluate((e) => e.dispatchEvent(new Event('scroll')));
  await page.keyboard.press('Control+s');
  await whenIdle(page);
  expect(Math.abs((await scrollTop()) - at) <= 1, `a draw that changes nothing (${await scrollTop()})`);
  await shot('scrolled-1366x768');
  await page.locator('button[aria-label="Collapse all"]').click();

  step('the filter keeps the tree: the folders above a match stay, open, with the match marked');
  await page.fill('.tree-filter', 'relay_b');
  const ids = await page.locator('.ft-row').evaluateAll((rs) => rs.map((r) => r.dataset.id));
  expect(ids.join() === 's:module,d:module:proposed,d:module:proposed/superseded,f:src/proposed/superseded/relay_b_with_snubber.nsm', `ancestors and the match, got ${ids.join()}`);
  expect((await page.locator('.ft-row mark').first().innerText()) === 'relay_b', 'the match is marked');
  await shot('filter-1366x768');
  await page.fill('.tree-filter', 'zzz');
  expect((await page.locator('.ft-empty').count()) === 1, 'an empty result says so');
  await page.fill('.tree-filter', '');

  step('what is open is remembered per project');
  await row('d:module:parts').click();
  expect((await row('d:module:parts').getAttribute('aria-expanded')) === 'true', 'the library is opened by a click');
  await page.reload();
  await open();
  expect((await row('d:module:parts').getAttribute('aria-expanded')) === 'true', 'still open after a reload');
  expect((await row('d:module:proposed').getAttribute('aria-expanded')) === 'false', 'and what was closed is still closed');

  step('the column can be dragged and the width is remembered');
  const width = async () => Math.round((await page.locator('.sidebar').boundingBox()).width);
  expect((await width()) === 260, `it starts at 260 px (${await width()})`);
  const handle = await page.locator('.ft-resizer').boundingBox();
  await page.mouse.move(handle.x + 4, handle.y + 200);
  await page.mouse.down();
  await page.mouse.move(handle.x + 124, handle.y + 200, { steps: 6 });
  await page.mouse.up();
  expect((await width()) === 380, `dragged to 380 px (${await width()})`);
  await page.reload();
  await open();
  expect((await width()) === 380, 'and kept after a reload');
  await page.locator('.ft-resizer').dblclick();
  expect((await width()) === 260, 'a double click puts it back');

  step('a click on a definition puts the cursor on its name and marks no letters');
  await row('f:src/greenhouse.nsm').locator('.ft-twisty').click();
  expect((await row('f:src/greenhouse.nsm').getAttribute('aria-expanded')) === 'true', 'the chevron of a file shows its definitions');
  await page.locator('.ft-def', { hasText: 'SensorHead' }).first().click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('board SensorHead'));
  const matches = await page.locator('.cm-selectionMatch').count();
  expect(matches < 5, `fewer than 5 selection-match highlights (${matches})`);
  const status = await page.locator('.statusbar').innerText();
  expect(/Ln 17, Col 7\b/.test(status), `the cursor is on the name SensorHead (${status.match(/Ln \d+, Col \d+/)})`);
  const selected = await page.evaluate(() => window.getSelection()?.toString() ?? '');
  expect(selected.length < 2, `nothing is selected (${JSON.stringify(selected)})`);
  expect((await page.locator('.cm-reveal-a, .cm-reveal-b').count()) === 1, 'the line it went to flashes');
  await shot('definition-click-1366x768');

  step('2000 x 1000');
  await page.setViewportSize({ width: 2000, height: 1000 });
  await page.locator('button[aria-label="Expand all"]').click();
  await whenIdle(page);
  await shot('2000x1000');
}
