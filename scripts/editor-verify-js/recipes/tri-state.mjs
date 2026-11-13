// The schematic's box of a part whose pin table has a `tri_state` row (ADR 0064): the view draws no
// symbol for a pin's kind, only the side it is on (an output goes right, an input left, a
// bidirectional pin to the shorter side), so a `tri_state` pin is on the right with the outputs
// and a mark on it names the kind, like the marks of the other kinds.
import { whenIdle } from '../settle.mjs';
export const fixture = 'tri_state';

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic[data-board="TriStateBoard"]');
  await page.waitForSelector('.schematic .part[data-ref="U2"]');
  await whenIdle(page);
  await shot('boxes');

  step('read the side of each pin name from the drawing');
  const anchors = await page.evaluate(() => {
    const out = {};
    for (const part of document.querySelectorAll('.schematic .part')) {
      for (const t of part.querySelectorAll('.pin .pin-name')) {
        out[`${part.getAttribute('data-ref')} ${t.textContent}`] = t.getAttribute('text-anchor');
      }
    }
    return out;
  });
  const side = (name) => (anchors[name] === 'start' ? 'left' : 'right');
  expect(side('U1 SCK') === 'right' && side('U1 MISO') === 'left', `an output is on the right and an input on the left (${JSON.stringify(anchors)})`);
  expect(side('U2 DO') === side('U1 SCK'), `the tri_state DO is on the side of the outputs (${side('U2 DO')})`);
  expect(side('U2 RDY') === side('U1 SCK'), `so is the tri_state RDY (${side('U2 RDY')})`);
  expect(side('U2 DI') === 'left' && side('U2 CLK') === 'left', 'the flash inputs are on the left');

  step('the open tri_state pin has the mark of any open pin, and names its kind');
  const marks = await page.evaluate(() => [...document.querySelectorAll('.schematic .nc title')].map((t) => t.textContent));
  expect(marks.some((m) => m === 'not connected: U2 RDY (tri_state)'), `a mark on RDY that says tri_state (${marks.join(' | ')})`);
  expect(marks.some((m) => m === 'not connected: U1 IO7 (bidirectional)'), 'and the bidirectional pin says its kind the same way');
  await shot('marks');
}
