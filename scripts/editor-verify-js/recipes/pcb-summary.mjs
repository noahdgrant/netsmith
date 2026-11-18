// The Summary tab of the PCB view (issue 1003): the placed board as numbers (totals, faces, modules,
// decoupling, nets), the parts near the selected part, and the distance between two pads typed in.
import { gate, whenIdle } from '../settle.mjs';
import { BOARD, PART, fresh, labelOf, openPcb, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools);
  await openPcb(page);
  const sc = await scene(page);
  const cap = partOf(sc, PART.inputCap);
  const reg = partOf(sc, PART.regulator);

  step('the Summary tab sums up the placement: totals, both faces, modules, decoupling, nets');
  await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
  await page.waitForSelector('[data-testid=summary-totals]');
  const totals = await page.locator('[data-testid=summary-totals]').innerText();
  expect(/\d+(\.\d+)? mm.*wire/.test(totals) && totals.includes('decoupling mean'), `the totals line (${totals})`);
  expect((await page.locator('[data-testid=summary-face]').count()) === 2, 'a line for each face');
  expect((await page.locator('[data-testid=summary-face][data-face=top]').innerText()).includes('mm² free'), 'the top face says how much is free');
  expect((await page.locator('[data-testid=summary-decoupling]').count()) >= 1, 'a decoupling capacitor is listed with its distance');
  expect((await page.locator('[data-testid=summary-net]').count()) >= 1, 'the longest nets are listed');
  await shot('summary');

  step('the numbers are the server\'s: the page shows what GET /api/pcb/summary says');
  const server = await page.evaluate(async (b) => (await fetch(`/api/pcb/summary?board=${encodeURIComponent(b)}`)).json(), BOARD);
  expect(server.layoutStamp === sc.layoutStamp, 'the summary is of the placement the scene draws');
  expect(totals.includes(`${server.totals.hpwl} mm`), `the page's wire length is the server's (${server.totals.hpwl})`);

  step('select a part: its neighbours are listed, nearest first');
  await page.locator('.side-tabs button', { hasText: 'Parts' }).click();
  await page.locator('.part-row', { hasText: labelOf(cap) }).first().click();
  // The fill of the pours is held until a distance is typed: its answer, which takes a while on any machine, re-renders the tab
  // while the box has not been left yet, and what is typed must stay.
  const filled = gate();
  let fillsHeld = 0;
  await page.route('**/api/pcb/fill?*', async (route) => {
    fillsHeld++;
    try {
      const answer = await route.fetch();
      await filled.opened;
      await route.fulfill({ response: answer });
    } catch {
      // The page was closed (a step before failed) while the answer was held: nothing is left to answer.
    }
  });
  await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
  await page.waitForSelector('[data-testid=summary-near]');
  const near = await page.locator('[data-testid=summary-near]').first().innerText();
  expect(near.startsWith(labelOf(cap)), `the neighbours of ${labelOf(cap)} (${near})`);
  await page.locator('[data-testid=summary-within]').fill('200');
  filled.open();
  await page.waitForSelector('[data-testid=summary-fill-counts], [data-testid=summary-fill-none], [data-testid=summary-fill-error]');
  expect(fillsHeld >= 1, 'the fill was asked for while the distance was typed');
  expect((await page.locator('[data-testid=summary-within]').inputValue()) === '200', 'the distance typed stays when an answer arrives before the box is left');
  await page.unroute('**/api/pcb/fill?*');
  await page.locator('[data-testid=summary-within]').blur();
  await whenIdle(page);
  const all = await page.locator('[data-testid=summary-near]').first().innerText();
  expect((all.match(/ mm/g) ?? []).length === sc.parts.length - 1, `every other part is within 200 mm (${all})`);

  step('type two pads: their distance is given, centre to centre and copper to copper');
  await page.locator('[data-testid=summary-pad-a]').fill(`${labelOf(cap)}.1`);
  await page.locator('[data-testid=summary-pad-b]').fill(`${PART.regulator}.1`);
  await page.locator('[data-testid=summary-measure]').click();
  await page.waitForSelector('[data-testid=summary-pads]');
  const pads = await page.locator('[data-testid=summary-pads]').first().innerText();
  expect(pads.includes(`${labelOf(cap)}.1 to ${labelOf(reg)}.1`) && pads.includes('between centres') && pads.includes('between the copper'), `the pair's line (${pads})`);
  await shot('summary-pads');

  step('a pad the part has none of is refused with the pads it has, and the pair that worked stays');
  await page.locator('[data-testid=summary-pad-a]').fill(`${labelOf(cap)}.99`);
  await page.locator('[data-testid=summary-pad-b]').fill(`${labelOf(cap)}.1`);
  await page.locator('[data-testid=summary-measure]').click();
  await page.waitForSelector('[data-testid=summary-refused]');
  expect((await page.locator('[data-testid=summary-refused]').innerText()).includes('has no pad `99`; its pads are: 1, 2'), 'the reason is shown');
  expect((await page.locator('[data-testid=summary-pads]').count()) === 1, 'the pair that worked stays');
  await page.locator('[data-testid=summary-clear]').click();
  await whenIdle(page);
  expect((await page.locator('[data-testid=summary-pads]').count()) === 0, 'Clear the pads takes them off');
  await shot('summary-refused');
}
