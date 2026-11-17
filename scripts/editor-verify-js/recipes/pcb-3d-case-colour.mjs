// A case model in the 3D view is drawn in a colour (issue #756): the base states `.color &=
// Color::Blue` over a STEP file that has none, and the lid states none and is drawn in the colour
// its file styles it in (`#cc3366`). The scene file carries each as a `case:#rrggbb` material, the
// page tints the case's solids with it (see-through, or opaque in Solid), and hiding a model
// leaves the other in its own colour. A model that has neither would be the theme's grey (the
// other case recipes, `pcb-3d-case`, draw one).
import { whenIdle } from '../settle.mjs';
import { openPcb } from '../pcb-helpers.mjs';

export const fixture = 'case_colour_3d';

const BOARD = 'Tinted';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

/** The red, green and blue of `#rrggbb`. */
const rgb = (hex) => [1, 3, 5].map((k) => parseInt(hex.slice(k, k + 2), 16));

/** The colours of a grid of points over the board and a margin round it, seen from the top. */
async function grid(page, info) {
  const out = [];
  for (const fx of [-0.06, 0.02, 0.2, 0.5, 0.8, 0.98, 1.06]) {
    for (const fy of [-0.1, 0.1, 0.5, 0.9, 1.1]) {
      const p = await ask(page, 'toScreen', [info.widthMm * fx, info.heightMm * fy, 0]);
      const c = await ask(page, 'colourAt', p.x, p.y);
      if (c) out.push(c);
    }
  }
  return out;
}

const pinkish = ([r, g, b]) => r > 1.4 * g && r > 1.1 * b;
const bluish = ([r, g, b]) => b > 1.4 * r && b > 1.2 * g;

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await page.goto(tools.url);
  await page.waitForSelector('.app');
  await openPcb(page, BOARD);
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`);
  await page.locator(HOST).focus();
  await whenIdle(page);

  step("each case model is tinted: the base in the colour its board states, the lid in its file's");
  const tints = await ask(page, 'caseTints');
  const base = tints.base ?? [];
  const lid = tints.lid ?? [];
  expect(base.length > 0 && base.every((t) => t === '#3460b8'), `the base is in Blue's tone (${base.join(', ')})`);
  expect(lid.length > 0 && lid.every((t) => t === '#cc3366'), `the lid is in its file's colour (${lid.join(', ')})`);
  await shot('case-colour-see-through');

  step('drawn solid, the lid is pink over the board and, with the lid hidden, the box is blue');
  await page.keyboard.press('Shift+C');
  await whenIdle(page);
  expect((await ask(page, 'caseMode')) === 'solid', 'Shift+C draws the case solid');
  await page.keyboard.press('t');
  await whenIdle(page);
  const info = await ask(page, 'board');
  const both = await grid(page, info);
  expect(both.some((c) => pinkish(rgb(c))), `the lid shows pink (${both.join(', ')})`);
  await shot('case-colour-solid');
  await page.locator('[data-testid=case-model-toggle][data-case=lid]').click();
  await whenIdle(page);
  const noLid = await grid(page, info);
  expect(!noLid.some((c) => pinkish(rgb(c))), 'with the lid hidden nothing is pink');
  expect(noLid.some((c) => bluish(rgb(c))), `the box shows blue (${noLid.join(', ')})`);
  await shot('case-colour-lid-hidden');

  step('the lid alone is still pink: hiding the base leaves it in its own colour');
  await page.locator('[data-testid=case-model-toggle][data-case=lid]').click();
  await page.locator('[data-testid=case-model-toggle][data-case=base]').click();
  await whenIdle(page);
  const noBase = await grid(page, info);
  expect(noBase.some((c) => pinkish(rgb(c))) && !noBase.some((c) => bluish(rgb(c))), `only the lid is drawn, pink (${noBase.join(', ')})`);
  await shot('case-colour-base-hidden');
}
