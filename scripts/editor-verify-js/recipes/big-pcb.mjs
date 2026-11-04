// The PCB of a big board: the board fills the canvas, the nets are in a list at the side, a
// hover on a net lights it, and routing shows its stages and time while the editor stays usable.
export const fixture = 'big';

export async function run({ page, shot, step, expect, section }) {
  step('open the PCB tab');
  await section('pcb');
  await page.waitForSelector('.pcb-host svg', { timeout: 60000 });
  expect((await page.locator('.pcb-host svg .l-key').count()) === 0, 'no key under the board');
  const host = await page.locator('.pcb-host').boundingBox();
  const board = await page.locator('.pcb-host svg .l-outline rect').boundingBox();
  expect(board.height > host.height * 0.6, `the board fills the canvas (${Math.round(board.height)} of ${Math.round(host.height)} px)`);
  await shot('placed');

  step('the nets are listed at the side (the Nets tab); the filter narrows them');
  await page.locator('.side-tabs button', { hasText: 'Nets' }).click();
  const rows = await page.locator('.net-row').count();
  expect(rows >= 30, `a row for each net (${rows})`);
  await page.fill('.side .filter', 'RAIL');
  expect((await page.locator('.net-row').count()) === 8, 'the filter leaves the 8 rails');
  await page.fill('.side .filter', '');

  step('hover a net in the list: it is lit on the board; click picks it');
  await page.locator('.net-row', { hasText: 'RAIL3' }).hover();
  await page.waitForSelector('.pcb-host svg.has-hl');
  expect((await page.locator('.pcb-host svg [data-net="RAIL3"].hl').count()) > 0, 'the net RAIL3 is lit');
  await shot('hover-net');
  await page.locator('.net-row', { hasText: 'RAIL3' }).click();
  await page.mouse.move(5, 5);
  await page.waitForTimeout(200);
  expect((await page.locator('.pcb-host svg.has-hl').count()) === 1, 'the picked net stays lit');

  step('collapse the panel');
  await page.getByRole('button', { name: 'Panel', exact: true }).click();
  expect((await page.locator('.side').count()) === 0, 'the panel is hidden');
  await page.getByRole('button', { name: 'Panel', exact: true }).click();

  step('place and route: stages and a clock show while it runs, and the page stays usable');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.job .job-line');
  const line = await page.locator('.job .job-line').innerText();
  expect(line.length > 0, `a stage is named (${line})`);
  expect(await page.locator('.statusbar .job-item').isVisible(), 'the status bar shows the job');
  expect(await page.getByRole('button', { name: 'Cancel' }).isVisible(), 'a cancel button');
  const status = await page.evaluate(async () => (await fetch('/api/analysis')).status);
  expect(status === 200, 'the server answers while the router runs');
  await section('schematic');
  await page.waitForSelector('.schematic');
  expect(true, 'another tab opens while the router runs');
  await section('pcb');
  await page.waitForSelector('.took', { timeout: 120000 });
  const took = await page.locator('.toolbar .took').innerText();
  expect(/took \d/.test(took), `the time it took (${took})`);
  await shot('routed');
}
