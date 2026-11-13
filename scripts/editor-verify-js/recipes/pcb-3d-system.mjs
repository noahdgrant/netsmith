// The system scene in the 3D view (ADR 0110, features/pcb-3d.md): on a board of a system, the 3D
// view's list has the system above its members; choosing it draws every board placed against the
// others by their mates, with a chip that says how many boards and the mated height. A click on a
// part of another member makes that member the page's board (its 2D board and schematic follow)
// and selects the part there, while the 3D view stays on the system. A finding across the boards
// (a header made too tall for the stack) is listed, and a click on it flies the camera to it.
import fs from 'node:fs';
import path from 'node:path';

import { openPcb } from '../pcb-helpers.mjs';

export const fixture = 'two_boards';

const HOST = '[data-testid=pcb-3d]';
const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);
const ready = (page) => page.waitForSelector(`${HOST}[data-ready]`, { timeout: 60000 });
/** Wait until the 3D view draws the system: a part of each member, by `member/path`. */
const systemDrawn = (page) =>
  page.waitForFunction(
    (sel) => {
      const parts = document.querySelector(sel)?.view3d?.parts?.() ?? [];
      return parts.some((p) => p.path.startsWith('led/')) && parts.some((p) => p.path.startsWith('power/'));
    },
    HOST,
    { timeout: 60000 },
  );
/** Where a fixed point of the scene (the base's origin) is on screen: it moves when the camera does. */
const origin = (page) => ask(page, 'toScreen', [0, 0, 0]);

export async function run({ page, url, project, shot, step, expect }) {
  const file = path.join(project, 'src/main.nsm');
  const original = fs.readFileSync(file, 'utf8');
  await page.goto(url);
  await page.waitForSelector('.app');
  await openPcb(page, 'TwoBoards/power');

  step('the 3D view of a member lists its system above its boards');
  await page.getByTestId('pcb-3d-toggle').click();
  await ready(page);
  const options = await page.locator('[data-testid=view-system] option').allInnerTexts();
  expect(options[0] === 'System TwoBoards' && options.includes('TwoBoards/power') && options.includes('TwoBoards/led'), `the list: ${options.join(' | ')}`);
  expect((await ask(page, 'parts')).every((p) => !p.path.includes('/')), 'the board alone first: no part of another member');

  step('choosing the system draws both boards, the LED board on the power board at the mated height');
  await page.selectOption('[data-testid=view-system]', 'system:TwoBoards');
  await systemDrawn(page);
  await page.waitForTimeout(400);
  const chip = await page.getByTestId('system-chip').innerText();
  expect(chip.trim() === 'System TwoBoards: 2 boards, mated at 10.65–11.35 mm', `the chip, with no finding: ${chip}`);
  const drawn = await ask(page, 'sample');
  expect(drawn.drawn > 0.05, `the canvas is drawn (${(drawn.drawn * 100).toFixed(0)} % not the background)`);
  await shot('system-iso');
  await page.getByTestId('view-front').click();
  await page.waitForTimeout(600);
  await shot('system-front');
  await page.locator('[data-testid=view-iso]').click();
  await page.waitForTimeout(600);

  step("a click on the LED board's socket makes the LED board the page's, selects the socket, and stays on the system");
  const at = await ask(page, 'project', 'led/power_in');
  expect(!!at, 'the socket is on screen');
  await page.mouse.move(at.x, at.y);
  await page.mouse.click(at.x, at.y);
  await page.waitForFunction(() => document.querySelector('.boardpick select')?.value === 'TwoBoards/led', null, { timeout: 30000 });
  await ready(page);
  await systemDrawn(page);
  await page.waitForSelector('[data-testid=part-inspector]', { timeout: 30000 });
  const title = await page.locator('[data-testid=part-inspector] h3').innerText();
  expect(title.length > 0, `the inspector shows the socket (${title})`);
  expect((await page.locator('[data-testid=view-system]').inputValue()) === 'system:TwoBoards', 'the 3D view is still on the system');
  await page.mouse.move(5, 5);
  await page.waitForTimeout(400);
  await shot('system-picked');

  step('a header too tall for the stack is a finding across the boards; a click flies the camera to it');
  fs.writeFileSync(file, original.replace('    .power_out: Header1x2\n', '    .power_out: Header1x2 { .height &= 12mm }\n'));
  try {
    await finding(page, shot, expect);
  } finally {
    fs.writeFileSync(file, original);
  }
}

async function finding(page, shot, expect) {
  await page.waitForSelector('[data-testid=system-chip].bad', { timeout: 60000 });
  const bad = await page.getByTestId('system-chip').innerText();
  expect(bad.includes('1 finding'), `the chip: ${bad}`);
  await page.getByTestId('system-chip').click();
  const row = page.locator('[data-testid=system-findings] [data-code=system_collision]');
  await row.waitFor();
  const before = await origin(page);
  await row.click();
  await page.waitForTimeout(1200);
  const after = await origin(page);
  const moved = Math.hypot(after.x - before.x, after.y - before.y);
  expect(moved > 20, `the camera flew to the finding: the base's origin moved ${moved.toFixed(0)} px on screen`);
  await shot('system-finding');
}
