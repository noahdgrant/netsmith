// The problems panel of a big board: errors and warnings listed, notes in a closed section
// with a count, each note going to its source line; the status bar counts leave notes out.
export const fixture = 'big';

export async function run({ page, shot, step, expect }) {
  step('the status bar counts errors and warnings, not notes');
  const bar = await page.locator('.statusbar .counts').innerText();
  expect(/0\s+▲\s*0|⊗ 0\s*▲ 0/.test(bar.replace(/\n/g, ' ')), `no errors and no warnings (${bar.replace(/\n/g, ' ')})`);

  step('open the panel: no errors or warnings, a collapsed Notes section with its count');
  await page.locator('.problems-head').click();
  const head = await page.locator('.problems-head').innerText();
  expect(/\d+ notes/.test(head), `the count of notes in the header (${head.replace(/\n/g, ' ')})`);
  expect((await page.locator('.problem-list').innerText()).includes('No errors or warnings'), 'nothing listed above the notes');
  expect((await page.locator('.problem.note').count()) === 0, 'the notes are not listed yet');
  await shot('notes-closed');

  step('open Notes: a note goes to its source line');
  await page.locator('.section-head.clickable').click();
  const notes = await page.locator('.problem.note').count();
  expect(notes >= 10, `the notes (${notes})`);
  await page.locator('.problem.note', { hasText: 'pulls1.a' }).first().click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.a: Resistor'));
  expect((await page.locator('.tab.active[title]').innerText()).includes('pull_bank.nsm'), 'the module file with the resistor');
  await shot('notes-open');

  step('an error is listed above the notes, and counted');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\n.oops: NoSuchPart\n');
  await page.waitForSelector('.problem.error', { timeout: 20000 });
  expect((await page.locator('.section-head', { hasText: 'Errors' }).count()) === 1, 'an Errors section');
  await shot('error');
}
