// The PCB view in 3D (ADR 0105, features/pcb-3d.md): the toggle loads the 3D chunk only when it is
// first opened, the board is drawn (pixels that are not the background) once the first frame is
// ready, the coverage chip counts the parts with a known height, the hover names a part's height
// and its source, a click picks the part in the page's selection, a pick made elsewhere lights in
// 3D, the preset views move the camera, the list of parts with no height picks one, and the light
// theme repaints the view. The fixture has a part of each kind: a stated height that breaks the
// lid's limit, one that fits, and one with no known height.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, labelOf, openPcb, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'height_limits';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lidded';
const HOST = '[data-testid=pcb-3d]';

/** Ask the 3D view itself (the handle it puts on its host, for recipes). */
const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

/** A point of the window where the view picks the part at `path` (it raycasts over the part's bounds: a package shape has leads and a body, not a box). */
async function placeOf(page, path) {
  const at = await ask(page, 'project', path);
  if (!at) throw new Error(`the 3D view finds no point of the window that picks the part ${path}`);
  return at;
}

/** Choose a theme as a person does: the command palette. */
async function theme(page, label, id) {
  await page.keyboard.press('Control+Shift+P');
  await page.keyboard.type(`Theme: ${label}`);
  await page.keyboard.press('Enter');
  await page.waitForFunction((t) => document.documentElement.dataset.theme === t, id);
}

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/height_limits.pcb.toml'));
  await openPcb(page, BOARD);
  await theme(page, 'Dark', 'dark');
  const s = await scene(page, BOARD);
  const tall = partOf(s, 'tall');
  const flat = partOf(s, 'flat');
  const bare = partOf(s, 'bare');
  const chunk = () => page.evaluate(() => performance.getEntriesByType('resource').filter((r) => /\/View3d[-.]/.test(r.name)).length);

  step('the 2D view does not load the 3D chunk; the toggle does, and the view says when its first frame is drawn');
  expect((await chunk()) === 0, 'no 3D chunk before 3D is opened');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`, { timeout: 30000 });
  expect((await chunk()) > 0, 'the 3D chunk was loaded on demand');
  expect((await page.locator('.pcb-overlay').count()) === 0, 'the 2D canvas (and its editing keys) is gone');
  const drawn = await ask(page, 'sample');
  expect(drawn.drawn > 0.05 && drawn.colours > 12, `the canvas is drawn: ${(drawn.drawn * 100).toFixed(0)} % of it is not the background, ${drawn.colours} colours`);
  expect((await ask(page, 'preset')) === 'iso', 'it opens on the isometric view');
  const parts3d = await ask(page, 'parts');
  expect(parts3d.length === s.parts.length, `one part in 3D for each part of the board (${parts3d.length})`);
  const of3d = (path) => parts3d.find((p) => p.path === path);
  expect(of3d('tall').provenance === 'stated' && of3d('tall').heightMm === 9, 'the tall part stands 9 mm, stated');
  expect(of3d('bare').provenance === 'unknown' && of3d('bare').heightMm === null, 'the bare part has no height, and none is guessed');
  await shot('iso-dark');

  step('the coverage chip and the legend say where the heights came from');
  const known = s.parts.filter((p) => p.height !== null).length;
  const chip = await page.getByTestId('height-coverage').innerText();
  expect(chip.includes(`${known} of ${s.parts.length} parts have a known height`), `the chip: ${chip}`);
  const legend = await page.getByTestId('height-legend').innerText();
  expect(/Stated\s*2/.test(legend) && /Unknown\s*1/.test(legend) && legend.includes('Limit exceeded'), `the legend: ${legend.replace(/\n/g, ' | ')}`);

  step('the hover names the part, its height and where the height came from, and lights it in the other views');
  const hoverOn = async (p) => {
    const at = await placeOf(page, p.path);
    await page.mouse.move(at.x + 30, at.y + 30);
    await page.mouse.move(at.x, at.y);
    await page.waitForSelector('[data-testid=tip-3d]');
    return { at, text: await page.locator('[data-testid=tip-3d]').innerText() };
  };
  const hTall = await hoverOn(tall);
  expect(
    hTall.text.includes(tall.refdes) && hTall.text.includes('9 mm tall, stated in the design') && hTall.text.includes('Too tall for the limit lid'),
    `the tall part: ${hTall.text.replace(/\n/g, ' | ')}`,
  );
  await page.waitForFunction(() => document.documentElement.classList.contains('has-hover'));
  const hBare = await hoverOn(bare);
  expect(hBare.text.includes(bare.refdes) && hBare.text.includes('Height not known'), `the part with no height: ${hBare.text.replace(/\n/g, ' | ')}`);
  await shot('hover-unknown');

  step('a click picks the part in the page: the inspector names it, and its body takes the selection colour');
  const at = await placeOf(page, flat.path);
  const before = await ask(page, 'colourAt', at.x, at.y);
  await page.mouse.move(at.x, at.y);
  await page.mouse.click(at.x, at.y);
  await page.waitForSelector('[data-testid=part-inspector]');
  const title = await page.locator('[data-testid=part-inspector] h3').innerText();
  expect(title.startsWith(labelOf(flat)), `the inspector shows ${labelOf(flat)} (${title})`);
  await page.mouse.move(5, 5);
  await page.waitForTimeout(250);
  const after = await ask(page, 'colourAt', at.x, at.y);
  expect(before !== after, `the picked body changed colour (${before} to ${after})`);
  await shot('picked');

  step('a pick made in the side panel lights the part in 3D');
  await page.locator('.part-row', { hasText: labelOf(tall) }).click();
  await page.waitForFunction((t) => document.querySelector('[data-testid=part-inspector] h3')?.innerText.startsWith(t), labelOf(tall));
  await page.waitForTimeout(250);
  const tallAt = await placeOf(page, tall.path);
  const lit = await ask(page, 'colourAt', tallAt.x, tallAt.y);
  const flatNow = await ask(page, 'colourAt', at.x, at.y);
  expect(flatNow === before, `the part picked before is back to its own colour (${flatNow})`);
  expect(lit !== null && lit !== flatNow, `the part picked in the list is lit (${lit})`);

  step('the preset views: top, then the flip to the bottom; perspective and orthographic');
  await page.getByTestId('view-top').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'top', HOST);
  await page.waitForTimeout(300);
  await shot('top');
  await page.getByTestId('view-flip').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'bottom', HOST);
  await page.getByTestId('view-projection').click();
  expect((await ask(page, 'ortho')) === true, 'orthographic');
  await page.keyboard.press('p');
  expect((await ask(page, 'ortho')) === false, 'the key P goes back to perspective');
  await page.keyboard.press('i');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'iso', HOST);

  step('the list of parts with no height: a click on one picks it');
  await page.getByTestId('height-coverage').click();
  await page.waitForSelector('[data-testid=unknown-heights]');
  const rows = await page.locator('[data-testid=unknown-heights] .v3-unknown-row').allInnerTexts();
  expect(rows.length === 1 && rows[0].includes(bare.refdes), `one part with no height: ${rows.join(' | ')}`);
  await page.locator('[data-testid=unknown-heights] .v3-unknown-row').first().click();
  await page.waitForFunction((t) => document.querySelector('[data-testid=part-inspector] h3')?.innerText.startsWith(t), labelOf(bare));
  await page.waitForTimeout(300);
  await shot('unknown-list');

  step('the light theme repaints the view in its own colours');
  const darkBg = await ask(page, 'colourAt', 20, 400);
  await theme(page, 'Light', 'light');
  await page.waitForTimeout(300);
  const box = await page.locator(HOST).boundingBox();
  const lightBg = await ask(page, 'colourAt', box.x + 6, box.y + box.height - 40);
  expect(lightBg !== darkBg, `the background changed with the theme (${darkBg} to ${lightBg})`);
  await shot('iso-light');

  step('back to 2D: the canvas that edits the board is there again');
  await page.getByTestId('pcb-2d').click();
  await page.waitForSelector('.pcb-overlay');
  expect((await page.locator(HOST).count()) === 0, 'the 3D view is gone');
}
