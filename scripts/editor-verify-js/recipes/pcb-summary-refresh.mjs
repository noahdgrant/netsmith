// The Summary tab of the PCB view after a source edit (issue 1637): with the tab open, an edit of the source that
// moves nothing the scene's stamps cover (they are the layout, physical and routes files') still has each section
// asked again when the scene is loaded again, as the 2D view's fill is. The source states how the pours join pads,
// on the process and then on a pin (which the pad's line names, issue 1636), and takes away a part of a pair of
// pads typed in, which is then left out with its reason.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { analysedSince, pageVersion, whenIdle } from '../settle.mjs';
import { scene, writeAtomic } from '../pcb-helpers.mjs';

export const fixture = 'buck_pours';
const BOARD = 'BuckPours';
const here = path.dirname(fileURLToPath(import.meta.url));

export async function run(tools) {
  const { page, shot, step, expect, project, url } = tools;
  const file = path.join(project, 'src/main.nsm');
  const original = fs.readFileSync(path.join(here, '../fixtures/buck_pours.nsm'), 'utf8');
  /** The source with each `[from, to]` made, which must be there to make. */
  const changed = (...changes) =>
    changes.reduce((text, [from, to]) => {
      if (!text.includes(from)) throw new Error(`the fixture has no ${JSON.stringify(from)}`);
      return text.replace(from, to);
    }, original);
  const importing = ['import std::mfg::jlcpcb::TwoLayer\n', 'import std::mfg::jlcpcb::TwoLayer\nimport std::mfg::stackup::PadConnection\n'];
  /** Write the source and wait until the page has taken the analysis of it and is idle. */
  const save = async (text) => {
    const before = await pageVersion(page);
    writeAtomic(file, text);
    await analysedSince(page, before);
  };

  await page.goto(url);
  await page.waitForSelector('.app');
  await whenIdle(page);
  if (fs.readFileSync(file, 'utf8') !== original) await save(original);
  try {
    await page.locator('.pane-strip [data-section="pcb"]').first().click();
    await page.waitForSelector('.pcb-overlay');
    await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
    await page.waitForSelector('[data-testid=summary-fill-counts]');
    await whenIdle(page);
    const warnings = page.locator('[data-testid=summary-fill-regulator-ground]');
    const top = page.locator('[data-testid=summary-copper-layer][data-kind=pour]').first();
    const stated = page.locator('[data-testid=summary-fill-pad][data-stated-by]');
    const before = await scene(page, BOARD);
    expect((await warnings.count()) === 1, 'the regulator\'s ground on a thermal relief is a warning');
    expect((await top.innerText()).includes('pads default'), 'the top pour joins pads as the process does');

    step('two pads typed in are measured');
    await page.locator('[data-testid=summary-pad-a]').fill('power_out.1');
    await page.locator('[data-testid=summary-pad-b]').fill('reg.ic.2');
    await page.locator('[data-testid=summary-measure]').click();
    await page.waitForSelector('[data-testid=summary-pads]');
    await whenIdle(page);

    step('the source makes the top pour join pads solid: the fill and the copper areas follow with the tab open');
    await save(changed(importing, ['.top.net &= "GND";', '.top.net &= "GND"; .top.pads &= PadConnection::Solid;']));
    const now = await scene(page, BOARD);
    expect(now.layoutStamp === before.layoutStamp && now.routesStamp === before.routesStamp, `the scene's stamps are the same (${now.layoutStamp}, ${now.routesStamp})`);
    expect((await warnings.count()) === 0, 'the warning is gone');
    expect((await top.innerText()).includes('pads solid'), `the copper areas say it (${await top.innerText()})`);
    await top.scrollIntoViewIfNeeded();
    await whenIdle(page);
    await shot('summary-refresh-pour');

    step('the source states it on the regulator\'s pin instead: the pad\'s line names the pin');
    await save(changed(importing, ['    .reg.vout ~ .output\n', '    .reg.vout ~ .output\n    .reg.ic.vin.neg.pour_connection &= PadConnection::Solid\n']));
    expect((await warnings.count()) === 0, 'no warning');
    expect((await top.innerText()).includes('pads default'), 'the pour is as it was');
    expect((await stated.count()) >= 1 && (await stated.first().getAttribute('data-stated-by')) === 'reg.ic.vin.neg', 'a pad is listed as its pin states');
    const area = page.locator('[data-testid=summary-fill-area]').filter({ has: stated }).first();
    await area.locator('summary').click();
    await stated.first().scrollIntoViewIfNeeded();
    await whenIdle(page);
    const line = await stated.first().innerText();
    expect(line.includes(': solid') && line.includes('(as its pin reg.ic.vin.neg states)'), `the line says so (${line})`);
    await shot('summary-refresh-pin');

    step('the source loses a part of the pair: the pair is left out, with the reason');
    await save(changed(['    .power_out: Header1x2\n', ''], ['    .output.pos ~ .power_out.p1\n    .output.neg ~ .power_out.p2\n', '']));
    expect((await warnings.count()) === 1, 'the warning is back with the source that has no statement');
    expect((await stated.count()) === 0, 'no pad is listed as a pin states');
    expect((await page.locator('[data-testid=summary-pads]').count()) === 0, 'no distance of the pair');
    const why = await page.locator('[data-testid=summary-left-out]').innerText();
    expect(why.includes('power_out.1 to reg.ic.2 is not measured') && why.includes('has no part `power_out`'), `why (${why})`);
    expect((await page.locator('[data-testid=summary-totals]').count()) === 1, 'the rest of the summary is there');
    await page.locator('[data-testid=summary-left-out]').scrollIntoViewIfNeeded();
    await whenIdle(page);
    await shot('summary-refresh-left-out');

    step('the part is back: so is the distance');
    await save(original);
    expect((await page.locator('[data-testid=summary-left-out]').count()) === 0, 'nothing is left out');
    expect((await page.locator('[data-testid=summary-pads]').count()) === 1, 'the pair is measured again');
    await page.locator('[data-testid=summary-pads]').scrollIntoViewIfNeeded();
    await whenIdle(page);
    await shot('summary-refresh-pair');
  } finally {
    // The recipes after this one of the same session start from the fixture. The page may be gone, so nothing waits on it.
    if (fs.readFileSync(file, 'utf8') !== original) writeAtomic(file, original);
  }
}
