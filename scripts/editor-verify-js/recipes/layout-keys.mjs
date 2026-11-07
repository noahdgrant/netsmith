// Arranging by keyboard (ADR 0061, stage 4): with a part selected the arrow keys nudge its block one grid step (Shift: ten), `r` turns
// it, `m` mirrors it, Backspace or Delete clears its entry. They are commands of the registry (palette, cheat sheet), and a press while
// a write is on its way waits behind it. With nothing selected, or the sheet's anchor, the arrows pan as before.
import { bodyPoint, entries, layoutFile, onGrid, openLedBoard, partStates, viewOf } from '../layout-helpers.mjs';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project }) {
  const layout = layoutFile(project);
  layout.remove();
  await page.setViewportSize({ width: 1400, height: 900 });
  const parts = await openLedBoard(page);
  await page.waitForTimeout(400);
  const states = await partStates(page);
  const anchor = Object.values(states).find((s) => s.anchor).ref;
  const mover = parts.r1 === anchor ? parts.led : parts.r1;
  const canvas = () => page.locator('.schematic-host').click({ position: { x: 6, y: 6 } });
  const waitFile = async (test, what) => {
    const until = Date.now() + 20000;
    while (Date.now() < until) {
      if (test(layout.text())) return layout.text();
      await page.waitForTimeout(100);
    }
    throw new Error(`expected ${what}; the file is ${JSON.stringify(layout.text())}`);
  };
  const settle = async () => {
    await page.waitForSelector('[data-testid="redrawing"]', { state: 'detached', timeout: 20000 });
    await page.waitForTimeout(400);
  };
  const entry = () => Object.values(entries(layout.text()))[0] ?? {};
  const pick = async (ref) => {
    const p = await bodyPoint(page, ref);
    await page.mouse.click(p.x, p.y);
    await page.waitForSelector('.selchip');
  };

  step('with nothing selected the arrow keys pan the view, and the nudge commands are not offered');
  await canvas();
  const t0 = (await viewOf(page)).transform;
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(150);
  expect((await viewOf(page)).transform !== t0, 'ArrowRight pans');
  expect(!layout.exists(), 'and writes nothing');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp');
  await page.keyboard.press('Control+a');
  await page.keyboard.type('>nudge');
  await page.waitForTimeout(200);
  expect((await page.locator('.qp-row').count()) === 0, 'no "Nudge" row with no part selected');
  await page.keyboard.press('Escape');

  step('the sheet’s anchor is selected: the arrows still pan, but it can be turned');
  await pick(anchor);
  const t1 = (await viewOf(page)).transform;
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(150);
  expect((await viewOf(page)).transform !== t1 && !layout.exists(), 'ArrowDown pans: the anchor is not nudged');
  await page.keyboard.press('Escape');

  step('a part is selected: the palette and the cheat sheet list the keys, with their chords');
  await pick(mover);
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp');
  await page.keyboard.press('Control+a');
  await page.keyboard.type('>');
  await page.waitForTimeout(200);
  const rows = (await page.locator('.qp-row').allInnerTexts()).map((r) => r.replace(/\n/g, ' '));
  for (const title of ['Nudge the selected part left one grid step', 'Nudge the selected part down ten grid steps', 'Rotate the selected part a quarter turn', 'Mirror the selected part']) {
    expect(rows.some((r) => r.includes(title)), `the palette has "${title}"`);
  }
  expect(!rows.some((r) => r.includes('Pan left')), 'and no longer "Pan left": the arrows nudge');
  expect(!rows.some((r) => r.includes("Clear the selected part's layout entry")), 'there is nothing to clear yet');
  await page.keyboard.press('Escape');
  await canvas();
  await pick(mover);
  await page.keyboard.press('?');
  await page.waitForSelector('.cheat[role=dialog]');
  const group = (await page.locator('.cheat section[data-group="Schematic"]').textContent()) ?? '';
  expect(/Nudge the selected part right one grid step/.test(group) && /Rotate the selected part/.test(group) && /Mirror the selected part/.test(group), 'the cheat sheet has them under Schematic');
  await page.keyboard.press('Escape');

  step('ArrowRight nudges one step, Shift+ArrowDown ten: the offset adds up on the grid');
  await canvas();
  await pick(mover);
  const start = (await partStates(page))[mover];
  await page.keyboard.press('ArrowRight');
  await waitFile((t) => t !== null && /dx = 1.27/.test(t), 'dx = 1.27');
  await settle();
  const one = (await partStates(page))[mover];
  expect(one.x > start.x + 1 && Math.abs(one.y - start.y) < 1.5, 'the part moved right by a step');
  expect((await page.locator('.selchip').count()) === 1, 'and is still the pick');
  await page.keyboard.press('Shift+ArrowDown');
  const text = await waitFile((t) => t !== null && /dy = 12.7/.test(t), 'dy = 12.7 (ten steps)');
  await settle();
  const e = entry();
  expect(Number(e.dx) === 1.27 && Number(e.dy) === 12.7 && onGrid(Number(e.dy)), `the entry has both (${JSON.stringify(e)})`);
  expect(text.startsWith('version = 1\n'), 'canonical text');
  await shot('nudged');

  step('two presses in a row: the second waits for the first write, and both are made');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');
  await waitFile((t) => t !== null && /dx = -1.27/.test(t), 'dx = -1.27: one step back, then one more');
  await settle();
  const e2 = entry();
  expect(Number(e2.dx) === -1.27 && Number(e2.dy) === 12.7, `both nudges were made (${JSON.stringify(e2)})`);

  step('r turns a quarter from the orientation drawn, Shift+R the other way, m mirrors; they replace the automatic turn');
  const drawn = (await partStates(page))[mover].orient;
  await page.keyboard.press('r');
  await waitFile((t) => t !== null && /rotate = \d+/.test(t), 'a rotate');
  await settle();
  const turns = Number(drawn.split(' ')[1]);
  expect(Number(entry().rotate) === ((turns + 1) % 4) * 90, `rotate = ${entry().rotate}, a quarter turn from the ${turns * 90} degrees drawn`);
  expect((await partStates(page))[mover].orient === `${drawn.split(' ')[0]} ${(turns + 1) % 4}`, 'and the drawing has it');
  await page.keyboard.press('Shift+R');
  await waitFile((t) => t !== null && Number(entries(t)[Object.keys(entries(t))[0]]?.rotate) === turns * 90, 'rotate back');
  await settle();
  await page.keyboard.press('m');
  await waitFile((t) => t !== null && /mirror = (true|false)/.test(t), 'a mirror');
  await settle();
  expect((await partStates(page))[mover].orient.split(' ')[0] !== drawn.split(' ')[0], 'the part is mirrored in the drawing');
  await shot('turned');

  step('Backspace clears the part’s entry: the block goes back where the drawing puts it');
  await page.keyboard.press('Backspace');
  await waitFile((t) => t !== null && !/\[/.test(t), 'a file with no entry');
  await settle();
  expect(layout.text() === 'version = 1\n', `the file is \`version = 1\` and still there (${JSON.stringify(layout.text())})`);
  const home = (await partStates(page))[mover];
  expect(Math.abs(home.x - start.x) < 1.5 && Math.abs(home.y - start.y) < 1.5 && !home.hinted, 'the part is where it was and wears no outline');
  expect(home.orient === drawn, 'in its automatic orientation');
}
