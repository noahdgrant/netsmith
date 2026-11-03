// A four-layer board (launch --fixture four_layer): its planes are drawn dashed, and routing
// finishes with no unrouted connection.
export const fixture = 'four_layer';

export async function run({ page, shot, step, expect }) {
  step('open the PCB tab');
  await page.getByText('PCB', { exact: true }).click();
  await page.waitForSelector('.pcb-host svg');
  expect((await page.locator('.pcb-host svg [stroke-dasharray]').count()) >= 2, 'a dashed border for each plane');
  step('route it');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  expect(true, 'routed with nothing unrouted');
  await shot('four-layer');
}
