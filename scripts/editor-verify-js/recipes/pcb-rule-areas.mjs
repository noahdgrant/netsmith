// The rule areas on the 2D PCB view (issue 1543): the antenna part's own footprint carries a keep-out zone that forbids tracks, vias
// and pours on both copper layers, as an RF module's antenna area does, and the view hatches it where the part is, with a tip naming
// what it forbids, before any design rules check. It is the server's (`GET /api/pcb/rule-areas`, the `rule_areas` of `netsmith
// board rules`, what `netsmith view pcb` hatches). `Rule areas` in the Layers menu hides it and brings it back, apart from Keepouts.
import { whenIdle } from '../settle.mjs';
import { openPcb, scene } from '../pcb-helpers.mjs';

export const fixture = 'rule_areas';
const BOARD = 'RuleAreas';

export async function run(tools) {
  const { page, url, shot, step, expect } = tools;
  // The fixture has no layout file: the placer puts the parts, and nothing changes it here.
  await page.goto(url);
  await page.waitForSelector('.app');
  await openPcb(page, BOARD);
  const sc = await scene(page, BOARD);
  // The rule areas are asked after the stitching: wait for those of the placement drawn.
  await page.waitForSelector('[data-testid=rule-areas][data-current=true]', { state: 'attached' });
  await whenIdle(page);

  step('the server lists the footprint\'s rule area of the placement drawn, and the page hatches it where it says');
  const server = await page.evaluate(async (b) => (await fetch(`/api/pcb/rule-areas?board=${encodeURIComponent(b)}`)).json(), BOARD);
  expect(server.layoutStamp === sc.layoutStamp && server.routesStamp === sc.routesStamp, 'of the placement the scene draws');
  const antenna = server.areas.find((a) => a.name === 'antenna');
  expect(!!antenna, `the antenna's area is listed (${server.areas.map((a) => a.name).join(', ')})`);
  const part = sc.parts.find((p) => p.refdes === antenna.part);
  expect(!!part, `it is the footprint's of ${antenna.part}`);
  expect(antenna.forbids.join(' ') === 'tracks vias copperpour', `it forbids tracks, vias and pours (${antenna.forbids.join(' ')})`);
  expect(antenna.layers.join(' ') === 'F.Cu B.Cu', `on both layers (${antenna.layers.join(' ')})`);
  const drawn = page.locator('.l-rule-areas > path.rule-area[data-rule-area="antenna"]');
  expect((await drawn.count()) === 1, 'one hatched path for it');
  const look = await drawn.evaluate((p) => ({
    d: p.getAttribute('d'),
    tip: p.querySelector('title')?.textContent,
    fill: getComputedStyle(p).fill,
    dash: getComputedStyle(p).strokeDasharray,
    box: (({ x, y, width, height }) => ({ x, y, width, height }))(p.getBBox()),
  }));
  const first = antenna.outlines[0][0];
  expect(look.d.startsWith(`M${Math.round(first.x * 1000) / 1000} ${Math.round(first.y * 1000) / 1000}`), `where the server says (${look.d.slice(0, 30)})`);
  expect(look.tip === `antenna of ${antenna.part} forbids tracks, vias, copperpour on F.Cu, B.Cu`, `its tip names the kinds (${look.tip})`);
  expect(look.fill.includes('pcb-hatch-keepout'), `hatched as a keepout is (${look.fill})`);
  expect(look.dash !== 'none', `its outline dashed (${look.dash})`);
  const c = part.courtyard;
  const inside = look.box.x >= c.x - 0.01 && look.box.y >= c.y - 0.01 && look.box.x + look.box.width <= c.x + c.width + 0.01 && look.box.y + look.box.height <= c.y + c.height + 0.01;
  expect(inside, `inside its part's courtyard, so it moves with the part (${JSON.stringify(look.box)} in ${JSON.stringify(c)})`);
  await page.getByRole('button', { name: 'Fit' }).click();
  await whenIdle(page);
  await shot('rule-areas');

  step('the Layers menu hides the rule areas and brings them back, apart from the keepouts');
  const shown = (sel) => page.locator(sel).first().evaluate((e) => getComputedStyle(e).display !== 'none');
  await page.locator('details.layers summary').click();
  const toggle = async (label) => {
    await page.locator('.layer-list label', { hasText: label }).locator('input').click();
    await whenIdle(page);
  };
  await toggle('Rule areas');
  expect(!(await shown('.l-rule-areas')), 'Rule areas off: the hatch goes');
  await shot('rule-areas-off');
  await toggle('Rule areas');
  expect(await shown('.l-rule-areas'), 'Rule areas on: it comes back');
  await toggle('Keepouts');
  expect(await shown('.l-rule-areas'), 'Keepouts off leaves the rule areas');
  await page.locator('.layer-list button', { hasText: 'Show all' }).click();
  await whenIdle(page);
  await page.keyboard.press('Escape');
}
