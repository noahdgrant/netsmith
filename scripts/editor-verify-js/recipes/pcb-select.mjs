// The PCB canvas: select a part by click and from the list, see its pads and nets, the
// ratsnest lines to it, hover tooltips, the grid, zoom.
import { BOARD, PART, drag, fresh, labelOf, openPcb, partAt, partOf, screenAt } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools);
  await openPcb(page);
  expect((await page.locator('.pcb-host svg g[data-path]').count()) === 12, 'twelve parts drawn, each with its path');
  expect((await page.locator('.part-row').count()) === 12, 'twelve parts in the list');
  await shot('board');

  step('click a part: it is selected, its nets are lit and the inspector shows its pads');
  const c2 = await partAt(page, PART.inputCap);
  const cap = labelOf(c2.part);
  await page.mouse.click(c2.centre.x, c2.centre.y);
  await page.waitForSelector('[data-testid=part-inspector]');
  expect((await page.locator('[data-testid=part-inspector] h3').innerText()).startsWith(cap), `the inspector names ${cap}`);
  expect((await page.locator('.pcb-overlay .outline-picked').count()) === 1, 'one outline on the selected part');
  expect(await page.locator('.pcb-host svg.has-hl').count() === 1, 'the nets of the part are lit and the rest dimmed');
  expect((await page.locator('.pcb-host .l-ratsnest line.sel-rat').count()) >= 1, 'the ratsnest lines to the part are marked');
  expect((await page.locator('.inspector .pads tr').count()) === 2, 'two pads with their nets');
  await shot('selected');

  step('hover a pad: the tooltip says the part, its value and the net');
  const pad = c2.part.pads[0];
  const where = await screenAt(page, pad.x, pad.y);
  await page.mouse.move(where.x, where.y);
  await page.mouse.move(where.x + 1, where.y + 1);
  await page.waitForSelector('[data-testid=tip]');
  const tip = await page.locator('[data-testid=tip]').innerText();
  expect(tip.includes(`${cap}.`) && tip.includes('10u') && tip.includes('net '), `the tooltip names part, value and net (${tip.replace(/\n/g, ' | ')})`);
  await shot('tooltip');

  step('click empty board: the selection goes; pick a part from the list');
  const empty = await screenAt(page, c2.scene.outline.x + c2.scene.outline.width / 2, c2.scene.outline.y + c2.scene.outline.height - 6);
  await page.mouse.click(empty.x, empty.y);
  expect((await page.locator('[data-testid=part-inspector]').count()) === 0, 'nothing selected');
  const header = labelOf(partOf(c2.scene, PART.powerIn));
  await page.locator('.part-row', { hasText: header }).click();
  await page.waitForSelector('[data-testid=part-inspector]');
  expect((await page.locator('[data-testid=part-inspector] h3').innerText()).startsWith(header), `${header} selected from the list`);
  await shot('from-list');

  step('box select: drag a rectangle over the capacitors and what lies between them');
  await page.getByRole('button', { name: 'Fit' }).click();
  await page.waitForTimeout(200);
  const s = c2.scene;
  const c1 = partOf(s, PART.inputCap).courtyard;
  const c2c = partOf(s, PART.outputCap).courtyard;
  const a = await screenAt(page, Math.min(c1.x, c2c.x) - 1.5, Math.min(c1.y, c2c.y) - 2);
  const b = await screenAt(page, Math.max(c1.x + c1.width, c2c.x + c2c.width) + 1.5, Math.max(c1.y + c1.height, c2c.y + c2c.height) + 2);
  await drag(page, a, b);
  await page.waitForSelector('[data-testid=part-inspector]');
  expect((await page.locator('[data-testid=part-inspector] h3').innerText()).match(/^\d+ PARTS/i), 'parts selected by the box');
  await shot('box-select');

  step('wheel zooms, the right button pans');
  const before = await page.locator('.zoombar span').innerText();
  await page.mouse.move(c2.centre.x, c2.centre.y);
  await page.mouse.wheel(0, -600);
  await page.waitForTimeout(200);
  expect((await page.locator('.zoombar span').innerText()) !== before, 'the zoom changed');
  await drag(page, { x: 600, y: 700 }, { x: 500, y: 650 }, { button: 'right' });
  await shot('zoomed');
  expect(BOARD.length > 0, 'done');
}
