// Findings on the PCB canvas: what is wrong with the board is listed with where it is, a click
// zooms to it and marks it, the list follows the edits, and Re-check runs KiCad's check.
import { PART, drag, fresh, openPcb, partAt, scene, settled } from '../pcb-helpers.mjs';

export const fixture = 'outline_and_pours';

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools);
  await openPcb(page);

  step('a clean board has no findings');
  await page.waitForSelector('[data-testid=findings-chip]');
  expect((await page.locator('[data-testid=findings-chip]').innerText()) === 'no findings', 'the chip says no findings');

  step('drag a capacitor onto the regulator: findings appear, with coordinates');
  const cap = await partAt(page, PART.inputCap);
  const reg = await partAt(page, PART.regulator);
  const stamp = cap.scene.layoutStamp;
  await page.mouse.click(cap.centre.x, cap.centre.y);
  await drag(page, cap.centre, { x: reg.centre.x + 3, y: reg.centre.y });
  await settled(page, stamp);
  await page.waitForSelector('[data-testid=findings-chip].bad');
  await page.locator('[data-testid=findings-chip]').click();
  await page.waitForSelector('[data-testid=finding]');
  const rows = await page.locator('[data-testid=finding]').allInnerTexts();
  expect(rows.some((r) => r.includes('overlap') && /\d+\.\d\d, \d+\.\d\d/.test(r)), `an overlap finding with its coordinates (${rows[0].replace(/\n/g, ' | ')})`);
  expect((await page.locator('.pcb-overlay .mark').count()) >= 1, 'the finding is marked on the board');
  await shot('findings');

  step('click a finding: the canvas zooms to it, marks it and selects its parts');
  const zoomBefore = await page.locator('.zoombar span').innerText();
  await page.locator('[data-testid=finding]', { hasText: 'overlap' }).first().click();
  await page.waitForTimeout(300);
  expect((await page.locator('.zoombar span').innerText()) !== zoomBefore, 'the zoom changed');
  expect((await page.locator('.pcb-overlay .mark.on').count()) === 1, 'one finding is highlighted');
  expect((await page.locator('[data-testid=part-inspector] h3').innerText()).match(/^2 PARTS/i), 'the two parts are selected');
  await shot('finding-clicked');

  step('undo the move: the finding goes without Re-check');
  await page.getByRole('button', { name: 'Fit' }).click();
  await page.keyboard.press('Control+z');
  await page.waitForSelector('[data-testid=findings-chip].ok');
  expect((await page.locator('[data-testid=finding]').count()) === 0, 'no finding left');
  await shot('clean-again');

  step('Re-check runs KiCad on an overlapping board and lists what it found');
  await page.keyboard.press('Control+Shift+z');
  await page.waitForSelector('[data-testid=findings-chip].bad');
  await page.locator('[data-testid=findings-chip]').click();
  await page.locator('[data-testid=recheck]').click();
  await page.waitForFunction(() => !document.querySelector('[data-testid=recheck]')?.textContent?.includes('Checking'), null, { timeout: 90000 });
  const note = await page.locator('.findings-head .faint').innerText();
  const s = await scene(page);
  expect(note === 'with KiCad' || note === 'KiCad unavailable', `KiCad's state is shown (${note})`);
  if (note === 'with KiCad') {
    const kicad = await page.locator('[data-testid=finding] .tag', { hasText: 'KiCad' }).count();
    expect(kicad > 0, `KiCad's findings are listed (${kicad})`);
    expect((await page.locator('[data-testid=finding]', { hasText: 'courtyards_overlap' }).count()) === 1, 'KiCad names the courtyard overlap');
  }
  expect(s.parts.length === 12, 'the board is as it was');
  await shot('rechecked');
}
