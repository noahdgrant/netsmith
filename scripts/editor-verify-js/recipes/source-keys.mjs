// The source editor's keys and settings, one step for each: Ctrl+/ comments the lines (the shortcuts stay on `?`),
// and autosave writes a file after a pause.
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, section }) {
  const file = path.join(project, 'src/main.nsm');
  const openOptions = async () => {
    await page.locator('.source-opts-item').click();
    await page.waitForSelector('[data-testid="opt-autosave"]');
  };
  await section('source');
  await page.waitForSelector('.cm-content');

  step('Ctrl+/ comments the line and again uncomments it, and opens no cheat sheet');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  // A line of our own: the file may begin with a comment that an earlier recipe left.
  await page.keyboard.type('probe_line\n');
  await page.keyboard.press('Control+Home');
  const first = await page.locator('.cm-line').first().innerText();
  expect(first === 'probe_line', 'a plain first line to start from');
  await page.keyboard.press('Control+/');
  await page.waitForFunction((was) => document.querySelector('.cm-line').innerText !== was, first);
  expect((await page.locator('.cm-line').first().innerText()).startsWith('//'), 'the first line is a comment');
  expect((await page.locator('[aria-label="Keyboard shortcuts"][role="dialog"]').count()) === 0, 'the shortcuts are not open');
  await page.keyboard.press('Control+/');
  await page.waitForFunction((was) => document.querySelector('.cm-line').innerText === was, first);
  expect(true, 'the line is back as it was');
  // `?` is a plain key: it opens the shortcuts from outside the editor, and types a ? inside it.
  await page.evaluate(() => document.activeElement?.blur());
  await page.keyboard.press('Shift+?');
  await page.waitForSelector('[role="dialog"][aria-label="Keyboard shortcuts"]');
  expect(true, '? still opens the shortcuts');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[role="dialog"][aria-label="Keyboard shortcuts"]', { state: 'detached' });
  await page.locator('.cm-content').click();
  // F1 opens them from inside the editor, where Ctrl+/ used to.
  await page.keyboard.press('F1');
  await page.waitForSelector('[role="dialog"][aria-label="Keyboard shortcuts"]');
  expect(true, 'F1 opens the shortcuts from the editor');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[role="dialog"][aria-label="Keyboard shortcuts"]', { state: 'detached' });
  await page.locator('.cm-content').click();
  for (let i = 0; i < 4; i++) await page.keyboard.press('Control+z');
  await page.waitForFunction(() => document.querySelector('.cm-line').innerText !== 'probe_line');

  step('a doc comment is code: it is commented out with a // in front, and only that comes off');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('/// probe doc\n');
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Control+/');
  await page.waitForFunction(() => document.querySelector('.cm-line').innerText === '// /// probe doc');
  await page.keyboard.press('Control+/');
  await page.waitForFunction(() => document.querySelector('.cm-line').innerText === '/// probe doc');
  expect(true, 'a doc line is commented and uncommented without losing a character');
  for (let i = 0; i < 6; i++) await page.keyboard.press('Control+z');

  step('autosave: off until chosen, then a file is written after a pause with no Ctrl+S');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('// typed once\n');
  await page.waitForSelector('.tab .dot');
  await page.waitForTimeout(2200);
  expect(!fs.readFileSync(file, 'utf8').includes('typed once'), 'nothing written while autosave is off');
  await openOptions();
  expect((await page.locator('[data-testid="opt-autosave"]').getAttribute('aria-checked')) === 'false', 'the menu shows it off');
  await page.locator('[data-testid="opt-autosave"]').click();
  await page.waitForFunction(() => !document.querySelector('.tab .dot'), null, { timeout: 8000 });
  expect(fs.readFileSync(file, 'utf8').includes('typed once'), 'the waiting edit was written when autosave went on');
  await page.keyboard.press('Escape');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('// typed twice\n');
  await page.waitForFunction(() => !document.querySelector('.tab .dot'), null, { timeout: 8000 });
  expect(fs.readFileSync(file, 'utf8').includes('typed twice'), 'the next edit was written by itself');
  await shot('autosaved');
  await openOptions();
  await page.locator('[data-testid="opt-autosave"]').click();
  await page.keyboard.press('Escape');
}
