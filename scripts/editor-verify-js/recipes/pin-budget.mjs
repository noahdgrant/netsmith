// The pin budget of a host processor in the schematic's board summary (issue 1284): with nothing selected, the inspector lists, for the
// microcontroller of the fixture, how many of its pins are used and left, what the board uses of it, and its serial blocks with how many
// are left. The numbers are the server's (`/api/board`'s `pin_budgets`, what `netsmith board explain u` prints).
import { whenIdle } from '../settle.mjs';

export const fixture = 'pin_map';

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic[data-board="PinMapBoard"]');
  const detail = await page.evaluate(async () => (await fetch('/api/board?name=PinMapBoard', { cache: 'no-store' })).json());
  step('the board detail has the budget of the microcontroller');
  const budget = (detail.pin_budgets ?? []).find((b) => b.host === 'u');
  expect(!!budget, 'the board has a pin budget for `u`');
  expect(budget.pins === 4 && budget.used === 3 && budget.left === 1, `4 pins, 3 used, 1 left (${budget.pins}, ${budget.used}, ${budget.left})`);

  step('the board summary shows it, with nothing selected');
  // The inspector is behind the Details button when the window is narrow.
  if (!(await page.locator('.sinspector').count())) await page.getByRole('button', { name: 'Details' }).click();
  await page.waitForSelector('[data-testid="pin-budget"][data-host="u"]');
  await whenIdle(page);
  const text = (await page.locator('[data-testid="pin-budget"][data-host="u"]').innerText()).replace(/\s+/g, ' ');
  expect(text.includes('3 of 4 pins used'), `the head says 3 of 4 pins used (${text})`);
  expect(text.includes('(board)') && text.includes('3 pins'), `the board itself uses the 3 pins (${text})`);
  expect(text.includes('other') && text.includes('0 of 1 used, 1 left (UARTE0)'), `the UART is left (${text})`);
  await shot('pin-budget');
}
