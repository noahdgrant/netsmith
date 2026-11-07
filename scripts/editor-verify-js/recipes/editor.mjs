// The source editor: highlighting, live diagnostics on unsaved text, saving to disk,
// go to definition and a change made on disk by another program.
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, section }) {
  const file = path.join(project, 'src/main.nsm');
  await section('source');
  await page.waitForSelector('.cm-content');

  step('the source is highlighted');
  expect((await page.locator('.cm-content span').count()) > 20, 'tokens in spans');

  step('type a mistake into the LED board: an undeclared type');
  await page.locator('.ft-def', { hasText: 'LedBoard' }).click();
  await page.locator('.cm-line', { hasText: '.r1: Resistor {}' }).click();
  await page.keyboard.press('Home');
  await page.keyboard.type('.oops: Missing\n    ');
  await page.waitForSelector('.cm-lintRange-error', { timeout: 10000 });
  expect(true, 'a squiggle under the mistake, before anything was saved');
  await page.waitForFunction(() => document.querySelector('.statusbar .counts .e')?.textContent.includes('1'));
  expect(true, 'the status bar counts one error');
  expect(!fs.readFileSync(file, 'utf8').includes('oops'), 'nothing written to disk by checking');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('.dock .problem.error');
  const problem = await page.locator('.problem.error').first().innerText();
  expect(problem.includes('Missing'), `the real compiler message in the problems list (${problem.replace(/\s+/g, ' ')})`);
  await shot('mistake');

  step('click the problem: the cursor goes to it');
  await page.keyboard.press('Control+Home');
  await page.locator('.problem.error').first().click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('oops'));
  expect(true, 'the cursor on the line with the mistake');

  step('delete the mistake: the squiggle goes');
  for (let i = 0; i < 20; i++) await page.keyboard.press('Control+z');
  await page.waitForFunction(() => !document.querySelector('.cm-lintRange-error'), null, { timeout: 10000 });
  expect(true, 'no squiggle');

  step('add a comment and save with Ctrl+S');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('// saved by the verify recipe\n');
  await page.waitForSelector('.tab .dot');
  expect(true, 'a dot on the tab for the unsaved text');
  await page.keyboard.press('Control+s');
  await page.waitForFunction(() => !document.querySelector('.tab .dot'));
  expect(fs.readFileSync(file, 'utf8').startsWith('// saved by the verify recipe'), 'the comment is in the file on disk');
  await shot('saved');

  step('Ctrl+click a type: go to its definition in the library');
  await page.locator('.ft-def', { hasText: 'LedBoard' }).click();
  const resistor = page.locator('.cm-line', { hasText: '.r1: Resistor' }).locator('span', { hasText: 'Resistor' }).first();
  await resistor.click({ modifiers: ['Control'] });
  await page.waitForSelector('.tab.active:has-text("passive.nsm")');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('generic Resistor'));
  expect(true, 'the editor at `generic Resistor` in std/parts/passive.nsm');
  await shot('definition');

  step('another program changes the file: the page reads it again');
  await page.locator('.tab', { hasText: 'main.nsm' }).click();
  fs.writeFileSync(file, `// changed on disk\n${fs.readFileSync(file, 'utf8')}`);
  await page.waitForFunction(() => document.querySelector('.cm-content')?.textContent.startsWith('// changed on disk'), null, { timeout: 10000 });
  expect(true, 'the new text in the editor');

  step('a mistake saved to disk shows in the page and keeps the last good drawing');
  fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('.r1: Resistor {}', '.r1: Nothing {}'));
  await page.waitForFunction(() => document.querySelector('.statusbar .counts .e')?.textContent.includes('1'), null, { timeout: 10000 });
  await section('schematic');
  await page.waitForSelector('.section:not(.away) .stale-strip');
  const strip = await page.locator('.section:not(.away) .stale-strip').innerText();
  expect(strip.includes('Showing last good version') && strip.includes('Nothing'), `a strip on the schematic names the error (${strip.replace(/\s+/g, ' ')})`);
  await shot('mistake-on-disk');
  await section('source');
  if (!(await page.locator('.dock .problem').count())) await page.locator('.statusbar .counts').click();
  await page.locator('.problem.error').first().click();
  await page.waitForSelector('.cm-lintRange-error');
  fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('.r1: Nothing {}', '.r1: Resistor {}'));
  await page.waitForFunction(() => document.querySelector('.statusbar .counts .e')?.textContent.includes('0'), null, { timeout: 10000 });
  expect(true, 'the mistake cleared once fixed on disk');
}
