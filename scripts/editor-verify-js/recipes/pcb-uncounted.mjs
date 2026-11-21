// The copper a net's length does not count, in the Summary tab and on the 2D PCB view (issue 1495): the recipe adds a dead stub
// to GND's copper in the routes file (a millimetre of track from the middle of one of its tracks, to nowhere), and the table of
// the nets (`GET /api/pcb/nets`, what `netsmith board nets` prints, `netsmith_fab::NetTable`) leaves it out of GND's length. The
// Summary tab says how much of GND's copper is not counted and lists the run (its kind, length, layer and a point of it); the
// 2D view draws it dashed in `--pcb-uncounted` over the track, `Not counted` in the Layers menu hides it, and a click on it,
// or on its chip in the tab, picks the track of the routes file it is.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { waitFor, whenIdle } from '../settle.mjs';
import { scene, writeAtomic } from '../pcb-helpers.mjs';

export const fixture = 'routed_led';
const BOARD = 'LedExample';
const here = path.dirname(fileURLToPath(import.meta.url));
// A track of GND from the middle of its track along y = 1.6, a millimetre down, to no pad.
const STUB = '  { layer = "top", width = 0.15, from = [5.0, 1.6], to = [5.0, 2.6] },\n';

export async function run(tools) {
  const { page, shot, step, expect, project, url } = tools;
  const file = path.join(project, 'src/main.routes.toml');
  const original = fs.readFileSync(path.join(here, '../fixtures/routed_led.routes.toml'), 'utf8');
  const anchor = '  { layer = "top", width = 0.15, from = [8.7757, 1.7757], mid = [8.5811, 1.6457], to = [8.3515, 1.6] },\n';
  expect(original.includes(anchor), "the fixture's GND tracks are as the recipe expects");
  writeAtomic(file, original.replace(anchor, anchor + STUB));
  // GND's tracks are numbered from 1 as the file lists them: the stub is the seventh.
  const stubTrack = 7;
  const stampOf = () => {
    const on = fs.statSync(file, { bigint: true });
    return `${on.mtimeNs}-${on.size}`;
  };
  try {
    await page.goto(url);
    await page.waitForSelector('.app');
    await waitFor('the server to have read the routes file', async () => (await scene(page, BOARD)).routesStamp === stampOf());
    await page.reload();
    await page.waitForSelector('.app');
    await page.locator('.pane-strip [data-section="pcb"]').first().click();
    await page.waitForSelector('.pcb-overlay');
    await page.locator('.side-tabs button', { hasText: 'Summary' }).click();
    await page.waitForSelector('[data-testid=summary-nets-uncounted][data-net=GND]');
    await page.waitForSelector('[data-testid=uncounted-copper][data-current=true]', { state: 'attached' });
    await whenIdle(page);

    step("the server leaves the stub out of GND's length and names it");
    const server = await page.evaluate(async (b) => (await fetch(`/api/pcb/nets?board=${encodeURIComponent(b)}`)).json(), BOARD);
    const gnd = server.nets.find((n) => n.net === 'GND');
    expect(!!gnd, 'GND is measured');
    // The fixture's own short tracks inside a pad are the way into it, counted and not listed (issue 1511): the stub is the one run.
    const stub = gnd.notCounted.find((r) => r.kind === 'stub' && Math.abs(r.length - 1) < 1e-3);
    expect(!!stub && gnd.notCounted.length === 1, `one run, a dead stub of 1 mm (${JSON.stringify(gnd.notCounted)})`);
    const runs = gnd.notCounted.reduce((sum, r) => sum + r.length, 0);
    expect(Math.abs(gnd.copper - gnd.length - runs) < 1e-3, `the runs add up to the copper less the length (${runs}; ${gnd.length} of ${gnd.copper})`);
    expect(stub.pieces.length === 1 && stub.layer === 'F.Cu', `one piece, on the front (${JSON.stringify(stub.pieces)})`);

    step('the Summary tab says how much of the copper is not counted, and lists the run');
    const line = page.locator('[data-testid=summary-nets-uncounted][data-net=GND]');
    const text = await line.textContent();
    const said = Math.round((gnd.copper - gnd.length) * 100) / 100;
    expect(text.includes(`GND: ${said} mm of its `) && text.includes('copper not counted in its length'), `the line of GND (${text})`);
    const chip = line.locator('button.uncounted-run[data-kind=stub]', { hasText: 'a dead stub of 1 mm on F.Cu at (' });
    expect((await chip.count()) === 1, 'a chip for the run');
    await line.scrollIntoViewIfNeeded();
    await whenIdle(page);
    await shot('uncounted-summary');

    step('the 2D view draws the stub dashed over its track, in a tone of its own, naming the track');
    const mark = page.locator('.l-uncounted > g.uncounted.stub[data-net=GND][data-length="1"] > line.uncounted-piece');
    expect((await mark.count()) === 1, `one mark (${await mark.count()})`);
    const drawn = await mark.evaluate((l) => ({
      x1: Number(l.getAttribute('x1')),
      y1: Number(l.getAttribute('y1')),
      x2: Number(l.getAttribute('x2')),
      y2: Number(l.getAttribute('y2')),
      track: l.getAttribute('data-route-track'),
      dash: getComputedStyle(l).strokeDasharray,
      stroke: getComputedStyle(l).stroke,
    }));
    const piece = stub.pieces[0];
    expect(
      Math.abs(drawn.x1 - piece.from[0]) < 1e-3 && Math.abs(drawn.y1 - piece.from[1]) < 1e-3 && Math.abs(drawn.x2 - piece.to[0]) < 1e-3 && Math.abs(drawn.y2 - piece.to[1]) < 1e-3,
      `where the server says (${JSON.stringify(drawn)})`,
    );
    expect(drawn.track === String(stubTrack), `it names the stub's track of the routes file (${drawn.track})`);
    expect(drawn.dash !== 'none', `dashed (${drawn.dash})`);
    const front = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--pcb-front').trim());
    const tone = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--pcb-uncounted').trim());
    expect(tone !== '' && tone !== front, `a tone apart from the front copper's (${tone}, the copper ${front})`);

    step('its chip in the tab zooms to it and picks the track');
    await chip.click();
    await whenIdle(page);
    const picked = page.locator(`.pcb-overlay .fixed-shape.picked[data-route-net=GND][data-route-track="${stubTrack}"]`);
    await picked.waitFor({ state: 'attached' });
    expect((await picked.count()) === 1, 'the stub is the track picked');
    await shot('uncounted-picked');

    step('a click on the mark picks the track too');
    await page.keyboard.press('Escape');
    await whenIdle(page);
    expect((await page.locator('.pcb-overlay .fixed-shape.picked').count()) === 0, 'Escape lets the track go');
    const box = await mark.boundingBox();
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
    await whenIdle(page);
    await picked.waitFor({ state: 'attached' });
    expect((await picked.count()) === 1, 'the click picked the stub');

    step('the Layers menu hides the marks and brings them back, and leaves the copper');
    const shown = (sel) => page.locator(sel).first().evaluate((e) => getComputedStyle(e).display !== 'none');
    await page.locator('details.layers summary').click();
    const toggle = async (label) => {
      await page.locator('.layer-list label', { hasText: label }).locator('input').click();
      await whenIdle(page);
    };
    await toggle('Not counted');
    expect(!(await shown('.l-uncounted')), 'Not counted off: the marks go');
    expect(await shown('.l-front-copper'), 'the front copper stays');
    await toggle('Not counted');
    expect(await shown('.l-uncounted'), 'Not counted on: they come back');
    await page.keyboard.press('Escape');
    await whenIdle(page);
  } finally {
    // The fixture's own file, for the recipes after this one in the session.
    writeAtomic(file, original);
  }
}
