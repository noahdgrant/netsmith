// The system view of a carrier with two plug-in cards: each mated pair of connectors with its
// contacts, the ports the mates join, and the ground group (ADR 0040).
export const fixture = 'carrier_and_cards';

export async function run({ page, shot, step, expect, section }) {
  step('open the System tab');
  await section('system');
  await page.waitForSelector('.sy-card');
  const graph = await page.locator('.sec-system .graph-host').innerText();
  expect(graph.includes('Carrier') && graph.includes('SensorCard') && graph.includes('SerialCard'), 'the three boards');
  expect(graph.includes('carrier.cards ~ sensor.supply'), 'the join the first mate derives');
  expect(graph.includes('carrier.cards ~ serial.supply'), 'the join the second mate derives');

  step('read the mates');
  const mates = await page.locator('.mates').innerText();
  expect(mates.includes('carrier.slot_a') && mates.includes('sensor.j1'), 'the first mate');
  expect(mates.includes('carrier.slot_b') && mates.includes('serial.j1'), 'the second mate');
  expect(mates.includes('10 contacts'), 'each mate has ten contacts');
  expect(
    mates.includes('carrier.GND + sensor.GND + serial.GND'),
    'one ground group of the three boards, each with its own GND net',
  );
  expect((await page.locator('.contact.role-ground').count()) >= 4, 'ground contacts are marked');
  expect((await page.locator('.contact.role-signal').count()) >= 6, 'signal contacts are marked');
  await shot('system-mates');
}
