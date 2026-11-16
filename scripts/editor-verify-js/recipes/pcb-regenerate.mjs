// Regenerate one module on the PCB canvas (issue 825): a part of module `a` picked offers
// "Regenerate a"; the proposal is drawn over the board in a second colour with the findings and
// totals before and after, and nothing is written; Another asks for the next seed; Accept writes
// it as one edit; a route after it keeps the copper of the nets of `b` and `c` and routes only the
// rest; Undo puts the old places back with the old copper, with no router. The Modules tab lists the
// board's modules as a tree: a row picks a module's parts, and its button proposes that module.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { whenIdle } from '../settle.mjs';
import { fresh, openPcb, partAt, scene, settled } from '../pcb-helpers.mjs';

export const fixture = 'lights';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lights';

export async function run(tools) {
  const { page, project, shot, step, expect } = tools;
  const layout = path.join(project, 'src/main.pcb.toml');
  await fresh(tools, path.join(here, '../fixtures/lights.pcb.toml'));
  await openPcb(page, BOARD);
  await page.getByRole('button', { name: 'Fit' }).click();
  await whenIdle(page);

  step('route the board first');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  const routed = await scene(page, BOARD);
  expect(routed.routed, 'the board is routed');

  step('a part of `a` picked offers to regenerate `a`');
  const c = await partAt(page, 'a.led', BOARD);
  await page.mouse.click(c.centre.x, c.centre.y);
  const button = page.locator('[data-testid=regenerate]');
  await button.waitFor();
  expect((await button.innerText()).includes('Regenerate a'), `the button: ${await button.innerText()}`);

  step('the proposal is drawn over the board and listed, and nothing is written');
  const fileBefore = fs.readFileSync(layout, 'utf8');
  await button.click();
  await page.waitForSelector('[data-testid=regen-card]');
  await whenIdle(page);
  const ghosts = await page.locator('[data-testid=regen-ghost]').count();
  expect(ghosts === 2, `two ghosts, for a.led and a.r (${ghosts})`);
  const card = await page.locator('[data-testid=regen-card]').innerText();
  expect(card.includes('Routed again') && card.includes('VCC'), `the card names the nets routed again: ${card.replace(/\n/g, ' | ')}`);
  expect(!card.includes('net-b.') && !card.includes('net-c.'), 'and not those of b or c');
  expect((await page.locator('[data-testid=regen-totals] li').count()) === 3, 'three totals before and after');
  expect(fs.readFileSync(layout, 'utf8') === fileBefore, 'the layout file is as it was');
  expect((await scene(page, BOARD)).routed, 'the board is still routed');
  await shot('proposal');

  step('Another asks for the next seed');
  await page.locator('[data-testid=regen-another]').click();
  await page.waitForSelector('[data-testid=regen-card] :text("Seed 1.")');
  await whenIdle(page);
  await shot('another');

  step('Accept writes it as one edit: two entries with source = "regen"');
  const stamp = (await scene(page, BOARD)).layoutStamp;
  await page.locator('[data-testid=regen-accept]').click();
  const edited = await settled(page, stamp, BOARD);
  await whenIdle(page);
  expect((await page.locator('[data-testid=regen-card]').count()) === 0, 'the proposal is gone');
  expect((await page.locator('[data-testid=regen-proposal]').count()) === 0, 'and its drawing');
  const file = fs.readFileSync(layout, 'utf8');
  expect(file.split('source = "regen"').length - 1 === 2, `two regen entries:\n${file}`);
  // Unrouted, unless the server routed this very placement before (a run of this recipe before): then its copper is back at once.
  console.log(`after the edit the board is ${edited.routed ? 'routed (this placement was routed before)' : 'unrouted'}`);

  step('a route after it keeps the copper of the nets of b and c');
  await page.getByRole('button', { name: /Place and route|Route again/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  await whenIdle(page);
  // Asked again, the server gives the same routed board back (it started from the same board), with what that route kept.
  const report = await page.evaluate(async (b) => (await fetch(`/api/pcb/route?board=${encodeURIComponent(b)}`, { method: 'POST', headers: { 'X-Netsmith': '1' } })).json(), BOARD);
  expect(
    report.carried.includes('net-b.led.anode') && report.carried.includes('net-c.led.anode'),
    `kept: ${report.carried.join(', ')}; routed again: ${report.rerouted.join(', ')}`,
  );
  await shot('routed-again');

  step('Undo puts the parts back and the copper they had, with no router');
  await page.keyboard.press('Control+z');
  const undone = await settled(page, (await scene(page, BOARD)).layoutStamp, BOARD);
  await whenIdle(page);
  expect(undone.routed, 'the board is routed at once');
  expect(undone.svg === routed.svg, 'with the copper of the first route');
  await page.waitForSelector('.chip:has-text("routed")');
  expect((await page.getByRole('button', { name: /Route again/ }).count()) === 1, 'the toolbar says it is routed');
  await shot('undone');

  step('the Modules tab lists the modules, a row picks the parts of one, and its button proposes it');
  await page.getByRole('button', { name: /^Modules \(3\)$/ }).click();
  const tree = page.locator('[data-testid=module-tree]');
  await tree.waitFor();
  const names = await tree.locator('.module-row .ref').allInnerTexts();
  expect(names.join(',') === 'a,b,c', `the modules: ${names.join(', ')}`);
  const rowB = tree.locator('.module-row[data-module="b"]');
  await rowB.click();
  await whenIdle(page);
  const picked = await page.locator('[data-testid=part-inspector]').innerText();
  expect(picked.includes('2 parts') && picked.includes('Regenerate b'), `the parts of b are picked: ${picked.replace(/\n/g, ' | ')}`);
  const fileNow = fs.readFileSync(layout, 'utf8');
  await rowB.locator('[data-testid=regen-module]').click();
  await page.waitForSelector('[data-testid=regen-card] :text("Regenerate b")');
  await whenIdle(page);
  expect((await page.locator('[data-testid=regen-ghost]').count()) === 2, 'two ghosts, for b.led and b.r');
  expect((await rowB.getAttribute('class')).includes('proposed'), 'the row of b is marked as the module proposed');
  await shot('module-tree');
  await page.locator('[data-testid=regen-reject]').click();
  await whenIdle(page);
  expect((await page.locator('[data-testid=regen-card]').count()) === 0, 'Reject drops the proposal');
  expect(fs.readFileSync(layout, 'utf8') === fileNow, 'and nothing was written');
}
