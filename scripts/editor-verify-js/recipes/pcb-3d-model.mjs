// A part drawn from its own STEP model in the 3D view (ADR 0108): the header of the fixture
// `model_3d` names a model written for it (a black plastic block, two gold pins, a red mark by
// pin 1), which the view draws in the file's colours in place of the header's generated shape;
// the hover says which file it is drawn from, and the height is still the part's own. The
// resistor names a file that is not in the project: a warning in the problems dock, and the
// resistor keeps its shape. Seen from the top, the plastic fills the header's silkscreen outline
// and the pins stand on its pads, which they do only if the model's y runs up the footprint, as
// KiCad's does.
import { whenIdle } from '../settle.mjs';
import { openPcb, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'model_3d';

const BOARD = 'Modelled';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

/** Let the view draw what changed (a frame, and a camera move of 180 ms). */

/** The red, green and blue of `#rrggbb`. */
const rgb = (hex) => [1, 3, 5].map((k) => parseInt(hex.slice(k, k + 2), 16));

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await page.goto(tools.url);
  await page.waitForSelector('.app');
  await openPcb(page, BOARD);
  const s = await scene(page, BOARD);
  const header = partOf(s, 'power_in');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`);
  await page.locator(HOST).focus();
  await page.keyboard.press('+');
  await page.keyboard.press('+');
  await whenIdle(page);

  step('the header is drawn from its model: the hover names the file, and the height is the stated one');
  const at = await ask(page, 'project', header.path);
  expect(at, 'the view finds the header');
  await page.mouse.move(at.x + 30, at.y + 30);
  await page.mouse.move(at.x, at.y);
  await page.waitForSelector('[data-testid=tip-3d]');
  const tip = await page.locator('[data-testid=tip-3d]').innerText();
  expect(tip.includes('Drawn from ../models/header.step'), `the hover names the model: ${tip.replace(/\n/g, ' | ')}`);
  expect(tip.includes('8.5 mm tall, stated in the design'), `the height is the part's own: ${tip.replace(/\n/g, ' | ')}`);
  await shot('model-hover');
  await page.mouse.move(5, 5);
  await page.waitForFunction(() => !document.documentElement.classList.contains('has-hover'));

  step("seen from the top, the model's colours: the gold pins on the pads, the black plastic round them");
  await page.keyboard.press('t');
  await whenIdle(page);
  const top = await ask(page, 'project', header.path);
  const centre = await ask(page, 'colourAt', top.x, top.y);
  expect(centre !== null, 'the view draws the header');
  // The plastic is the file's near black (31, 31, 33), whatever the theme's body tone is.
  const [r, g, b] = rgb(centre);
  expect(Math.max(r, g, b) < 90, `the header's middle is the model's plastic, not a body tone (${centre})`);
  await shot('model-top');

  step('a model that is not in the project is a warning, and the resistor keeps its shape');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('[data-testid=dock]');
  const missing = page.locator('[data-testid=problem]', { hasText: 'missing.step' }).first();
  await missing.waitFor();
  const text = await missing.innerText();
  expect(text.includes('drawn with its generated shape'), `the warning says what is drawn: ${text}`);
  await page.keyboard.press('Control+j');
  await page.keyboard.press('i');
  await whenIdle(page);
  await shot('model-iso');
}
