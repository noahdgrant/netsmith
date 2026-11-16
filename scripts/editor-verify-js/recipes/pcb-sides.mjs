// The 2D PCB view's keepouts, height limits and sides (issue #770): the Layers menu hides keepouts and height limits apart; a height limit is
// an outline with its limit and face written at its corner, never a fill, so seven of an enclosure's limits do not hide the parts under them;
// the Side switch (and the key B) looks at the front, the back (mirrored left to right, as KiCad's flipped view is, every text still reading
// the right way round) or both, and the zones drawn are those of the face shown; a part on the back is called `back` in the list and the inspector.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { whenIdle } from '../settle.mjs';
import { fresh, labelOf, openPcb, partAt, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'enclosure_limits';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Enclosed';
const LIMITS = ['floor', 'floor_edge', 'boss', 'tab', 'shank', 'rib_a', 'rib_b'];
const TOP = ['boss', 'tab', 'shank', 'rib_a', 'rib_b'];
const BOTTOM = ['floor', 'floor_edge'];

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/enclosure_limits.pcb.toml'));
  await openPcb(page, BOARD);
  await page.getByRole('button', { name: 'Fit' }).click();
  await whenIdle(page);

  // The names of the zones that are drawn (not hidden by a rule of the page): the rectangles, and the labels of the limits.
  const drawn = (sel) => page.locator(sel).evaluateAll((els) => els.filter((e) => getComputedStyle(e).display !== 'none').map((e) => e.getAttribute('data-keepout') ?? e.getAttribute('data-height-limit')).sort());
  const rects = () => drawn('.pcb-host svg [data-keepout]');
  const labels = () => drawn('.pcb-host svg text[data-height-limit]');
  const side = async (which) => {
    await page.locator(`[data-testid=pcb-side-${which}]`).click();
    await whenIdle(page);
  };
  const layer = async (name, on) => {
    const box = page.locator('.layer-list label', { hasText: name }).locator('input');
    if (!(await page.locator('.layer-list').isVisible())) await page.locator('.layers summary').click();
    await (on ? box.check() : box.uncheck());
    await whenIdle(page);
  };

  step('both faces: eight zones are drawn, and a height limit is an outline, not a fill, with its limit and face at its corner');
  const s = await scene(page, BOARD);
  expect(s.keepouts.length === 8, `the scene has eight zones (${s.keepouts.map((k) => k.name).join(', ')})`);
  expect((await rects()).join() === [...LIMITS, 'antenna'].sort().join(), `all of them are drawn: ${(await rects()).join(', ')}`);
  const fills = await page.locator('.pcb-host svg [data-keepout][data-limit-only]').evaluateAll((els) => els.map((e) => getComputedStyle(e).fill));
  expect(fills.length === 7 && fills.every((f) => f === 'none'), `no height limit has a fill (${[...new Set(fills)].join(', ')})`);
  const hatch = await page.locator('.pcb-host svg [data-keepout="antenna"]').evaluate((e) => getComputedStyle(e).fill);
  expect(hatch.includes('pcb-hatch-keepout'), 'the keepout that forbids parts is still hatched');
  const texts = await page.locator('.pcb-host svg text[data-height-limit]').evaluateAll((els) => els.map((e) => [e.getAttribute('data-height-limit'), e.textContent]));
  const say = Object.fromEntries(texts);
  expect(say.floor === '3.9 mm, bottom' && say.rib_a === '9 mm, top' && say.shank === '0.8 mm, top', `each limit says itself: ${JSON.stringify(say)}`);
  const at = await page.locator('.pcb-host svg text[data-height-limit]').evaluateAll((els) => els.map((e) => `${e.getAttribute('x')},${e.getAttribute('y')}`));
  expect(new Set(at).size === at.length, `the labels of limits that start at one corner are on lines of their own (${at.join(' ')})`);
  await shot('both');

  step('the Layers menu has Keepouts and Height limits, apart: each hides its own');
  await layer('Height limits', false);
  expect((await rects()).join() === 'antenna' && (await labels()).length === 0, `with the height limits off only the keepout is drawn (${(await rects()).join(', ')})`);
  await layer('Height limits', true);
  await layer('Keepouts', false);
  expect((await rects()).join() === [...LIMITS].sort().join() && (await labels()).length === 7, 'with the keepouts off the seven limits and their labels are drawn');
  await shot('keepouts-off');
  await layer('Keepouts', true);
  await layer('Height limits', false);
  await layer('Keepouts', false);
  expect((await rects()).length === 0, 'both off, no zone is drawn');
  await page.locator('.layer-list .link', { hasText: 'Show all' }).click();
  await whenIdle(page);
  expect((await rects()).length === 8, 'Show all brings them back');
  await page.locator('.layers summary').click();

  step('the front: the zones of the top layer, the back dimmed, the board not mirrored');
  await side('front');
  expect((await rects()).join() === [...TOP, 'antenna'].sort().join(), `the top's zones: ${(await rects()).join(', ')}`);
  expect((await labels()).join() === [...TOP].sort().join(), 'and their labels');
  const opacity = (ref) => page.locator(`.pcb-host svg g[data-ref="${ref}"]`).evaluate((e) => getComputedStyle(e).opacity);
  const front = partOf(s, 'front_a');
  const back = partOf(s, 'back_a');
  expect((await opacity(labelOf(front))) === '1' && Number(await opacity(labelOf(back))) < 0.5, 'the part on the back is dimmed, the one on the front is not');
  expect((await page.locator('.pcb-stage').evaluate((e) => getComputedStyle(e).transform)) === 'none', 'the board is drawn as it is');
  await shot('front');

  step('the back: mirrored left to right, the zones of the bottom layer, the front dimmed, every text the right way round');
  await side('back');
  expect((await rects()).join() === [...BOTTOM].sort().join(), `the bottom's zones: ${(await rects()).join(', ')}`);
  expect((await labels()).join() === [...BOTTOM].sort().join(), 'and their labels');
  expect(Number(await opacity(labelOf(front))) < 0.5 && (await opacity(labelOf(back))) === '1', 'the part on the front is dimmed, the one on the back is not');
  const mirrored = await page.locator('.pcb-stage').evaluate((e) => getComputedStyle(e).transform);
  expect(mirrored === 'matrix(-1, 0, 0, 1, 0, 0)', `the board is mirrored (${mirrored})`);
  const middle = async (p) => (await partAt(page, p, BOARD)).centre.x;
  const [xa, xb, xc] = [await middle('front_a'), await middle('back_a'), await middle('front_b')];
  expect(xa > xb && xb > xc, `left to right is turned: front_a (x 10) is right of back_a (x 22) which is right of front_b (x 32): ${xa.toFixed(0)}, ${xb.toFixed(0)}, ${xc.toFixed(0)}`);
  // A text is the right way round when the whole of what is drawn to it, the part's mirror, the page's and its own, does not mirror it.
  const wrong = await page.locator('.pcb-host svg text').evaluateAll((els) => els.filter((e) => getComputedStyle(e).display !== 'none' && (e.textContent ?? '').trim() && e.getScreenCTM().a < 0).map((e) => e.textContent));
  expect(wrong.length === 0, `no text of the board reads mirrored on the screen (${wrong.slice(0, 5).join(' | ')})`);
  const total = await page.locator('.pcb-host svg text').count();
  expect(total > 0, `and there are texts to read (${total})`);
  await shot('back');

  step('a click on the screen picks the part that is there, mirrored or not, and the part list and inspector say back');
  await page.locator('.side-tabs button', { hasText: 'Parts' }).click();
  const tag = page.locator('.part-row', { hasText: labelOf(back) }).locator('[data-testid=side-tag]');
  expect((await tag.textContent()) === 'back', 'the part on the back is marked back in the list');
  expect((await page.locator('[data-testid=side-tag]').count()) === 1, 'and no other part is');
  const c = await partAt(page, 'front_a', BOARD);
  await page.mouse.click(c.centre.x, c.centre.y);
  await page.waitForSelector('[data-testid=part-inspector]');
  const insp = await page.locator('[data-testid=part-inspector]').innerText();
  expect(insp.includes(labelOf(front)) && /Face\s+front/.test(insp), `the pick is front_a, on the front: ${insp.replace(/\n/g, ' | ').slice(0, 120)}`);
  const cb = await partAt(page, 'back_a', BOARD);
  await page.mouse.click(cb.centre.x, cb.centre.y);
  await page.waitForFunction((r) => document.querySelector('[data-testid=part-inspector]')?.innerText.includes(r), labelOf(back));
  expect(/Face\s+back/.test(await page.locator('[data-testid=part-inspector]').innerText()), 'the inspector says Face back');
  await shot('picked-on-the-back');

  step('the key B goes front, back, both; the view stays on what it looked at');
  await page.mouse.click(5, 5);
  const on = () => page.locator('[data-testid^=pcb-side-][aria-checked=true]').getAttribute('data-testid');
  await page.keyboard.press('b');
  await whenIdle(page);
  expect((await on()) === 'pcb-side-both', `from the back, B shows both (${await on()})`);
  expect((await page.locator('.pcb-stage').evaluate((e) => getComputedStyle(e).transform)) === 'none', 'and the board is not mirrored');
  expect((await rects()).length === 8, 'with all the zones');
  await page.keyboard.press('b');
  await whenIdle(page);
  expect((await on()) === 'pcb-side-front', 'B again shows the front');
  await page.keyboard.press('b');
  await whenIdle(page);
  expect((await on()) === 'pcb-side-back', 'and again the back');
  const shown = await page.locator('.pcb-host svg:not(.pcb-overlay)').evaluate((svg) => {
    const box = svg.getBoundingClientRect();
    const host = svg.closest('.pcb-host').getBoundingClientRect();
    return box.right > host.left && box.left < host.right && box.bottom > host.top && box.top < host.bottom;
  });
  expect(shown, 'the board is still in the window after turning it over');
}
