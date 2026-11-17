// A board that states its solder mask colour, `.stackup.mask_colour &= Color::Red` (issue #771),
// is drawn with a red mask in the 3D view: the scene file carries the colour's name on its root, the
// page draws the mask in the theme's own red (`--pcb3d-mask-red` and its mask-over-copper tone),
// and the colour follows the theme. Seen from the top, the bare mask near the board's corners is
// red, not the theme's own slate; a board that states none is not touched (palette.test.ts and
// the other 3D recipes keep the theme's tone).
import { whenIdle } from '../settle.mjs';
import { openPcb } from '../pcb-helpers.mjs';

export const fixture = 'mask_colour_3d';

const BOARD = 'Crimson';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

/** The red, green and blue of `#rrggbb`. */
const rgb = (hex) => [1, 3, 5].map((k) => parseInt(hex.slice(k, k + 2), 16));

/** Choose a theme as a person does: the command palette. */
async function theme(page, label, id) {
  await page.keyboard.press('Control+Shift+P');
  await page.keyboard.type(`Theme: ${label}`);
  await page.keyboard.press('Enter');
  await page.waitForFunction((t) => document.documentElement.dataset.theme === t, id);
}

/** The colours of the bare mask a little in from each corner of the top face, in the window. */
async function corners(page, info) {
  const out = [];
  for (const [fx, fy] of [[0.08, 0.1], [0.92, 0.1], [0.08, 0.9], [0.92, 0.9]]) {
    const p = await ask(page, 'toScreen', [info.widthMm * fx, info.heightMm * fy, 0]);
    const c = await ask(page, 'colourAt', p.x, p.y);
    if (c) out.push(c);
  }
  return out;
}

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await page.goto(tools.url);
  await page.waitForSelector('.app');
  await openPcb(page, BOARD);
  await theme(page, 'Dark', 'dark');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`);
  await page.locator(HOST).focus();

  step("the scene file names the board's mask colour");
  const info = await ask(page, 'board');
  expect(info.maskColour === 'Red', `the board states a red mask (${info.maskColour})`);

  step('seen from the top, the bare mask is red');
  await page.keyboard.press('t');
  await whenIdle(page);
  const dark = await corners(page, info);
  expect(dark.length > 0, 'the view draws the mask near the corners');
  const reddish = ([r, g, b]) => r > 1.4 * g && r > 1.4 * b;
  expect(dark.some((c) => reddish(rgb(c))), `a corner of the board is red in the dark theme (${dark.join(', ')})`);
  await shot('mask-red-dark');

  step("the light theme draws it in its own red, as it repaints the rest");
  await theme(page, 'Light', 'light');
  await whenIdle(page);
  const light = await corners(page, info);
  expect(light.some((c) => reddish(rgb(c))), `a corner of the board is red in the light theme (${light.join(', ')})`);
  expect(light.join() !== dark.join(), 'the colours changed with the theme');
  await shot('mask-red-light');
}
