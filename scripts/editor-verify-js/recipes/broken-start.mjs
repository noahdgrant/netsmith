// A page opened while the source does not build: it says why, with the real message, shows
// the last drawing that built, and recovers by itself when the file is fixed on disk.
// (A server started on a file that never built has no drawing: see the Rust test
// `a_project_that_never_built_has_problems_and_no_boards`.)
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, section }) {
  const file = path.join(project, 'src/main.nsm');
  const good = fs.readFileSync(file, 'utf8');
  try {
    step('break the file on disk and open the page afresh');
    fs.writeFileSync(file, good.replace('.led: Led {', '.led: Led {\n        .nonsense &= 1V'));
    await page.reload();
    await page.waitForSelector('.app');
    await page.waitForFunction(() => document.querySelector('.statusbar .counts .e')?.textContent.includes('1'));
    expect(true, 'the status bar counts the error');
    // The server built the file before it broke, so it still has that drawing, marked stale.
    await section('schematic');
    expect((await page.locator('.schematic-view .banner').innerText()).includes('mistake'), 'the schematic says the source has a mistake and shows the last good drawing');
    await section('source');
    await page.locator('.problems-head').click();
    const problem = await page.locator('.problem.error').first().innerText();
    expect(problem.includes('nonsense'), `the compiler's message in the problems list (${problem.replace(/\s+/g, ' ')})`);
    await shot('broken');
    step('fix the file: the views come back without a reload');
    fs.writeFileSync(file, good);
    await section('schematic');
    await page.waitForSelector('.schematic', { timeout: 10000 });
    expect(true, 'the schematic is drawn again');
  } finally {
    fs.writeFileSync(file, good);
  }
}
