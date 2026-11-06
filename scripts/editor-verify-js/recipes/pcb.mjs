// The PCB: placed at once, routed on demand, with layers that can be hidden.
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  step('open the PCB tab');
  await section('pcb');
  await page.waitForSelector('.pcb-host svg');
  expect((await page.locator('.pcb-host svg g.l-pads').count()) > 0, 'the board drawn with its pads');
  await shot('placed');

  step('a small board opens fitted, at most 300 %; its references are small beside the pads, in screen pixels at any zoom; the toolbar has the Changes switch');
  const zoom = Number((await page.locator('.pcb-host .zoombar span').innerText()).replace('%', ''));
  expect(zoom > 0 && zoom <= 300, `the zoom of the first look is at most 300 % (${zoom} %)`);
  const refs = await page.evaluate(() => [...document.querySelectorAll('.pcb-host svg .l-refs text')].map((t) => t.getBoundingClientRect().height));
  expect(refs.length > 0 && refs.every((h) => h >= 8 && h <= 20), `a reference is between 8 and 20 screen pixels high (${refs.map((h) => Math.round(h)).join(', ')})`);
  expect((await page.locator('.pcb-bar [data-testid="changes-switch"]').count()) === 1, 'the PCB toolbar has the Changes switch, as the other views do');
  await page.mouse.move(500, 400);
  await page.mouse.wheel(0, -600);
  await page.waitForTimeout(300);
  const zoomed = await page.evaluate(() => [...document.querySelectorAll('.pcb-host svg .l-refs text')].map((t) => t.getBoundingClientRect().height));
  expect(zoomed.every((h) => h >= 8 && h <= 20), `and after a zoom they are still small on the screen (${zoomed.map((h) => Math.round(h)).join(', ')})`);
  await page.locator('.pcb-host .zoombar button', { hasText: 'Fit' }).click();

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
