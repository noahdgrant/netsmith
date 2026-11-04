// A project with one board and no system (launch --fixture regulator): the power tree and
// the board's own hierarchy in the System tab.
export const fixture = 'regulator';

export async function run({ page, shot, step, expect, section }) {
  step('the power tree of one board');
  await section('power');
  await page.waitForSelector('.card.power');
  const text = await page.locator('.sec-power .graph-host').innerText();
  expect(text.includes('VIN') && text.includes('3V3'), 'the input and output nets');
  expect(/of 100mA/.test(text), 'the 100mA the regulator can give');
  expect(text.includes('within what the supply can give'), 'the budget within the limit');
  await shot('power');

  step('the System tab shows the members of the board and the nets they share');
  await section('system');
  await page.waitForSelector('.card.sys');
  const members = await page.locator('.sec-system .graph-host').innerText();
  expect(members.includes('stage') && members.includes('power_in'), 'the stage module and the header');
  await shot('members');
}
