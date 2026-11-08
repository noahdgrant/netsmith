// The problems panel of a big board: errors and warnings listed, notes hidden behind their toggle
// with a count, then listed grouped by cause (a head with the cause and its count, collapsible),
// each note going to its source line; the status bar counts leave notes out.
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
  expect(Number(await page.locator('.sev-toggle.note .n').innerText()) === Number((await page.locator('.statusbar .counts .nt').innerText()).trim()), 'the same count as the status bar\'s');
  await shot('notes-closed');

  step('show the notes: a note goes to its source line');
  await page.locator('.sev-toggle.note').click();
  const notes = await page.locator('.problem.note').count();
  expect(notes >= 10, `the notes (${notes})`);

  step('the notes are grouped by cause: each head has the cause, its count and what it means, and the counts add up to the notes');
  expect((await page.locator('.dock-group').inputValue()) === 'cause', 'the default grouping is by cause');
  const heads = await page.locator('.dock-group-head').evaluateAll((els) =>
    els.map((e) => ({
      title: e.querySelector('[role=columnheader]').textContent.trim(),
      n: Number(e.querySelector('.count').textContent.trim()),
      why: e.querySelector('.why')?.textContent.trim() ?? '',
      open: e.querySelector('button').getAttribute('aria-expanded'),
    })),
  );
  expect(heads.length >= 1, `a head for each cause (${heads.map((h) => `${h.title} ${h.n}`).join(' | ')})`);
  expect(heads.reduce((a, h) => a + h.n, 0) === notes, `the counts of the heads add up to the ${notes} notes`);
  expect(heads.every((h) => h.why.length > 10 && h.open === 'true'), 'each head says what its cause is and is open');
  expect(new Set(heads.map((h) => h.title)).size === heads.length, 'one head for each cause');
  await shot('notes-by-cause');

  step('a head closes its group and opens it again; the other groups stay');
  const first = page.locator('.dock-group-head button').first();
  const inFirst = heads[0].n;
  await first.click();
  expect((await page.locator('.problem.note').count()) === notes - inFirst, 'the rows of the closed group are gone');
  expect((await first.getAttribute('aria-expanded')) === 'false', 'and the head says it is closed');
  await shot('group-closed');
  await first.click();
  expect((await page.locator('.problem.note').count()) === notes, 'it opens again');
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
