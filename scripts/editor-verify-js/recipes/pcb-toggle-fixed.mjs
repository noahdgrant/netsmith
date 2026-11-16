// The 2D | 3D switch of the PCB toolbar stays where it is (issue #784): it is the first thing of the bar, so the status
// chips after a route, the Side and Tool switches and the tool's options (which the 3D view hides or shows) never move it.
import { whenIdle } from '../settle.mjs';
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect }) {
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await page.locator('.pane-strip [data-section="pcb"]').first().click();
  await page.waitForSelector('.pcb-overlay');
  await whenIdle(page);
  const at = async () => {
    const b = await page.locator('.pcb-bar [aria-label="Drawing"]').boundingBox();
    return `${b.x.toFixed(1)},${b.y.toFixed(1)} ${b.width.toFixed(1)}x${b.height.toFixed(1)}`;
  };
  const first = await at();
  const bar = await page.locator('.pcb-bar').boundingBox();
  expect(Number(first.split(',')[0]) < bar.x + 40, `the switch is at the left end of the bar (${first})`);
  await shot('2d-placed');

  step('a route changes the chips of the bar, the tools change its options: the switch stays');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  await whenIdle(page);
  expect((await at()) === first, `after a route the switch is at ${first} (${await at()})`);
  await page.locator('[data-testid=tool-keepout]').click();
  await whenIdle(page);
  expect((await at()) === first, `with the keepout tool (its options shown) it is at ${first} (${await at()})`);
  await page.locator('[data-testid=pcb-side-back]').click();
  await whenIdle(page);
  expect((await at()) === first, `on the back side it is at ${first} (${await at()})`);

  step('3D hides the Side and Tool switches and the grid: the switch stays');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector('[data-testid=pcb-3d][data-ready]');
  await whenIdle(page);
  expect((await page.locator('[data-testid=tool-keepout]').count()) === 0, 'the Tool switch is gone in 3D');
  expect((await at()) === first, `in 3D it is at ${first} (${await at()})`);
  await shot('3d');

  step('and back in 2D it is where it was');
  await page.getByTestId('pcb-2d').click();
  await page.waitForSelector('.pcb-overlay');
  await whenIdle(page);
  expect((await at()) === first, `back in 2D it is at ${first} (${await at()})`);
}
