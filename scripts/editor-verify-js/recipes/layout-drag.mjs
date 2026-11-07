// Arranging the schematic by hand (ADR 0061, stage 4): a drag moves a part's block on the 1.27 mm grid, with a ghost where it was
// and a readout beside the pointer; the drop writes `<stem>.sch.toml` through `PUT /api/schematic-hints`; Esc cancels; Alt pins;
// a press that does not move is a click; the sheet's anchor cannot be dragged; a drop the drawing refuses is put back and said;
// the selection and the zoom survive the redraw. Parts are found by their path, never by designator.
import { BOARD, DEFINITION, bodyPoint, entries, layoutFile, onGrid, openLedBoard, partStates, pixelsPerStep, viewOf } from '../layout-helpers.mjs';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, allowConsole }) {
  const layout = layoutFile(project);
  layout.remove();
  await page.setViewportSize({ width: 1400, height: 900 });
  const parts = await openLedBoard(page);
  const [resistor, led, socket] = [parts.r1, parts.led, parts.power_in];
  expect(!!resistor && !!led && !!socket, `the LED board has r1, led and power_in (${JSON.stringify(parts)})`);
  await page.waitForTimeout(400);
  const waitFile = (test, what) => page.waitForFunction(() => true).then(async () => {
    const until = Date.now() + 20000;
    while (Date.now() < until) {
      if (test(layout.text())) return layout.text();
      await page.waitForTimeout(100);
    }
    throw new Error(`expected ${what}; the file is ${JSON.stringify(layout.text())}`);
  });
  /** Press on a part's body, move to `by` steps from there in `moves` steps of the pointer, and leave the button down. */
  const grab = async (ref, sx, sy, { alt = false } = {}) => {
    const at = await bodyPoint(page, ref);
    expect(at?.hit === ref, `the middle of ${ref}'s body is ${ref} (${at?.hit})`);
    const step1 = await pixelsPerStep(page);
    if (alt) await page.keyboard.down('Alt');
    await page.mouse.move(at.x, at.y);
    await page.mouse.down();
    await page.mouse.move(at.x + (sx * step1) / 2, at.y + (sy * step1) / 2, { steps: 4 });
    await page.mouse.move(at.x + sx * step1, at.y + sy * step1, { steps: 6 });
    return { at, step1 };
  };
  const release = async ({ alt = false } = {}) => {
    await page.mouse.up();
    if (alt) await page.keyboard.up('Alt');
  };

  step('the drawing says which part is the sheet’s anchor, and which can be moved');
  const before = await partStates(page);
  const anchors = Object.values(before).filter((s) => s.anchor);
  expect(anchors.length === 1, `one part is the reference part of the sheet (${anchors.map((s) => s.ref)})`);
  const anchor = anchors[0].ref;
  expect(!anchors[0].movable && /anchor: drag a neighbour/.test(anchors[0].title), `its tooltip says why it is fixed (${JSON.stringify(anchors[0].title)})`);
  const movable = Object.values(before).filter((s) => !s.anchor);
  expect(movable.length === 2 && movable.every((s) => s.movable && s.origin && /^[01] [0-3]$/.test(s.orient)), 'the others can be dragged, and say their origin and turn');
  expect(!Object.values(before).some((s) => s.hinted), 'no part wears the hand-arranged outline: there is no file');
  // The two parts that can move: one is dragged, the other stays to measure from.
  const [mover, other] = [movable[0].ref, movable[1].ref];
  await page.waitForTimeout(200);
  await shot('before');

  step('a drag shows the block moving on the grid, a ghost where it was and a readout; nothing is written while the pointer moves');
  const view0 = await viewOf(page);
  const moverAt0 = before[mover];
  const { step1 } = await grab(mover, 5, 3);
  await page.waitForSelector('.drag-ghost');
  const live = await page.evaluate((r) => {
    const g = document.querySelector(`svg.schematic g.part.dragging[data-ref="${r}"]`);
    const m = /translate\(([-\d.]+)px, ([-\d.]+)px\)/.exec(g?.style.transform ?? '');
    return { transform: g?.style.transform, x: m && Number(m[1]), y: m && Number(m[2]), ghosts: document.querySelectorAll('svg.schematic .drag-ghost').length, readout: document.querySelector('[data-testid="drag-readout"]')?.textContent ?? '', ghostNamed: !!document.querySelector('.drag-ghost [data-ref], .drag-ghost[data-ref]') };
  }, mover);
  expect(live.x !== null && onGrid(live.x) && onGrid(live.y) && live.x > 0, `the block carries a whole number of grid steps (${live.transform})`);
  expect(live.ghosts === 1 && !live.ghostNamed, 'one ghost stays where it was, and names no part');
  expect(/dx \+\d/.test(live.readout) && /mm/.test(live.readout) && !/pin/.test(live.readout), `the readout says the offset (${live.readout})`);
  expect(!layout.exists(), 'nothing is written while the button is down');
  await shot('dragging');

  step('Esc cancels: the block and the readout go, nothing is written, and Esc does not clear the pick');
  await page.keyboard.press('Escape');
  await release();
  await page.waitForTimeout(300);
  const after = await page.evaluate(() => ({ ghosts: document.querySelectorAll('.drag-ghost').length, dragging: document.querySelectorAll('.dragging').length, readout: !!document.querySelector('[data-testid="drag-readout"]'), moved: document.querySelector('svg.schematic.is-dragging') !== null }));
  expect(after.ghosts === 0 && after.dragging === 0 && !after.readout && !after.moved, 'ghost, lift and readout are gone');
  expect(!layout.exists(), 'and no file was written');
  const back = await partStates(page);
  expect(Math.abs(back[mover].x - moverAt0.x) < 1 && Math.abs(back[mover].y - moverAt0.y) < 1, 'the part is where it was');
  expect((await page.locator('.selchip').count()) === 0, 'no click happened: nothing is picked');

  step('a press that does not move is a click as before: it picks the part and writes nothing');
  const click = await bodyPoint(page, mover);
  await page.mouse.click(click.x, click.y);
  await page.waitForSelector('.selchip');
  expect((await page.locator('.selchip').innerText()).includes(mover), `the chip names ${mover}`);
  expect(!layout.exists(), 'no file');
  await page.keyboard.press('Escape');

  step('a drop writes the offset for the part, the page redraws, and the pick and the zoom survive');
  const stagger = await partStates(page);
  const gap0 = { x: stagger[mover].x - stagger[other].x, y: stagger[mover].y - stagger[other].y };
  await grab(mover, 5, 3);
  await release();
  const text = await waitFile((t) => t !== null && /dx/.test(t), 'the file has an offset');
  const written = entries(text);
  const key = Object.keys(written).find((k) => k.startsWith(`${DEFINITION}.`));
  expect(!!key && onGrid(Number(written[key].dx)) && onGrid(Number(written[key].dy)), `the entry ${key} is on the grid (${JSON.stringify(written[key])})`);
  expect(Number(written[key].dx) > 0 && Number(written[key].dy) > 0, 'to the right and down, as dragged');
  expect(text.startsWith('version = 1\n'), 'the file is the canonical text');
  await page.waitForFunction((ref) => {
    const hint = document.querySelector(`svg.schematic g.part[data-ref="${ref}"]`)?.previousElementSibling?.previousElementSibling?.previousElementSibling;
    return hint?.classList.contains('on');
  }, mover, { timeout: 20000 });
  await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
  await page.waitForTimeout(500);
  const moved = await partStates(page);
  const gap1 = { x: moved[mover].x - moved[other].x, y: moved[mover].y - moved[other].y };
  expect(gap1.x > gap0.x + step1 && gap1.y > gap0.y + step1, `the part is further right and down of ${other} (${JSON.stringify(gap0)} to ${JSON.stringify(gap1)})`);
  expect(moved[mover].hinted, 'it wears the hand-arranged outline');
  expect((await page.locator('.selchip').innerText()).includes(mover), 'it is the pick, so the keys that follow are about it');
  expect((await viewOf(page)).zoom === view0.zoom, `the zoom is what it was (${view0.zoom})`);
  expect((await page.locator('.drag-ghost').count()) === 0, 'no ghost is left');
  await shot('moved');

  step('Alt pins: the entry is a place from the sheet’s reference part, not an offset');
  await page.keyboard.press('Escape');
  const { at: pinAt } = await grab(other, 0, 4, { alt: true });
  const readout = await page.locator('[data-testid="drag-readout"]').innerText();
  expect(/pin/.test(readout), `the readout says it is a pin (${readout})`);
  await release({ alt: true });
  const pinned = await waitFile((t) => t !== null && /pin = \[/.test(t), 'a pin');
  const pe = entries(pinned);
  const pinKey = Object.keys(pe).find((k) => pe[k].pin);
  const pin = JSON.parse(pe[pinKey].pin);
  expect(pin.length === 2 && onGrid(pin[0]) && onGrid(pin[1]) && !('dx' in pe[pinKey]), `${pinKey} = ${pe[pinKey].pin}, on the grid, and no offset beside it`);
  expect(pinned.includes(`[${DEFINITION}.parts.`) && Object.keys(pe).length === 2, 'the first entry is kept');
  await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
  await page.waitForTimeout(400);

  step('the sheet’s anchor is not dragged: the press pans, as before, and writes nothing');
  const fileNow = layout.text();
  const a0 = await partStates(page);
  const t0 = (await viewOf(page)).transform;
  const anchorAt = await bodyPoint(page, anchor);
  await page.mouse.move(anchorAt.x, anchorAt.y);
  await page.mouse.down();
  await page.mouse.move(anchorAt.x + 40, anchorAt.y + 25, { steps: 6 });
  expect((await page.locator('.drag-ghost').count()) === 0, 'no ghost');
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect((await viewOf(page)).transform !== t0, 'the canvas panned');
  expect(layout.text() === fileNow, 'the file is as it was');
  expect(Object.keys(a0).includes(anchor), 'the anchor is still drawn');

  step('a drop the drawing refuses is put back, and the toast names the reason');
  await page.keyboard.press('f');
  await page.waitForTimeout(200);
  const quiet = layout.text();
  const refuseFrom = (await partStates(page))[mover];
  const onto = (await partStates(page))[anchor];
  const per = await pixelsPerStep(page);
  // Drop the part's block on the body of the sheet's anchor.
  await grab(mover, Math.round((onto.x - refuseFrom.x) / per), Math.round((onto.y - refuseFrom.y) / per));
  await release();
  await page.waitForSelector('.toast', { timeout: 20000 });
  const toastText = await page.locator('.toast').first().innerText();
  expect(/cannot put it there/.test(toastText) && /lie on|outside|rule/.test(toastText) && /back where it was/.test(toastText), `the toast names the reason (${toastText.replace(/\n/g, ' ')})`);
  await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
  await page.waitForTimeout(500);
  expect(layout.text() === quiet, 'the file is as it was before the drop');
  const home = await partStates(page);
  expect(Math.abs(home[mover].x - refuseFrom.x) < 2 && Math.abs(home[mover].y - refuseFrom.y) < 2, 'and the block is where it was');
  await shot('refused');

  step('a file changed by another program meanwhile is not overwritten: the page says so, reads it again, and the drop is lost');
  allowConsole(/409/);
  const raced = layout.text();
  const alt = raced.replace(/^version = 1\n/, 'version = 1\n\n[Spare.parts.x]\ndx = 1.27\ndy = 0\n');
  const at2 = await bodyPoint(page, other);
  await page.mouse.move(at2.x, at2.y);
  await page.mouse.down();
  await page.mouse.move(at2.x + per * 3, at2.y + per * 2, { steps: 6 });
  layout.write(alt);
  await page.mouse.up();
  await page.waitForSelector('.toast', { timeout: 20000 });
  const stale = (await page.locator('.toast').allInnerTexts()).join(' ').replace(/\n/g, ' ');
  expect(/changed on disk/.test(stale), `the toast says the file changed (${stale})`);
  await page.waitForTimeout(800);
  expect(layout.text() === alt, 'the other program’s file is still there, untouched by the page');
  layout.remove();
}
