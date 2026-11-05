// The problems panel of a big board: errors and warnings listed, notes in a closed section
// with a count, each note going to its source line; the status bar counts leave notes out.
export const fixture = 'big';

export async function run({ page, shot, step, expect, section }) {
  await section('source');
  step('the status bar counts errors and warnings, not notes');
  const bar = await page.locator('.statusbar .counts').innerText();
  expect(/0\s+0/.test(bar.replace(/\n/g, ' ')), `no errors and no warnings (${bar.replace(/\n/g, ' ')})`);

  step('open the dock: nothing wrong, with the number of notes that are hidden');
  await page.locator('.statusbar .counts').click();
  await page.waitForSelector('.dock');
  const empty = await page.locator('.dock-empty').innerText();
  expect(/No problems/.test(empty) && /\d+ notes hidden/.test(empty), `a calm empty state that says notes are hidden (${empty.replace(/\n/g, ' ')})`);
  expect((await page.locator('.problem.note').count()) === 0, 'the notes are not listed yet');
  await shot('notes-closed');

  step('show the notes: a note goes to its source line');
  await page.locator('.sev-toggle.note').click();
  const notes = await page.locator('.problem.note').count();
  expect(notes >= 10, `the notes (${notes})`);
  await page.locator('.problem.note', { hasText: 'pulls1.a' }).first().click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.a: Resistor'));
  expect((await page.locator('.tab.active[title]').innerText()).includes('pull_bank.nsm'), 'the module file with the resistor');
  await shot('notes-open');

  step('an error is listed, and counted');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\n.oops: NoSuchPart\n');
  await page.waitForSelector('.problem.error', { timeout: 20000 });
  expect((await page.locator('.sev-toggle.error .n').innerText()) === '1', 'one error in the dock header');
  await shot('error');
}
