// Findings on the PCB canvas: what is wrong with the board is listed with where it is, a click
// zooms to it and marks it, the list follows the edits, and Re-check runs KiCad's check.
import { BOARD, PART, drag, fresh, openPcb, partAt, scene, settled } from '../pcb-helpers.mjs';

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
  const zoomBefore = await page.locator('.pcb-host .zoombar span').innerText();
  await page.locator('[data-testid=finding]', { hasText: 'overlap' }).first().click();
  await page.waitForTimeout(300);
  expect((await page.locator('.pcb-host .zoombar span').innerText()) !== zoomBefore, 'the zoom changed');
  expect((await page.locator('.pcb-overlay .mark.on').count()) === 1, 'one finding is highlighted');
  expect((await page.locator('[data-testid=part-inspector] h3').innerText()).match(/^2 PARTS/i), 'the two parts are selected');
  await shot('finding-clicked');

  step('undo the move: the finding goes without Re-check');
  await page.getByRole('button', { name: 'Fit' }).click();
  await page.keyboard.press('Control+z');
  await page.waitForSelector('[data-testid=findings-chip].ok');
  expect((await page.locator('[data-testid=finding]').count()) === 0, 'no finding left');
  await shot('clean-again');

  step('Re-check asks KiCad and says what KiCad did, as the server reports it');
  await page.keyboard.press('Control+Shift+z');
  await page.waitForSelector('[data-testid=findings-chip].bad');
  await page.locator('[data-testid=findings-chip]').click();
  // The answer of the check, not the click: the page redraws a moment after it.
  const answered = page.waitForResponse((r) => r.url().includes('/api/pcb/drc') && r.request().method() === 'POST', { timeout: 90000 });
  await page.locator('[data-testid=recheck]').click();
  await answered;
  await page.waitForFunction(() => !document.querySelector('[data-testid=recheck]')?.textContent?.includes('Checking'), null, { timeout: 90000 });
  // What the server says now: KiCad's state for this layout, and whether it found kicad-cli at all.
  const server = await page.evaluate(async (b) => {
    const findings = await (await fetch(`/api/pcb/findings?board=${encodeURIComponent(b)}`)).json();
    const project = await (await fetch('/api/project')).json();
    return { state: findings.kicad, note: findings.kicadNote, found: project.kicad, kicad: findings.findings.filter((f) => f.source === 'kicad').length };
  }, BOARD);
  console.log(`    server: kicad state ${server.state}, kicad-cli found ${server.found}, ${server.kicad} KiCad finding(s)`);
  await page.waitForFunction((state) => document.querySelector('[data-testid=kicad-state]')?.getAttribute('data-state') === state, server.state, { timeout: 10000 });
  const shown = await page.locator('[data-testid=kicad-state]').innerText();
  // Every wording the page has, by the state the server reports (`pcb/kicad.ts`): a check that ran,
  // one that failed, or one not asked; and when kicad-cli is not found, the line that says how to fix it.
  const NOT_FOUND = 'KiCad not found: set KICAD_CLI or install KiCad';
  const WORDS = { ran: ['with KiCad'], unavailable: ['KiCad unavailable', NOT_FOUND], not_run: ['KiCad not run', NOT_FOUND] };
  expect(!!WORDS[server.state], `the server's state is one the page knows (${server.state})`);
  expect(WORDS[server.state].includes(shown), `the page's line matches the server's state ${server.state} (${shown})`);
  if (!server.found && server.state !== 'ran') expect(shown === NOT_FOUND, `with no kicad-cli the line says how to fix it (${shown})`);
  if (server.state === 'ran') {
    const kicad = await page.locator('[data-testid=finding] .tag', { hasText: 'KiCad' }).count();
    expect(kicad > 0 && kicad === server.kicad, `KiCad's findings are listed (${kicad})`);
    expect((await page.locator('[data-testid=finding]', { hasText: 'courtyards_overlap' }).count()) === 1, 'KiCad names the courtyard overlap');
  } else {
    expect((await page.locator('.note-line.bad').innerText()).length > 0, 'the note says why KiCad did not run');
  }
  const s = await scene(page);
  expect(s.parts.length === 12, 'the board is as it was');
  await shot('rechecked');
}
