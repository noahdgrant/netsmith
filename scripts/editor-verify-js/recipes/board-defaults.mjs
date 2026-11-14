// A board's `defaults` block (issue #625) in the editor: a part's card says which statement gave its package (a line of the block,
// or the part's own `&=`), with the file and line, and a click goes there; a line that reaches no part is a note at that line.
import { whenIdle } from '../settle.mjs';
export const fixture = 'board_defaults';

export async function run({ page, shot, step, expect, section }) {
  await page.setViewportSize({ width: 1500, height: 950 });
  await section('source');
  await page.waitForSelector('.cm-content');
  const lineOf = (text) => page.locator('.cm-line', { hasText: text }).first();
  const lineNumber = async (text) => {
    const lines = await page.locator('.cm-line').allInnerTexts();
    return lines.findIndex((l) => l.includes(text)) + 1;
  };
  const away = async () => {
    await page.mouse.move(1450, 920);
    await whenIdle(page);
  };

  step('a part the block reaches says so, with the line of the block');
  const blockLine = await lineNumber('Resistor.package ?= "0402"');
  expect(blockLine > 0, 'the block is in the file');
  await lineOf('.from_block:').locator('span', { hasText: 'from_block' }).first().hover();
  await page.waitForSelector('[data-testid="hover-card"][data-kind="part"]');
  const from = page.locator('[data-testid="hc-package-from"]');
  await from.waitFor();
  const text = await from.innerText();
  expect(text === `board default, main.nsm:${blockLine}`, `the package's source (${text})`);
  const card = await page.locator('[data-testid="hover-card"]').innerText();
  expect(card.includes('0402'), `the package itself (${card.replace(/\s+/g, ' ').slice(0, 200)})`);
  await shot('defaults-card-block');

  step('a click on it goes to the line');
  await from.click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('Resistor.package ?= "0402"'));
  await whenIdle(page);
  await shot('defaults-went-to-line');
  await away();

  step("a part that states its own package says it is stated, at its own line");
  const ownLine = await lineNumber('.package &= "0805"');
  await lineOf('.own:').locator('span', { hasText: 'own' }).first().hover();
  await page.waitForSelector('[data-testid="hover-card"][data-kind="part"]');
  const own = await page.locator('[data-testid="hc-package-from"]').innerText();
  expect(own === `stated, main.nsm:${ownLine}`, `the stated package's source (${own})`);
  await shot('defaults-card-stated');
  await away();

  step('the line that reaches nothing is a note at that line');
  const problems = await page.evaluate(async () => (await (await fetch('/api/analysis')).json()).problems);
  const note = problems.find((p) => p.message.includes('the default for `Led.package` reaches nothing'));
  const ledLine = await lineNumber('Led.package ?= "0603"');
  expect(note?.severity === 'note' && note.at?.line === ledLine, `a note at line ${ledLine} (${JSON.stringify(note)})`);
}
