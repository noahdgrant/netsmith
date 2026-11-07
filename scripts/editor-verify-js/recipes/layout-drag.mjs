// Arranging the schematic by hand (ADR 0061, stage 4): a drag moves a part's block on the 1.27 mm grid, with a ghost where it was
// and a readout beside the pointer; the drop writes `<stem>.sch.toml` through `PUT /api/schematic-hints`; Esc cancels; Alt pins;
// a press that does not move is a click; the sheet's reference part cannot be dragged; a drop the drawing refuses, or that moves
// nothing, is put back and said; the selection and the zoom survive the redraw. Parts are found by their path, never by designator.
import { LED, POWER, bodyPoint, entries, layoutFile, onGrid, openBoard, partStates, pixelsPerStep, viewOf } from '../layout-helpers.mjs';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, allowConsole }) {
  const layout = layoutFile(project);
  layout.remove();
  await page.setViewportSize({ width: 1400, height: 900 });
  const parts = await openBoard(page, POWER);
  const [input, output, regulator, header] = [parts['stage.input_cap'], parts['stage.output_cap'], parts['stage.regulator'], parts.power_out];
  expect(!!input && !!output && !!regulator && !!header, `the power board has its capacitors, regulator and headers (${JSON.stringify(parts)})`);
  await page.waitForTimeout(400);
  const waitFile = async (test, what) => {
    const until = Date.now() + 20000;
    while (Date.now() < until) {
      if (test(layout.text())) return layout.text();
      await page.waitForTimeout(100);
    }
    throw new Error(`expected ${what}; the file is ${JSON.stringify(layout.text())}`);
  };
  /** The drawing has come back from a write: `ref` is somewhere else on the screen than `from`, and no redraw is on. */
  const redrawn = async (ref, from) => {
    await page.waitForFunction(([r, x, y]) => {
      const b = document.querySelector(`svg.schematic g.part[data-ref="${r}"]`)?.getBoundingClientRect();
      return b && (Math.abs(b.left + b.width / 2 - x) > 3 || Math.abs(b.top + b.height / 2 - y) > 3);
    }, [ref, from.x, from.y], { timeout: 20000 });
    await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
    await page.waitForTimeout(500);
  };
  /** Press on a part's body and move it `sx`, `sy` grid steps, leaving the button down. */
  const grab = async (ref, sx, sy, { alt = false } = {}) => {
    const at = await bodyPoint(page, ref);
    expect(at?.hit === ref, `the middle of ${ref}'s body is ${ref} (${at?.hit})`);
    const per = await pixelsPerStep(page);
    if (alt) await page.keyboard.down('Alt');
    await page.mouse.move(at.x, at.y);
    await page.mouse.down();
    await page.mouse.move(at.x + (sx * per) / 2, at.y + (sy * per) / 2, { steps: 4 });
    await page.mouse.move(at.x + sx * per, at.y + sy * per, { steps: 6 });
    return { at, per };
  };
  const release = async ({ alt = false } = {}) => {
    await page.mouse.up();
    if (alt) await page.keyboard.up('Alt');
  };

  step('the drawing says which part is the reference part of each sheet, and which can be moved');
  const before = await partStates(page);
  const refs = Object.values(before).filter((s) => s.reference);
  expect(refs.length === 2 && refs.some((s) => s.ref === regulator), `a reference part for the module and one for the board's own parts (${refs.map((s) => s.ref)})`);
  const stageRef = refs.find((s) => s.ref === regulator);
  expect(!stageRef.movable && /reference part: drag a neighbour/.test(stageRef.title), `its tooltip says why it is fixed (${JSON.stringify(stageRef.title)})`);
  const movable = Object.values(before).filter((s) => !s.reference);
  expect(movable.length === 3 && movable.every((s) => s.movable && s.origin && /^[01] [0-3]$/.test(s.orient)), 'the others can be dragged, and say their origin and turn');
  expect(!Object.values(before).some((s) => s.hinted), 'no part wears the hand-arranged outline: there is no file');
  await page.waitForTimeout(200);
  await shot('before');

  step('a drag shows the block moving on the grid, a ghost where it was and a readout; nothing is written while the pointer moves');
  const view0 = await viewOf(page);
  const start = before[input];
  const { per } = await grab(input, 2, 5);
  await page.waitForSelector('.drag-ghost');
  const live = await page.evaluate((r) => {
    const g = document.querySelector(`svg.schematic g.part.dragging[data-ref="${r}"]`);
    const m = /translate\(([-\d.]+)px, ([-\d.]+)px\)/.exec(g?.style.transform ?? '');
    return { transform: g?.style.transform, x: m && Number(m[1]), y: m && Number(m[2]), ghosts: document.querySelectorAll('svg.schematic .drag-ghost').length, readout: document.querySelector('[data-testid="drag-readout"]')?.textContent ?? '', ghostNamed: !!document.querySelector('.drag-ghost [data-ref], .drag-ghost[data-ref]') };
  }, input);
  expect(live.x !== null && onGrid(live.x) && onGrid(live.y) && live.x > 0, `the block carries a whole number of grid steps (${live.transform})`);
  expect(live.ghosts === 1 && !live.ghostNamed, 'one ghost stays where it was, and names no part');
  expect(/dx \+\d/.test(live.readout) && /mm/.test(live.readout) && !/pin/.test(live.readout), `the readout says the offset (${live.readout})`);
  expect(!layout.exists(), 'nothing is written while the button is down');
  await shot('dragging');

  step('Esc cancels: the block and the readout go, nothing is written, and Esc does not clear the pick');
  await page.keyboard.press('Escape');
  await release();
  await page.waitForTimeout(300);
  const after = await page.evaluate(() => ({ ghosts: document.querySelectorAll('.drag-ghost').length, dragging: document.querySelectorAll('.dragging').length, readout: !!document.querySelector('[data-testid="drag-readout"]'), dragClass: document.querySelector('svg.schematic.is-dragging') !== null }));
  expect(after.ghosts === 0 && after.dragging === 0 && !after.readout && !after.dragClass, 'ghost, lift and readout are gone');
  expect(!layout.exists(), 'and no file was written');
  const back = await partStates(page);
  expect(Math.abs(back[input].x - start.x) < 1 && Math.abs(back[input].y - start.y) < 1, 'the part is where it was');

  step('a press that does not move is a click as before: it picks the part and writes nothing');
  const click = await bodyPoint(page, input);
  await page.mouse.click(click.x, click.y);
  await page.waitForSelector('.selchip');
  expect((await page.locator('.selchip').innerText()).includes(input), `the chip names ${input}`);
  expect(!layout.exists(), 'no file');
  await page.keyboard.press('Escape');

  step('a drop writes the offset for the part in its module’s table, the page redraws, and the pick and the zoom survive');
  const rest = await partStates(page);
  const gap0 = { x: rest[input].x - rest[regulator].x, y: rest[input].y - rest[regulator].y };
  await grab(input, 2, 5);
  await release();
  const text = await waitFile((t) => t !== null && /dx/.test(t), 'the file has an offset');
  const written = entries(text);
  const key = 'PowerBoard.sheets.stage.parts.input_cap';
  expect(!!written[key] && onGrid(Number(written[key].dx)) && onGrid(Number(written[key].dy)), `the entry ${key} is on the grid (${JSON.stringify(written)})`);
  expect(Number(written[key].dx) === 2.54 && Number(written[key].dy) === 6.35, 'two steps right and five down, as dragged');
  expect(text.startsWith('version = 1\n'), 'the file is the canonical text');
  await redrawn(input, rest[input]);
  const moved = await partStates(page);
  const gap1 = { x: moved[input].x - moved[regulator].x, y: moved[input].y - moved[regulator].y };
  expect(gap1.x > gap0.x + per && gap1.y > gap0.y + per, `the capacitor is further right and down of the regulator (${JSON.stringify(gap0)} to ${JSON.stringify(gap1)})`);
  expect(moved[input].hinted && !moved[output].hinted, 'it wears the hand-arranged outline, and the other capacitor does not');
  expect((await page.locator('.selchip').innerText()).includes(input), 'it is the pick, so the keys that follow are about it');
  expect((await viewOf(page)).zoom === view0.zoom, `the zoom is what it was (${view0.zoom})`);
  expect(Math.abs(moved[regulator].x - rest[regulator].x) < 2 && Math.abs(moved[regulator].y - rest[regulator].y) < 2, 'the reference part is where it was on the screen');
  expect((await page.locator('.drag-ghost').count()) === 0, 'no ghost is left');
  await shot('moved');

  step('Alt pins: the entry is a place from the sheet’s reference part, not an offset');
  await page.keyboard.press('Escape');
  const pinFrom = (await partStates(page))[output];
  await grab(output, 0, 4, { alt: true });
  const readout = await page.locator('[data-testid="drag-readout"]').innerText();
  expect(/pin/.test(readout), `the readout says it is a pin (${readout})`);
  await release({ alt: true });
  const pinned = await waitFile((t) => t !== null && /pin = \[/.test(t), 'a pin');
  const pe = entries(pinned);
  const pinKey = 'PowerBoard.sheets.stage.parts.output_cap';
  const pin = JSON.parse(pe[pinKey].pin);
  expect(pin.length === 2 && onGrid(pin[0]) && onGrid(pin[1]) && !('dx' in pe[pinKey]), `${pinKey} = ${pe[pinKey].pin}, on the grid, and no offset beside it`);
  expect(!!pe['PowerBoard.sheets.stage.parts.input_cap'], 'the first entry is kept');
  await redrawn(output, pinFrom);

  step('the reference part is not dragged: the press pans, as before, and writes nothing');
  const fileNow = layout.text();
  const t0 = (await viewOf(page)).transform;
  const refAt = await bodyPoint(page, regulator);
  await page.mouse.move(refAt.x, refAt.y);
  await page.mouse.down();
  await page.mouse.move(refAt.x + 40, refAt.y + 25, { steps: 6 });
  expect((await page.locator('.drag-ghost').count()) === 0, 'no ghost');
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect((await viewOf(page)).transform !== t0, 'the canvas panned');
  expect(layout.text() === fileNow, 'the file is as it was');

  step('a note can be dragged too: it writes its offset under `notes`, and a click on it still opens the source');
  await page.keyboard.press('f');
  await page.waitForTimeout(250);
  const notePoint = () =>
    page.evaluate(() => {
      const n = [...document.querySelectorAll('svg.schematic g.note.note-doc')].find((e) => e.getClientRects().length);
      const f = n.querySelector('.note-frame').getBoundingClientRect();
      return { n: n.getAttribute('data-n'), x: f.left + f.width / 2, y: f.top + f.height / 2 };
    });
  const note0 = await notePoint();
  const nper = await pixelsPerStep(page);
  const noteKeyBefore = Object.keys(entries(layout.text())).join('|');
  await page.mouse.move(note0.x, note0.y);
  await page.mouse.down();
  await page.mouse.move(note0.x - nper * 4, note0.y, { steps: 8 });
  expect((await page.locator('.drag-ghost').count()) === 1 && /note/.test(await page.locator('[data-testid="drag-readout"]').innerText()), 'a ghost and a readout, for the note');
  await page.mouse.up();
  const withNote = await waitFile((t) => t !== null && /\.notes\./.test(t), 'a note entry');
  const ne = Object.entries(entries(withNote)).find(([k]) => /\.notes\./.test(k));
  expect(!!ne && Number(ne[1].dx) === -5.08 && Number(ne[1].dy) === 0, `the note is moved four steps left (${JSON.stringify(ne)})`);
  expect(noteKeyBefore.split('|').every((k) => k in entries(withNote)), 'the part entries are kept');
  await page.waitForFunction(([n, x, y]) => {
    const f = document.querySelector(`svg.schematic g.note[data-n="${n}"] .note-frame`)?.getBoundingClientRect();
    return f && (Math.abs(f.left + f.width / 2 - x) > 3 || Math.abs(f.top + f.height / 2 - y) > 3);
  }, [note0.n, note0.x, note0.y], { timeout: 20000 });
  await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
  await page.waitForTimeout(500);
  await shot('note-moved');
  const note1 = await notePoint();
  await page.mouse.click(note1.x, note1.y);
  await page.waitForSelector('.cm-content', { timeout: 10000 });
  expect((await page.locator('.cm-content').count()) === 1, 'a click on the note opens the source, as before');
  await page.locator('.pane-strip [data-section="schematic"]').first().click();
  await page.waitForSelector('svg.schematic .part[data-ref]');
  expect(layout.text() === withNote, 'and writes nothing');

  step('a drop the drawing refuses is put back, and the toast names the reason');
  await page.keyboard.press('f');
  await page.waitForTimeout(200);
  const quiet = layout.text();
  const from = (await partStates(page))[input];
  const onto = (await partStates(page))[regulator];
  const step1 = await pixelsPerStep(page);
  // Drop the capacitor's block on the body of the regulator.
  await grab(input, Math.round((onto.x - from.x) / step1), Math.round((onto.y - from.y) / step1));
  await release();
  await page.waitForSelector('.toast >> text=cannot put it there', { timeout: 20000 });
  const toastText = (await page.locator('.toast', { hasText: 'cannot put it there' }).first().innerText()).replace(/\n/g, ' ');
  expect(/cannot put it there/.test(toastText) && /lie on|outside|rule/.test(toastText) && /back where it was/.test(toastText), `the toast names the reason (${toastText})`);
  await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
  await page.waitForTimeout(600);
  expect(layout.text() === quiet, 'the file is as it was before the drop');
  const home = (await partStates(page))[input];
  expect(Math.abs(home.x - from.x) < 2 && Math.abs(home.y - from.y) < 2, 'and the block is where it was');
  await shot('refused');

  step('a file changed by another program meanwhile is not overwritten: the page says so and the drop is lost');
  allowConsole(/409/);
  const raced = layout.text();
  const alt = raced.replace(/^version = 1\n/, 'version = 1\n\n[Spare.parts.x]\ndx = 1.27\ndy = 0\n');
  const at2 = await bodyPoint(page, input);
  await page.mouse.move(at2.x, at2.y);
  await page.mouse.down();
  await page.mouse.move(at2.x + step1 * 3, at2.y + step1 * 2, { steps: 6 });
  layout.write(alt);
  await page.mouse.up();
  await page.waitForSelector('.toast >> text=changed on disk', { timeout: 20000 });
  await page.waitForTimeout(800);
  expect(layout.text() === alt, 'the other program’s file is still there, untouched by the page');
  layout.remove();

  step('a block that holds the sheet’s reference part moves nothing: it is put back, and the file the drop made goes');
  await page.waitForTimeout(600);
  await openBoard(page, LED);
  await page.waitForTimeout(600);
  expect(!layout.exists(), 'there is no file');
  const led = await partStates(page);
  const connector = Object.values(led).find((s) => !s.reference);
  const ledAt = connector.ref;
  await grab(ledAt, 5, 3);
  await release();
  await page.waitForSelector('.toast >> text=nothing was arranged', { timeout: 20000 });
  const inert = (await page.locator('.toast', { hasText: 'nothing was arranged' }).first().innerText()).replace(/\n/g, ' ');
  expect(/nothing was arranged/.test(inert) && /reference part/.test(inert) && /back where it was/.test(inert), `the toast says it (${inert})`);
  await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
  await page.waitForTimeout(1200);
  expect(!layout.exists(), 'the file the drop made is gone: it was not there before');
  await shot('nothing-arranged');
}
