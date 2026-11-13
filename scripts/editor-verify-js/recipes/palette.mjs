// One selection in every view, go to anything (Ctrl+P), the command palette (Ctrl+Shift+P, Ctrl+K)
// and the keyboard cheat sheet (?). Parts are found by their instance path from /api/board, never
// by a designator typed in the recipe.
import { whenIdle } from '../settle.mjs';
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  const board = 'TwoBoards/power';
  await page.selectOption('.boardpick select', board);
  await page.waitForSelector('.pane-strip [data-section="schematic"]');
  const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
  const part = detail.parts.find((p) => p.path.includes('.')) ?? detail.parts[0];
  const net = detail.nets.find((n) => n.named && n.pins.length > 1);
  const chip = async () => ((await page.locator('.selchip').count()) ? await page.locator('.selchip').innerText() : '');
  const lit = (refdes) => page.evaluate((r) => !!document.querySelector(`svg.schematic .part.mark:not(.linked)[data-ref="${r}"]`), refdes);

  step('Ctrl+P opens go to anything; the designator finds the part (grouped, marked), Enter selects it');
  await page.keyboard.press('Control+p');
  await page.waitForSelector('.qp[role=dialog]');
  expect((await page.locator('.qp input[role=combobox]').count()) === 1, 'the input is a combobox in a dialog');
  await page.keyboard.type(part.refdes.toLowerCase());
  await page.waitForSelector('.qp-row[role=option]');
  expect((await page.locator('.qp-row').first().innerText()).startsWith(part.refdes), `${part.refdes} is the first result`);
  const groupName = (i) => page.locator('.qp-group').nth(i).evaluate((el) => el.firstChild.textContent.trim().toLowerCase());
  expect((await groupName(0)) === 'parts', 'the group is Parts');
  expect((await page.locator('.qp-group .qp-group-count').first().innerText()) === String(await page.evaluate(() => document.querySelector('.qp-group').parentElement.querySelectorAll('.qp-row').length)), 'its head counts its rows');
  expect((await page.locator('.qp-row .qp-hit').count()) > 0, 'the matched letters are marked');
  await shot('goto-part');
  await page.keyboard.press('Enter');
  await whenIdle(page);
  expect((await page.locator('.qp').count()) === 0, 'the window closes');
  expect((await chip()).includes(part.refdes), `the top bar chip shows ${part.refdes}`);
  expect(await lit(part.refdes), 'the schematic marks the part');
  await shot('selected-schematic');

  step('the same selection is lit in the BOM, the PCB and the power tree');
  await page.keyboard.press('5');
  await page.waitForSelector('.section.sec-bom:not(.away) .bom tbody tr');
  expect((await page.locator('.bom tr.sel').count()) >= 1, 'a BOM row is lit');
  await page.keyboard.press('6');
  await page.waitForSelector('.pcb-overlay');
  await whenIdle(page);
  expect((await page.locator('[data-testid=part-inspector]').count()) === 1, 'the PCB inspector shows the part');
  await shot('selected-pcb');
  await page.keyboard.press('3');
  await page.waitForSelector('.section.sec-power:not(.away)');
  await shot('selected-power');

  step('# finds a net; the net is selected in every view and the schematic dims the rest');
  await page.keyboard.press('2');
  await page.keyboard.press('Control+p');
  await page.keyboard.press('Control+a');
  await page.keyboard.type(`#${net.name}`);
  await page.waitForSelector('.qp-row');
  expect((await page.locator('.qp-row').first().innerText()).startsWith(net.name), `${net.name} is the first net`);
  await page.keyboard.press('Enter');
  await whenIdle(page);
  expect((await chip()).includes(net.name), 'the chip shows the net');
  expect(await page.evaluate(() => document.querySelector('svg.schematic')?.classList.contains('has-on')), 'the schematic dims what is not on the net');
  await shot('selected-net');
  await page.keyboard.press('Escape');
  await whenIdle(page);
  expect((await chip()) === '', 'Esc clears the selection');

  step('Ctrl+Shift+P and Ctrl+K are the command palette: keys at the right, a command runs, recent first');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp');
  expect((await page.locator('.qp input').inputValue()) === '>', 'it starts in command mode');
  expect((await page.locator('.qp-row kbd').count()) > 3, 'the key chords are shown');
  await shot('commands');
  await page.keyboard.type('power');
  await page.keyboard.press('Enter');
  await page.waitForSelector('.section.sec-power:not(.away)');
  await page.keyboard.press('Control+k');
  await page.waitForSelector('.qp');
  expect((await page.locator('.qp-group').first().evaluate((el) => el.firstChild.textContent.trim().toLowerCase())) === 'recently used', 'the command just used is first');
  await page.keyboard.press('Escape');
  expect((await page.locator('.qp').count()) === 0, 'Esc closes');

  step('the last search of the session comes back, selected');
  await page.keyboard.press('Control+p');
  expect((await page.locator('.qp input').inputValue()) === `#${net.name}`, 'the last query is back');
  await page.keyboard.press('Escape');

  step('? opens the cheat sheet, grouped; Esc closes; a ? typed in a field is only a character');
  await page.keyboard.press('?');
  await page.waitForSelector('.cheat[role=dialog]');
  expect((await page.locator('.cheat h3').count()) >= 6, 'the shortcuts are grouped');
  await shot('cheat-sheet');
  await page.keyboard.press('Escape');
  expect((await page.locator('.cheat').count()) === 0, 'Esc closes the cheat sheet');
  await page.keyboard.press('1');
  await page.locator('.tree-filter').click();
  await page.keyboard.type('?');
  expect((await page.locator('.cheat').count()) === 0, 'a ? in a text field does not open it');
}
