// The problems dock under every view: Ctrl+J, a calm empty state, rows for a mistake on disk,
// F8 from a view to the mistake in the source, the strip above a stale drawing, a height that
// is kept, and the status bar's counts.
import fs from 'node:fs';
import path from 'node:path';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, section }) {
  const file = path.join(project, 'src/main.nsm');
  const good = fs.readFileSync(file, 'utf8');
  try {
    await section('schematic');
    await page.waitForSelector('.schematic');

    step('Ctrl+J opens the dock under the schematic: nothing is wrong');
    await page.keyboard.press('Control+j');
    await page.waitForSelector('.dock');
    expect((await page.locator('.dock-empty').innerText()).includes('No problems'), 'a calm "No problems"');
    expect((await page.locator('.dock').boundingBox()).width > 600, 'the dock spans the window');
    await shot('dock-empty');

    step('a mistake saved to disk: counts, a row, and the strip above the dimmed drawing');
    fs.writeFileSync(file, good.replace('.led: Led {', '.led: Led {\n        .nonsense &= 1V'));
    await page.waitForSelector('.dock .problem.error', { timeout: 15000 });
    await page.waitForSelector('.section:not(.away) .stale-strip');
    const strip = await page.locator('.section:not(.away) .stale-strip').innerText();
    expect(strip.includes('Showing last good version') && strip.includes('nonsense'), `the strip names the error (${strip.replace(/\s+/g, ' ')})`);
    const row = await page.locator('.problem.error').first().innerText();
    expect(row.includes('nonsense') && row.includes('Checker') && /main\.nsm:\d+/.test(row), `a row with the message, its stage and file:line (${row.replace(/\s+/g, ' ')})`);
    expect((await page.locator('.statusbar .counts').innerText()).includes('1'), 'the status bar counts it');
    expect((await page.locator('.statusbar').innerText()).includes('Last good version'), 'the status bar says the drawing is the last good one');
    await shot('stale');

    step('F8 from the schematic goes to the mistake in the source');
    await page.keyboard.press('F8');
    await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('nonsense'), null, { timeout: 10000 });
    expect(true, 'the cursor is on the line with the mistake');
    expect((await page.locator('.problem.active').count()) === 1, 'the row is marked');
    await shot('f8');

    step('the dock keeps its height across a reload, and Ctrl+J closes it');
    const box = await page.locator('.dock-resize').boundingBox();
    await page.mouse.move(box.x + 200, box.y + 3);
    await page.mouse.down();
    await page.mouse.move(box.x + 200, box.y - 80, { steps: 5 });
    await page.mouse.up();
    const h = (await page.locator('.dock').boundingBox()).height;
    await page.reload();
    await page.waitForSelector('.dock');
    expect(Math.abs((await page.locator('.dock').boundingBox()).height - h) < 2, `the same height after a reload (${h})`);
    await page.locator('.statusbar .conn').waitFor();
    await page.keyboard.press('Control+j');
    await page.waitForSelector('.dock', { state: 'detached' });
    expect(true, 'closed');

    step('fixed on disk: the strip goes, the dock says nothing is wrong');
    fs.writeFileSync(file, good);
    await section('schematic');
    await page.waitForSelector('.section:not(.away) .stale-strip', { state: 'detached', timeout: 15000 });
    await page.keyboard.press('Control+j');
    await page.waitForSelector('.dock-empty');
    expect(true, 'no strip, the empty state');
  } finally {
    fs.writeFileSync(file, good);
  }
}
