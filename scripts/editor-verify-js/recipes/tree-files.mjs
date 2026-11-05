// The file tree's file operations, on the invented greenhouse project (`launch --fixture tree`):
// make a folder and files (with a starter), see them in the tree, open one, rename it, delete
// it, by the toolbar, the right-click menu and the keys. The files are checked on the disk too.
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'tree';

export async function run({ page, shot, step, expect, section, project }) {
  const row = (id) => page.locator(`.ft-row[data-id="${id}"]`);
  const disk = (p) => path.join(project, p);
  const press = async (key) => {
    await page.keyboard.press(key);
    await page.waitForTimeout(120);
  };
  const analysisProblems = (file) => page.evaluate(async (f) => {
    const a = await (await fetch('/api/analysis')).json();
    return a.problems.filter((p) => p.at?.file === f).map((p) => `${p.severity}: ${p.message}`);
  }, file);
  const focusedId = () => page.evaluate(() => document.activeElement?.dataset?.id ?? null);

  await page.setViewportSize({ width: 1366, height: 768 });
  await section('source');
  await page.waitForSelector('.ft-row[role=treeitem]');
  await page.waitForTimeout(300);

  step('the toolbar has New file and New folder');
  expect((await page.locator('button[aria-label="New file"]').isEnabled()) && (await page.locator('button[aria-label="New folder"]').isEnabled()), 'both buttons are there and enabled');

  step('New folder: a name is typed in the tree; the folder is made, shown (empty) and selected');
  await page.locator('button[aria-label="New folder"]').click();
  await page.waitForSelector('.ft-input');
  expect((await page.evaluate(() => document.activeElement?.className)) === 'ft-input ', 'the name field has the focus');
  await page.keyboard.type('a b');
  expect((await page.locator('.ft-entry-error').innerText()).includes('module name'), 'a name with a space is refused, with the rule');
  await press('Enter');
  expect((await page.locator('.ft-input').count()) === 1, 'Enter does nothing while the name is wrong');
  await page.fill('.ft-input', 'a..b');
  expect((await page.locator('.ft-entry-error').innerText()).includes('..'), 'a `..` is refused');
  await page.fill('.ft-input', '../x');
  expect(((await page.locator('.ft-entry-error').innerText()).length > 0), 'so is a path that goes up');
  await page.fill('.ft-input', 'sheets');
  expect((await page.locator('.ft-entry-error').innerText()).includes('already there'), 'a folder that is there is refused');
  await page.fill('.ft-input', 'a/b');
  expect((await page.locator('.ft-entry-error').innerText()).includes('slash'), 'a slash is refused');
  await page.fill('.ft-input', '');
  expect((await page.locator('.ft-entry-note').count()) === 1, 'an empty name only says what to type');
  await page.fill('.ft-input', 'drafts');
  await shot('new-folder-1366x768');
  await press('Enter');
  await page.waitForSelector('.ft-row[data-id="d:module:drafts"]');
  expect(fs.statSync(disk('src/drafts')).isDirectory(), 'the folder is on the disk');
  expect((await row('d:module:drafts').locator('.ft-count').getAttribute('title')) === 'An empty folder', 'it shows as an empty folder');
  await page.waitForFunction(() => document.activeElement?.dataset?.id === 'd:module:drafts');
  expect((await focusedId()) === 'd:module:drafts', 'and it is the selected row');
  expect((await page.locator('.ft-input').count()) === 0, 'the name field is gone');

  step('Ctrl+N makes a file in the selected folder; a name that looks like a board starts as a board');
  await page.keyboard.press('Control+n');
  await page.waitForSelector('.ft-input');
  const parentLevel = Number(await row('d:module:drafts').getAttribute('aria-level'));
  expect((await page.locator('.ft-entry').getAttribute('aria-level')) === String(parentLevel + 1), 'the entry is one level inside the folder');
  const version = () => page.evaluate(async () => (await (await fetch('/api/analysis')).json()).version);
  const v0 = await version();
  await page.keyboard.type('clock_board');
  const chip = async () => page.locator('.ft-chip[aria-checked="true"]').innerText();
  expect((await chip()) === 'Board', `a board is offered (${await chip()})`);
  expect((await page.locator('.ft-entry-note').innerText()).includes('Makes src/drafts/clock_board.nsm'), 'the note says the whole name, with the ending added');
  await shot('new-file-1366x768');
  await press('Enter');
  await page.waitForSelector('.tab.active:has-text("clock_board.nsm")');
  expect(fs.readFileSync(disk('src/drafts/clock_board.nsm'), 'utf8').includes('board ClockBoard {'), 'the file is made with a board skeleton');
  await page.waitForSelector('.ft-row[data-id="f:src/drafts/clock_board.nsm"].active');
  expect((await row('f:src/drafts/clock_board.nsm').getAttribute('aria-selected')) === 'true', 'it is open and selected in the tree');
  await page.waitForFunction(() => document.querySelector('.cm-content')?.innerText.includes('board ClockBoard'));
  expect(true, 'the source section shows it');
  await page.locator('.ft-row[data-id="f:src/drafts/clock_board.nsm"] .ft-twisty:not(.none)').waitFor();
  await page.locator('.ft-row[data-id="f:src/drafts/clock_board.nsm"] .ft-twisty').click();
  const defs = await page.locator('.ft-row[data-id^="e:src/drafts/clock_board.nsm"]').count();
  expect(defs === 1, `its definition is in the tree (${defs})`);
  const problems = await analysisProblems('src/drafts/clock_board.nsm');
  expect(problems.length === 0, `the check finds nothing wrong in it (${problems.join('; ')})`);
  expect((await version()) > v0, 'the check was made again (its version went up)');

  step('a template is chosen with a click; the others start as they are asked');
  for (const [name, choice, needle] of [['lamp_driver', 'Part', 'part LampDriver {'], ['plain', 'Empty', '//! Plain.'], ['power_stage', null, 'module PowerStage {']]) {
    await row('d:module:drafts').focus();
    await press('Control+n');
    await page.waitForSelector('.ft-input');
    await page.keyboard.type(name);
    if (choice) await page.locator('.ft-chip', { hasText: choice }).click();
    expect((await page.evaluate(() => document.activeElement?.className)) === 'ft-input ', 'the name field keeps the focus after a click on a choice');
    await press('Enter');
    await page.waitForSelector(`.tab.active:has-text("${name}.nsm")`);
    const text = fs.readFileSync(disk(`src/drafts/${name}.nsm`), 'utf8');
    expect(text.includes(needle), `${name}.nsm has ${needle}`);
    const bad = (await analysisProblems(`src/drafts/${name}.nsm`)).filter((p) => p.startsWith('error'));
    expect(bad.length === 0, `${name}.nsm has no error (${bad.join('; ')})`);
  }

  step('a physical file is made beside its source, for the board in it');
  await row('d:module:drafts').focus();
  await press('Control+n');
  await page.keyboard.type('clock_board.physical');
  expect((await page.locator('.ft-chip').count()) === 0, 'no choice of starter for a physical file');
  expect((await page.locator('.ft-entry-note').innerText()).includes('for ClockBoard'), 'the note says which board it is for');
  await press('Enter');
  await page.waitForSelector('.tab.active:has-text("clock_board.physical.nsm")');
  const physical = fs.readFileSync(disk('src/drafts/clock_board.physical.nsm'), 'utf8');
  expect(/physical for ClockBoard \{\n    case \{\n    \}\n\}/.test(physical), 'it holds `physical for ClockBoard { case { } }`');
  expect((await analysisProblems('src/drafts/clock_board.physical.nsm')).filter((p) => p.startsWith('error')).length === 0, 'and the check finds no error in it');
  await row('d:module:drafts').focus();
  await press('Control+n');
  await page.keyboard.type('nothing_here.physical');
  expect((await page.locator('.ft-entry-error').innerText()).includes('nothing_here.nsm'), 'a physical file with no source beside it is refused');
  await press('Escape');
  expect((await page.locator('.ft-input').count()) === 0, 'Escape drops the entry');

  step('a file name that is there is refused, in any case');
  await row('d:module:drafts').focus();
  await press('Control+n');
  await page.keyboard.type('CLOCK_BOARD');
  expect((await page.locator('.ft-entry-error').innerText()).includes('already there'), 'a name that is taken is refused');
  await press('Escape');

  step('the right-click menu of a folder and of a file');
  await row('d:module:drafts').click({ button: 'right' });
  await page.waitForSelector('.ft-menu');
  const labels = await page.locator('.ft-menu [role=menuitem]').allInnerTexts();
  expect(labels.map((l) => l.split('\n')[0]).join('|') === 'New file here|New folder here|Rename|Delete', `the four actions (${labels.join('|').replace(/\n/g, ' ')})`);
  await shot('menu-1366x768');
  await press('ArrowDown');
  await press('Escape');
  expect((await page.locator('.ft-menu').count()) === 0, 'Escape closes it');
  await row('d:module:drafts').click({ button: 'right' });
  await page.locator('.ft-menu [data-action="new-folder"]').click();
  await page.waitForSelector('.ft-input');
  await page.keyboard.type('old');
  await press('Enter');
  await page.waitForSelector('.ft-row[data-id="d:module:drafts/old"]');
  expect(fs.statSync(disk('src/drafts/old')).isDirectory(), 'New folder here made a folder inside it');
  expect((await row('d:module:drafts/old').getAttribute('aria-level')) === String(parentLevel + 1), 'and it is nested in the tree');

  step('Rename from the menu: the file keeps its text, its tab and its place in the order');
  await row('f:src/drafts/plain.nsm').click({ button: 'right' });
  await page.locator('.ft-menu [data-action="rename"]').click();
  await page.waitForSelector('.ft-input');
  const selected = await page.evaluate(() => { const i = document.querySelector('.ft-input'); return i.value.slice(i.selectionStart, i.selectionEnd); });
  expect(selected === 'plain', `the name without its ending is selected (${selected})`);
  await page.keyboard.type('quiet');
  await press('Enter');
  await page.waitForSelector('.ft-row[data-id="f:src/drafts/quiet.nsm"]');
  expect(!fs.existsSync(disk('src/drafts/plain.nsm')) && fs.existsSync(disk('src/drafts/quiet.nsm')), 'renamed on the disk');
  expect((await row('f:src/drafts/plain.nsm').count()) === 0, 'the old row is gone');
  expect((await page.locator('.tab:has-text("quiet.nsm")').count()) === 1 && (await page.locator('.tab:has-text("plain.nsm")').count()) === 0, 'its tab has the new name');

  step('F2 renames from the keyboard; a name with a slash is refused');
  await row('f:src/drafts/quiet.nsm').focus();
  await press('F2');
  await page.waitForSelector('.ft-input');
  await page.keyboard.type('hush');
  await press('Enter');
  await page.waitForSelector('.ft-row[data-id="f:src/drafts/hush.nsm"]');
  expect(fs.existsSync(disk('src/drafts/hush.nsm')), 'F2, a name, Enter');
  await row('f:src/drafts/hush.nsm').focus();
  await press('F2');
  await page.keyboard.type('a/b');
  expect((await page.locator('.ft-entry-error').innerText()).includes('slash'), 'a slash in a rename is refused too');
  await press('Escape');
  expect((await focusedId()) === 'f:src/drafts/hush.nsm', 'Escape gives the focus back to the row');
  await row('f:netsmith.toml').focus();
  await press('F2');
  expect((await page.locator('.ft-input').count()) === 0, 'the manifest is not renamed');
  await row('f:netsmith.toml').click({ button: 'right' });
  const rename = page.locator('.ft-menu [data-action="rename"]');
  const del = page.locator('.ft-menu [data-action="delete"]');
  expect((await rename.getAttribute('aria-disabled')) === 'true' && (await del.getAttribute('aria-disabled')) === 'true', 'Rename and Delete are off for the manifest');
  expect(((await del.getAttribute('title')) ?? '').includes('outside the editor'), 'and the tooltip says why');
  await shot('menu-manifest-1366x768');
  await press('Escape');

  step('Delete asks first; Cancel keeps the file; Delete removes it and closes its tab');
  await row('f:src/drafts/hush.nsm').focus();
  await press('Delete');
  await page.waitForSelector('[role=alertdialog]');
  const dialog = await page.locator('[role=alertdialog]').innerText();
  expect(dialog.includes('Delete src/drafts/hush.nsm?') && dialog.includes('cannot be undone'), 'the question names the file');
  expect((await page.evaluate(() => document.activeElement?.textContent)) === 'Cancel', 'Cancel has the focus, so Enter does not delete');
  await shot('delete-1366x768');
  await press('Escape');
  expect(fs.existsSync(disk('src/drafts/hush.nsm')) && (await page.locator('[role=alertdialog]').count()) === 0, 'Escape keeps it');
  await press('Delete');
  await page.waitForSelector('[role=alertdialog]');
  await page.locator('[role=alertdialog] button', { hasText: 'Delete' }).click();
  await page.waitForSelector('.ft-row[data-id="f:src/drafts/hush.nsm"]', { state: 'detached' });
  expect(!fs.existsSync(disk('src/drafts/hush.nsm')), 'gone from the disk');
  expect((await page.locator('.tab:has-text("hush.nsm")').count()) === 0, 'and its tab is closed');
  expect((await page.evaluate(() => document.activeElement?.closest('[role=tree]') !== null)) === true, 'the focus stays in the tree');

  step('a top-level file needs the box ticked; the manifest cannot be deleted');
  await row('f:src/irrigation.nsm').focus();
  await press('Delete');
  await page.waitForSelector('[role=alertdialog]');
  const strongText = await page.locator('[role=alertdialog]').innerText();
  expect(strongText.includes('top-level file') && strongText.includes('netsmith.toml'), 'it says it is a top-level file');
  const yes = page.locator('[role=alertdialog] button', { hasText: 'Delete' });
  expect(await yes.isDisabled(), 'Delete is off until the box is ticked');
  await page.locator('[role=alertdialog] input[type=checkbox]').check();
  expect(await yes.isEnabled(), 'and on after');
  await page.locator('[role=alertdialog] button', { hasText: 'Cancel' }).click();
  expect(fs.existsSync(disk('src/irrigation.nsm')), 'Cancel kept it');
  await row('f:netsmith.toml').focus();
  await press('Delete');
  expect((await page.locator('[role=alertdialog]').count()) === 0 && fs.existsSync(disk('netsmith.toml')), 'the manifest is not offered for deletion');

  step('a folder with files in it: the question says so, and the folder goes with them');
  await row('d:module:drafts').focus();
  await press('Delete');
  await page.waitForSelector('[role=alertdialog]');
  const folderText = await page.locator('[role=alertdialog]').innerText();
  expect(/It has \d+ files? in it/.test(folderText), `it counts the files (${folderText.split('\n')[1]})`);
  await page.locator('[role=alertdialog] input[type=checkbox]').check();
  await page.locator('[role=alertdialog] button', { hasText: 'Delete' }).click();
  await page.waitForSelector('.ft-row[data-id="d:module:drafts"]', { state: 'detached' });
  expect(!fs.existsSync(disk('src/drafts')), 'the folder and its files are gone from the disk');
  expect((await page.locator('.tab:has-text("clock_board.nsm")').count()) === 0, 'their tabs are closed');
  await page.waitForFunction(() => document.querySelector('.statusbar')?.innerText.length > 0);
  await shot('after-1366x768');

  step('2000 x 1000');
  await page.setViewportSize({ width: 2000, height: 1000 });
  await page.locator('button[aria-label="New folder"]').click();
  await page.keyboard.type('proposed_2');
  await page.waitForTimeout(200);
  await shot('new-folder-2000x1000');
  await press('Enter');
  await page.waitForSelector('.ft-row[data-id="d:module:proposed_2"]');
  await row('d:module:proposed_2').click({ button: 'right' });
  await page.waitForSelector('.ft-menu');
  await shot('menu-2000x1000');
  await press('Escape');
}
