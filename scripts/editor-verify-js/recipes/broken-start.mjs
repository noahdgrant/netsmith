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
    const strip = await page.locator('.section:not(.away) .stale-strip').innerText();
    expect(strip.includes('Showing last good version') && strip.includes('nonsense'), `the schematic says it shows the last good drawing and names the error (${strip.replace(/\s+/g, ' ')})`);
    await page.locator('.section:not(.away) .stale-strip button', { hasText: 'Go to error' }).click();
    await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('nonsense'));
    expect(true, 'Go to error puts the cursor on the mistake');
    if (!(await page.locator('.dock .problem').count())) await page.keyboard.press('Control+j');
    const problem = await page.locator('.problem.error').first().innerText();
    expect(problem.includes('nonsense'), `the compiler's message in the problems list (${problem.replace(/\s+/g, ' ')})`);
    await shot('broken');
    step('fix the file: the views come back without a reload');
    fs.writeFileSync(file, good);
    await section('schematic');
    await page.waitForSelector('.schematic');
    expect(true, 'the schematic is drawn again');
  } finally {
    fs.writeFileSync(file, good);
  }
}
