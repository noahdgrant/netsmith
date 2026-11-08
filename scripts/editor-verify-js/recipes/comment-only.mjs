// A change to a plain `//` comment is not analysed again: typing one and saving it leaves the server's reading (its version) as it
// was, and the status bar never says it is analysing. A change that adds a line, a doc comment or code is analysed as ever.
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'two_boards';

export async function run({ page, step, expect, project, section }) {
  const file = path.join(project, 'src/main.nsm');
  const read = () => page.evaluate(() => fetch('/api/analysis', { cache: 'no-store' }).then((r) => r.json()));
  await section('source');
  await page.waitForSelector('.cm-content');
  await page.waitForSelector('.statusbar .item.idle', { timeout: 60000 });
  const before = (await read()).version;

  step('a note at the end of a line, typed and saved: no analysis');
  await page.locator('.cm-line', { hasText: 'import std::sources::Rail' }).first().click();
  await page.keyboard.press('End');
  await page.keyboard.type(' // the rails come from here');
  await page.waitForTimeout(900);
  expect((await page.locator('.statusbar .analysing').count()) === 0, 'the bar never said it was analysing while typing');
  await page.keyboard.press('Control+s');
  await page.waitForFunction(() => !document.querySelector('.tab .dot'), null, { timeout: 8000 });
  await page.waitForTimeout(1500);
  expect(fs.readFileSync(file, 'utf8').includes('the rails come from here'), 'the comment is on disk');
  expect((await read()).version === before, 'the server did not read the project again');
  expect((await page.locator('.statusbar .analysing').count()) === 0, 'the bar did not say it was analysing');

  step('a doc comment on a new line is analysed');
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('//! a doc line\n');
  await page.keyboard.press('Control+s');
  await page.waitForFunction(() => !document.querySelector('.tab .dot'), null, { timeout: 8000 });
  await page.waitForFunction(async (v) => (await fetch('/api/analysis', { cache: 'no-store' }).then((r) => r.json())).version > v, before, { timeout: 30000 });
  expect(true, 'the server read the project again');
  await page.waitForSelector('.statusbar .item.idle', { timeout: 60000 });
}
