// A panel (launch --fixture led_panel): the tree lists it, and a click on it draws the panel in
// the PCB view with its boards named, the cut lines and the panel checks (ADR 0040).
export const fixture = 'led_panel';

export async function run({ page, shot, step, expect }) {
  await page.waitForSelector('.schematic');
  step('the tree lists the panel');
  const row = page.locator('.row.def', { hasText: 'LedPanel' });
  await row.waitFor();
  expect((await row.locator('.badge.k-panel').count()) === 1, 'a panel badge');
  step('click it: the PCB view draws the panel');
  await row.click();
  await page.waitForSelector('[data-testid="panel-preview"] svg');
  const drawing = await page.locator('.panel-drawing').innerHTML();
  expect(drawing.includes('LedTile 6') && drawing.includes('stroke-dasharray'), 'six named cells and the cut lines');
  const header = await page.locator('.panel-preview .graph-title').innerText();
  expect(header.includes('3 x 2 LedTile') && header.includes('130 x 70 mm'), 'the grid and the size: ' + header);
  expect(await page.locator('[data-testid="panel-findings"]').isVisible(), 'the panel checks are listed');
  await shot('panel');
  step('back to the board');
  await page.getByRole('button', { name: /Show the board/ }).click();
  await page.waitForSelector('.pcb-host svg');
  expect((await page.getByRole('button', { name: /Panel LedPanel/ }).count()) === 1, 'a button to draw the panel again');
  await shot('panel-board');
}
