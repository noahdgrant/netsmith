// The power tree of the two-board system: rail cards joined by the stages between them, with
// voltage, headroom against the supply's limit, and loads that click through to the source.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  step('open the Power tree tab');
  await section('power');
  await page.waitForSelector('.pt-rail');
  const cards = await page.locator('.pt-rail').count();
  expect(cards >= 3, `a card for each rail net of the system (${cards})`);
  const text = (await page.locator('.sec-power .graph-host').innerText()).replace(/\s+/g, ' ');
  expect(text.includes('3V3') && text.includes('VIN'), 'the VIN and 3V3 nets');
  expect(/Headroom 50 mA of 100 mA/.test(text), `the regulator limit of 100 mA as the capacity, and what is left (${text.slice(0, 200)})`);
  expect(/No supply limit stated/.test(text), 'VIN is supplied from outside: no limit is stated');
  expect((await page.locator('.pt-stage').count()) >= 1 && (await page.locator('.pt-edge').count()) >= 2, 'the regulator is a stage between VIN and 3V3');
  await shot('power-tree');

  step('open the loads of a rail and click one: the editor goes to its source');
  await page.getByRole('button', { name: 'List all loads' }).click();
  const load = page.locator('.pt-list .pt-row').first();
  const who = (await load.locator('.pt-who').innerText()).trim();
  await load.click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForFunction(() => (document.querySelector('.cm-activeLine')?.textContent ?? '').trim().length > 0);
  expect(true, `the cursor on the declaration of ${who}`);
  const marks = await page.locator('.cm-selectionMatch').count();
  expect(marks < 5, `no run of marks in the source after the click-through (${marks})`);
  expect((await page.locator('.pane-strip').count()) === 1, 'in the same pane: no second pane was opened');
  await page.locator('.back-btn').click();
  await page.waitForSelector('.section.sec-power:not(.away)');
  expect(true, 'Back returns to the Power tree');
}
