// The system view: the boards of the system and the joins of their ports; a board's own
// members and nets.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  step('open the System tab');
  await section('system');
  await page.waitForSelector('.card.sys');
  const text = await page.locator('.sec-system .graph-host').innerText();
  expect(text.includes('PowerBoard') && text.includes('LedBoard'), 'both boards of the system');
  expect(text.includes('power.output ~ led.input'), 'the join between their ports');
  // The two connectors are mated, and the mate derives the join (ADR 0040).
  const mates = await page.locator('.mates').innerText();
  expect(mates.includes('power.power_out') && mates.includes('led.power_in'), 'the mated connectors are listed');
  expect(mates.includes('Ground') && mates.includes('power.GND') && mates.includes('led.GND'), 'the grounds the mate joins');
  await shot('system');
}
