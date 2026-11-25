// The warning of a switching regulator's ground pad in the Summary tab of the PCB view (issues 1586 and 1635):
// a ground pad of a part that provides `BuckIc` that no fill of its net on its layer joins solid is listed under
// "Pads in the pours", with how each fill holds it and the pin that has the pad to state
// `.pour_connection &= PadConnection::Solid` on (issues 1666 and 1667), in the words `netsmith board fill` warns with. The list is the server's (`regulatorGrounds` of `GET /api/pcb/fill`).
import { whenIdle } from '../settle.mjs';

export const fixture = 'buck_pours';
const BOARD = 'BuckPours';

export async function run(tools) {
  const { page, shot, step, expect, url } = tools;
  await page.goto(url);
  await page.waitForSelector('.app');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
  await page.waitForSelector('[data-testid=summary-fill-counts]');
  await whenIdle(page);
  const server = await page.evaluate(async (b) => (await fetch(`/api/pcb/fill?board=${encodeURIComponent(b)}`)).json(), BOARD);

  step('the regulator\'s ground pad on a thermal relief is a warning, with the pin to state it solid on');
  expect(server.regulatorGrounds?.length === 1, `the server warns of one ground pad (${JSON.stringify(server.regulatorGrounds)})`);
  const [ground] = server.regulatorGrounds;
  expect(ground.path === 'reg.ic' && ground.net === 'GND' && ground.join === 'thermal', `of reg.ic, on a relief of a ground pour (${JSON.stringify(ground)})`);
  const warning = page.locator('[data-testid=summary-fill-regulator-ground]');
  expect((await warning.count()) === 1, 'the page lists it');
  expect((await warning.getAttribute('data-pad')) === `${ground.part}.${ground.pad}`, 'the same pad');
  expect((await warning.getAttribute('data-layer')) === ground.layer && (await warning.getAttribute('data-join')) === 'thermal', 'on the same layer, joined the same way');
  expect((await warning.getAttribute('class')).split(' ').includes('warn'), 'as a warning');
  await warning.scrollIntoViewIfNeeded();
  await whenIdle(page);
  const text = await warning.innerText();
  expect(text.includes('the ground of the switching regulator reg.ic') && text.includes(`the pour holds it on a thermal relief, ${ground.joinedSpokes} of ${ground.spokes} spokes`), `what is wrong with it (${text})`);
  expect(ground.pin === 'reg.ic.vin.neg' && text.includes(`.pour_connection &= PadConnection::Solid on its pin ${ground.pin}, which reaches`), `what to state, and on which pin (${text})`);
  await shot('fill-regulator-ground');
}
