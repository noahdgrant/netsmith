// A polygon outline on the PCB canvas (issue #57): the sheet draws the polygon's own edge, an L
// with fillets and an arc, not its bounding box, and every part's courtyard is inside it.
import { openPcb, scene } from '../pcb-helpers.mjs';

export const fixture = 'polygon_outline';

export async function run(tools) {
  const { page, url, shot, step, expect } = tools;
  // The fixture has no layout file: the placer puts the parts, and nothing changes it here.
  await page.goto(url);
  await page.waitForSelector('.app');
  await openPcb(page, 'Ell');

  step('the outline is the polygon, drawn as a path, and its box is the board size');
  const s = await scene(page, 'Ell');
  expect((await page.locator('.pcb-host svg path[data-outline="polygon"]').count()) === 1, 'the polygon edge is drawn as a path');
  expect((await page.locator('.pcb-host svg g.l-outline > rect').count()) === 0, 'no rectangle is drawn for the outline');
  expect(Math.abs(s.outline.width - 40) < 0.01, `the board is 40 mm wide (${s.outline.width})`);
  expect(Math.abs(s.outline.height - 34) < 0.01, `the bulge, whose lowest point is the one it passes through, makes it 34 mm tall (${s.outline.height})`);

  step('no part is in the cut-away corner');
  const notch = { x0: s.originX + 25, x1: s.originX + 40, y0: s.originY, y1: s.originY + 15 };
  const inNotch = s.parts.filter((p) => p.courtyard && p.courtyard.x + p.courtyard.width > notch.x0 + 0.01 && p.courtyard.y < notch.y1 - 0.01);
  expect(s.parts.length === 3, `three parts (${s.parts.length})`);
  expect(inNotch.length === 0, `no courtyard reaches into the notch (${inNotch.map((p) => p.refdes).join(', ')})`);
  await shot('polygon');
}
