// The PCB: placed at once, routed on demand, with layers that can be hidden.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  step('open the PCB tab');
  await page.getByText('PCB', { exact: true }).click();
  await page.waitForSelector('.pcb-host svg');
  expect((await page.locator('.pcb-host svg g.l-pads').count()) > 0, 'the board drawn with its pads');
  await shot('placed');

  step('place and route');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  expect(true, 'the board routed');
  await shot('routed');

  step('hide the silkscreen layer');
  await page.locator('.layers summary').click();
  await page.locator('.layer-list label', { hasText: 'Silkscreen' }).locator('input').uncheck();
  expect(await page.locator('.pcb-host .l-silk').first().isHidden(), 'the silkscreen hidden');
  await shot('layers');
}
