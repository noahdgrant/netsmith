// The power tree of a big board: rails in columns by supply, readable cards, short numbers,
// loads heaviest first, and a red state when a supply is overdrawn.
export const fixture = 'big';

export async function run({ page, shot, step, expect, section }) {
  step('open the Power tree tab');
  await section('power');
  await page.waitForSelector('.card.power');
  const cards = await page.locator('.card.power').count();
  expect(cards >= 9, `a card for VIN and each of the 8 rails (${cards})`);
  const text = await page.locator('.sec-power .graph-host').innerText();
  expect(!/0\.0000000/.test(text), 'no number with a string of zeros');
  expect(/\d(m|u)?A\.\.\d/.test(text), 'currents with a unit prefix');
  const columns = await page.evaluate(() => new Set([...document.querySelectorAll('.card.power')].map((c) => c.style.left)).size);
  expect(columns >= 2, `the rails stand in columns by supply (${columns} columns)`);
  const edges = await page.locator('.edge.converts').count();
  expect(edges >= 8, `an edge from VIN to each rail (${edges})`);
  await shot('power-tree');

  step('loads are sorted by current, heaviest first');
  const who = await page.locator('.card.power', { hasText: 'RAIL8' }).first().locator('.line .who').allInnerTexts();
  expect(
    JSON.stringify(who.slice(1)) === JSON.stringify(['out8', 'light8.power', 'pulls8.power']),
    `out8 (up to 60mA), then light8 (15mA), then pulls8 (2mA) (${who.join(', ')})`,
  );

  step('an overdrawn supply is red');
  // The solver refuses to build a board whose regulator is overdrawn, so the answer is
  // changed in flight to show what the page does with one.
  await page.route('**/api/power?*', async (route) => {
    const response = await route.fetch();
    const tree = await response.json();
    const node = tree.nodes.find((n) => n.net === 'RAIL3');
    node.budget = 'over';
    await route.fulfill({ response, json: tree });
  });
  await page.reload();
  await section('power');
  await page.waitForSelector('.card.power.bad');
  expect((await page.locator('.card.power.bad').count()) === 1, 'one red card');
  expect(await page.locator('.card.power.bad .over-badge').isVisible(), 'a badge that says OVER');
  await shot('over-limit');
}
