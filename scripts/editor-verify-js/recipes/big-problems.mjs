// The problems panel of a big board: errors and warnings listed, notes in a closed section
// with a count, each note going to its source line; the status bar counts leave notes out.
export const fixture = 'big';

export async function run({ page, shot, step, expect, section }) {
  await section('source');
  step('the status bar counts errors and warnings, and shows the notes\' count as a quieter third figure');
  const bar = await page.locator('.statusbar .counts').innerText();
  expect(/^0\s/.test(bar.replace(/\n/g, ' ')), `no errors (${bar.replace(/\n/g, ' ')})`);
  await page.waitForSelector('.statusbar .counts .nt');
  const barNotes = Number((await page.locator('.statusbar .counts .nt').innerText()).trim());
  expect(barNotes > 0, `and the notes are counted too (${barNotes})`);

  step('open the dock: the warnings are listed and the notes are hidden, with their count');
  await page.locator('.statusbar .counts').click();
  await page.waitForSelector('.dock');
  expect((await page.locator('.problem.note').count()) === 0, 'the notes are not listed yet');
  expect(/\d+\s+Notes/.test(await page.locator('.sev-toggle.note').innerText()), 'the Notes toggle has their count');
  await page.waitForTimeout(1500); // the power tree's notes land a moment after the analysis: both counts are read after
  expect(Number(await page.locator('.sev-toggle.note .n').innerText()) === Number((await page.locator('.statusbar .counts .nt').innerText()).trim()), 'the same count as the status bar\'s');
  await shot('notes-closed');

  step('show the notes: a note goes to its source line');
  await page.locator('.sev-toggle.note').click();
  const notes = await page.locator('.problem.note').count();
  expect(notes >= 10, `the notes (${notes})`);
  await page.locator('.problem.note', { hasText: 'pulls1.a' }).first().click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.a: Resistor'));
  expect((await page.locator('.tabbar.files .tab.active[title]').innerText()).includes('pull_bank.nsm'), 'the module file with the resistor');
  await shot('notes-open');

  step('an error is listed, and counted');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\n.oops: NoSuchPart\n');
  await page.waitForSelector('.problem.error', { timeout: 20000 });
  expect(Number(await page.locator('.sev-toggle.error .n').innerText()) >= 1, 'the error is counted in the dock header');
  await shot('error');
}
