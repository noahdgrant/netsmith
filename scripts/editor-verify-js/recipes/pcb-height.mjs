// Height limits on the PCB canvas (ADR 0045 section 11): the hover says how tall a part is and what
// the limit under it allows, a finding that names a limit lights the limit's zone when it is
// picked, and a drag will not put a part that is too tall under a limit (it stops at the edge and
// says why), while a part that is not too tall, or states no height, goes where it likes.
import { whenIdle } from '../settle.mjs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { drag, fresh, labelOf, openPcb, partAt, partOf, scene, screenAt, settled } from '../pcb-helpers.mjs';

export const fixture = 'height_limits';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Lidded';

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/height_limits.pcb.toml'));
  await openPcb(page, BOARD);
  // The whole board in the window, so that every drag below starts and ends on screen.
  const fit = async () => {
    await page.getByRole('button', { name: 'Fit' }).click();
    await whenIdle(page);
  };
  await fit();
  const lidOf = (s) => s.keepouts.find((k) => k.name === 'lid');
  const hover = async (p) => {
    await page.mouse.move(p.x + 40, p.y + 40);
    await page.mouse.move(p.x, p.y);
    await page.waitForSelector('[data-testid=tip]');
    return page.locator('[data-testid=tip]').innerText();
  };

  step('the scene has the limit and the heights; the lid is drawn amber with its limit in words');
  const s0 = await scene(page, BOARD);
  const lid = lidOf(s0);
  expect(lid && lid.maxHeight === 3 && lid.layers.join() === 'top', 'the lid limits the top face to 3 mm');
  expect(partOf(s0, 'tall').height === 9 && partOf(s0, 'flat').height === 1.5 && partOf(s0, 'bare').height === null, 'heights 9, 1.5 and not stated');
  expect((await page.locator('.pcb-host svg text[data-height-limit="lid"]').textContent()) === 'max 3 mm', 'the zone says max 3 mm');
  const fill = await page.locator('.pcb-host svg rect[data-keepout="lid"]').getAttribute('stroke');
  expect(fill === '#e0a030', 'drawn in amber');

  step('the hover says how tall each part is and what the lid allows under it');
  const tall = await partAt(page, 'tall', BOARD);
  const hTall = await hover(tall.centre);
  expect(hTall.includes('stands 9 mm tall') && hTall.includes('lid allows 3 mm here: 6 mm too tall'), `the tall part: ${hTall.replace(/\n/g, ' | ')}`);
  await shot('hover-tall');
  const flat = await partAt(page, 'flat', BOARD);
  const hFlat = await hover(flat.centre);
  expect(hFlat.includes('stands 1.5 mm tall') && hFlat.includes('lid allows 3 mm here: fits'), `the flat part: ${hFlat.replace(/\n/g, ' | ')}`);
  const bare = await partAt(page, 'bare', BOARD);
  const hBare = await hover(bare.centre);
  expect(hBare.includes('height not stated') && hBare.includes('lid allows 3 mm here: unchecked'), `the part with no height: ${hBare.replace(/\n/g, ' | ')}`);
  const edge = await screenAt(page, lid.area.x + lid.area.width, lid.area.y + 2);
  const hLid = await hover(edge);
  expect(hLid.includes('keepout lid') && hLid.includes('parts at most 3 mm tall') && hLid.includes('on top'), `the lid's edge: ${hLid.replace(/\n/g, ' | ')}`);
  await shot('hover-lid');

  step('the inspector says the height of a picked part and the limit of a picked keepout');
  await page.mouse.click(tall.centre.x, tall.centre.y);
  await page.waitForSelector('[data-testid=part-inspector]');
  expect((await page.locator('[data-testid=part-inspector]').innerText()).includes('9 mm'), 'the part inspector has the height');

  step('the findings: an error for the tall part, a warning for the one with no height; picking the error lights the lid with its limit');
  await page.locator('.side-tabs button', { hasText: 'Findings' }).click();
  await page.waitForSelector('[data-testid=finding]');
  const rows = await page.locator('[data-testid=finding]').allInnerTexts();
  expect(rows.some((r) => r.includes('too_tall') && r.includes('the keepout `lid` allows 3.00 mm')), `a too_tall finding names the lid and its limit (${rows.length} findings)`);
  expect(rows.some((r) => r.includes('height_unchecked')), 'a height_unchecked warning');
  expect((await page.locator('[data-testid=keepout-lit]').count()) === 0, 'nothing is lit before a pick');
  await page.locator('[data-testid=finding]', { hasText: 'too_tall' }).click();
  await page.waitForSelector('[data-testid=keepout-lit]');
  expect((await page.locator('[data-testid=keepout-lit]').getAttribute('data-keepout-lit')) === 'lid', 'the lid is lit');
  expect((await page.locator('[data-testid=keepout-lit] text').textContent()) === 'lid: at most 3 mm', 'with its limit in words');
  await shot('finding-lights-the-lid');
  await page.locator('[data-testid=finding]', { hasText: 'too_tall' }).click();
  expect((await page.locator('[data-testid=keepout-lit]').count()) === 0, 'putting the pick back puts the light out');

  step('a part that starts under a limit it is too tall for may be dragged out of it');
  let c = await partAt(page, 'tall', BOARD);
  await page.mouse.click(c.centre.x, c.centre.y);
  let stamp = c.scene.layoutStamp;
  const far = await screenAt(page, lid.area.x + lid.area.width + 6, c.part.y);
  await drag(page, c.centre, { x: far.x, y: c.centre.y });
  let s = await settled(page, stamp, BOARD);
  const out = partOf(s, 'tall');
  expect(out.body.x >= lid.area.x + lid.area.width - 1e-6, `it is out of the lid (body at ${out.body.x}, lid ends at ${lid.area.x + lid.area.width})`);
  await page.waitForSelector('[data-testid=findings-chip]');

  step('dragged back toward the lid it stops at the edge, and says which limit holds it');
  c = await partAt(page, 'tall', BOARD);
  stamp = c.scene.layoutStamp;
  await page.mouse.move(c.centre.x, c.centre.y);
  await page.mouse.down();
  const to = await screenAt(page, c.part.x - 12, c.part.y);
  expect(to.x > 0, 'the end of the drag is on screen');
  for (let i = 1; i <= 10; i++) await page.mouse.move(c.centre.x + ((to.x - c.centre.x) * i) / 10, c.centre.y);
  const tip = await page.locator('[data-testid=tip]').innerText();
  expect(tip.includes('limited by lid') && tip.includes('stands 9 mm tall') && tip.includes('lid allows 3 mm'), `the note while held: ${tip.replace(/\n/g, ' | ')}`);
  await shot('held-at-the-edge');
  await page.mouse.up();
  s = await settled(page, stamp, BOARD);
  const held = partOf(s, 'tall');
  expect(held.body.x >= lid.area.x + lid.area.width - 1e-6, `the body stops at the lid's edge (body at ${held.body.x}, lid ends at ${lid.area.x + lid.area.width})`);
  expect(held.x < out.x, 'it did move toward the lid');
  expect((await page.locator('[data-testid=tip]').count()) === 0, 'the note goes when the drag ends');
  const found = await page.evaluate(async (b) => (await (await fetch(`/api/pcb/findings?board=${encodeURIComponent(b)}`)).json()).findings.map((f) => f.rule), BOARD);
  expect(!found.includes('too_tall'), `no too_tall finding is left (${found.join(', ')})`);

  step('a part that is not too tall goes under the lid, and so does one that states no height');
  for (const [name, label, dx, dy] of [['flat', 'flat', 4, -4], ['bare', 'bare', 12, 4]]) {
    const p = await partAt(page, name, BOARD);
    await page.mouse.click(p.centre.x, p.centre.y);
    const before = p.scene.layoutStamp;
    const dest = await screenAt(page, lid.area.x + dx, p.part.y + dy);
    await drag(page, p.centre, { x: dest.x, y: p.centre.y });
    s = await settled(page, before, BOARD);
    const q = partOf(s, name);
    expect(Math.abs(q.x - p.part.x) > 0.4 || Math.abs(q.y - p.part.y) > 0.4, `${label} (${labelOf(q)}) moved`);
    expect((await page.locator('[data-testid=tip]').count()) === 0, `no limit note for ${label}`);
  }
  await shot('flat-and-bare-under-the-lid');
}
