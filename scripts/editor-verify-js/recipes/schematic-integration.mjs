// The schematic and the shared selection together, on the invented four-sheet board
// (`launch --fixture schematic_bench`): a part found with go to anything lands on its sheet at a
// zoom that reads, with a ring on it, and a view that already shows it does not move; a view
// zoomed far in is not kept there; a net picked in the schematic is the one the top bar chip and
// the inspector name; a click on a flag follows the net to the other sheet; Esc clears the
// selection in the schematic, the chip and the bill of materials.
import { activeSheet, openSheet, zoomPercent } from '../schematic-helpers.mjs';

export const fixture = 'schematic_bench';

/** The sheet each designator is drawn on, from the page's own drawing (hidden sheets are in it too). */
const sheetsOf = (page) =>
  page.evaluate(() => {
    const out = {};
    for (const slot of document.querySelectorAll('svg.schematic > g.sheet-slot')) {
      for (const g of slot.querySelectorAll('.part[data-ref]')) out[g.getAttribute('data-ref')] = slot.getAttribute('data-sheet');
    }
    return out;
  });

/** Where a drawn thing is on the screen, as a box. */
const screenBox = (page, selector) => page.locator(selector).first().boundingBox();

export async function run({ page, shot, step, expect, section }) {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.waitForSelector('svg.schematic');
  await page.waitForSelector('.sheetpick .crumb-sheet');
  const board = await page.locator('.boardpick select').inputValue();
  const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
  const sheetOf = await sheetsOf(page);
  const chip = async () => ((await page.locator('.selchip').count()) ? await page.locator('.selchip').innerText() : '');
  const host = () => page.locator('.schematic-host').boundingBox();
  const inView = async (ref) => {
    const h = await host();
    const b = await screenBox(page, `.sheet-slot:not([style*="none"]) .part[data-ref="${ref}"]`);
    return !!b && b.x >= h.x && b.y >= h.y && b.x + b.width <= h.x + h.width && b.y + b.height <= h.y + h.height;
  };
  const centreOff = async (ref) => {
    const h = await host();
    const b = await screenBox(page, `.sheet-slot:not([style*="none"]) .part[data-ref="${ref}"]`);
    return { dx: Math.abs(b.x + b.width / 2 - (h.x + h.width / 2)) / h.width, dy: Math.abs(b.y + b.height / 2 - (h.y + h.height / 2)) / h.height };
  };
  const goToAnything = async (text) => {
    await page.keyboard.press('Control+p');
    await page.waitForSelector('.qp[role=dialog]');
    await page.keyboard.press('Control+a');
    await page.keyboard.type(text);
    await page.waitForSelector('.qp-row[role=option]');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.qp[role=dialog]', { state: 'detached' });
  };
  const transform = () => page.locator('.schematic-host .canvas').evaluate((e) => e.style.transform);

  // The part to find: a capacitor on the microcontroller's sheet, which is large and opens at its top left.
  const mcu = Object.entries(sheetOf).find(([ref]) => detail.parts.find((p) => p.refdes === ref)?.kind === 'Mcu97')?.[1];
  const caps = detail.parts.filter((p) => p.kind === 'Capacitor' && sheetOf[p.refdes] === mcu);
  expect(!!mcu && caps.length >= 2, `the bench board has the microcontroller sheet (${mcu}) with capacitors (${caps.map((c) => c.refdes).join(', ')})`);
  const first = caps[caps.length - 1];
  // Open on another sheet first, so the pick has a sheet to change.
  const names = await page.locator('.sheetnav .sl-name').allInnerTexts();
  const other = names.find((n) => n !== 'All sheets' && n !== mcu);
  await page.locator('.sheetnav .sl-row', { has: page.locator('.sl-name', { hasText: new RegExp(`^${other}$`) }) }).click();
  expect((await activeSheet(page)) === other, `the sheet ${other} is open`);

  step('go to anything finds a part on another sheet: its sheet opens, the zoom reads, the part is on screen with a ring');
  await goToAnything(first.refdes.toLowerCase());
  await page.waitForFunction((s) => document.querySelector('.sheetpick .crumb-sheet')?.textContent === s, mcu);
  expect((await activeSheet(page)) === mcu, `the sheet ${mcu} is open`);
  await page.waitForSelector('.schematic-host .pulse-ring', { timeout: 3000 });
  expect(true, 'a ring is on the part');
  await page.waitForTimeout(150);
  const z1 = await zoomPercent(page);
  expect(z1 >= 140 && z1 <= 260, `the zoom is one the text reads at, not a close-up (${z1}%)`);
  expect(await inView(first.refdes), `${first.refdes} is on screen`);
  const c1 = await centreOff(first.refdes);
  expect(c1.dx < 0.5 && c1.dy < 0.5, 'and inside the view');
  expect((await chip()).includes(first.refdes), `the top bar chip names ${first.refdes}`);
  expect(await page.evaluate((r) => !!document.querySelector(`svg.schematic .part.mark:not(.linked)[data-ref="${r}"]`), first.refdes), 'the drawing marks it');
  await shot('part-found');
  await page.waitForSelector('.schematic-host .pulse-ring', { state: 'detached', timeout: 4000 });
  expect(true, 'the ring goes after a moment');

  step('a part that is on screen already: the view does not move, the ring shows where it is');
  const before = await transform();
  await goToAnything(first.refdes.toLowerCase());
  await page.waitForSelector('.schematic-host .pulse-ring', { timeout: 3000 });
  expect((await transform()) === before, 'the zoom and the place did not change');
  expect(await inView(first.refdes), `${first.refdes} is still where it was, on screen`);
  await page.waitForSelector('.schematic-host .pulse-ring', { state: 'detached', timeout: 4000 });

  step('a part off screen: the view pans to it and keeps the zoom');
  const small = detail.parts.filter((p) => sheetOf[p.refdes] === mcu && p.kind !== 'Mcu97' && p.refdes !== first.refdes);
  let away = null;
  for (const p of small) if (!(await inView(p.refdes))) away = p.refdes;
  expect(!!away, `a part of the sheet is off screen (${away})`);
  const zBefore = await zoomPercent(page);
  await goToAnything(away.toLowerCase());
  await page.waitForSelector('.schematic-host .pulse-ring', { timeout: 3000 });
  await page.waitForTimeout(200);
  expect(await inView(away), `${away} is on screen now`);
  const zAway = await zoomPercent(page);
  // The zoom is kept, or lowered where the part and what it is wired to (a crystal and the big IC) would not fit in view at it.
  expect(zAway <= zBefore + 1 && zAway >= 140, `at the zoom it was or less, never below what reads (${zBefore}% to ${zAway}%)`);
  const off = await centreOff(away);
  expect(off.dx < 0.1 && off.dy < 0.1, 'in the middle of the view');
  await page.waitForSelector('.schematic-host .pulse-ring', { state: 'detached', timeout: 4000 });

  step('zoomed far in, a pick elsewhere on the sheet does not leave the view that close');
  const area = await host();
  await page.mouse.move(area.x + area.width / 2, area.y + area.height / 2);
  for (let i = 0; i < 5; i++) {
    await page.mouse.wheel(0, -400);
    await page.waitForTimeout(80);
  }
  const zoomedIn = await zoomPercent(page);
  expect(zoomedIn > 300, `zoomed in to ${zoomedIn}%`);
  const target = small.find((c) => c.refdes !== away && !!c.refdes) ?? first;
  await goToAnything(target.refdes.toLowerCase());
  await page.waitForSelector('.schematic-host .pulse-ring', { timeout: 3000 });
  await page.waitForTimeout(200);
  const z3 = await zoomPercent(page);
  expect(z3 <= 205, `the zoom came back to a reading size (${zoomedIn}% to ${z3}%)`);
  expect(await inView(target.refdes), `${target.refdes} is on screen`);
  await shot('far-in-then-found');

  step('a net picked in the schematic: the chip, the inspector and the lit wires name the same net');
  await page.keyboard.press('Escape');
  await page.keyboard.press('f');
  const pickNet = await page.evaluate(() => {
    const host = document.querySelector('.schematic-host').getBoundingClientRect();
    const slot = document.querySelector('svg.schematic > g.sheet-slot:not([style*="none"])');
    for (const w of slot.querySelectorAll('.wire[data-net]')) {
      const net = w.getAttribute('data-net');
      if (net.startsWith('net-')) continue;
      const line = w.querySelector('polyline.line');
      const m = line.getScreenCTM();
      const p = line.getPointAtLength(line.getTotalLength() / 2);
      const x = m.a * p.x + m.c * p.y + m.e;
      const y = m.b * p.x + m.d * p.y + m.f;
      if (x > host.left + 30 && x < host.right - 30 && y > host.top + 30 && y < host.bottom - 30 && document.elementFromPoint(x, y)?.closest('[data-net]')?.getAttribute('data-net') === net) return { net, x, y };
    }
    return null;
  });
  expect(!!pickNet, 'a named wire is on screen to click');
  await page.mouse.click(pickNet.x, pickNet.y);
  await page.waitForFunction((n) => document.querySelector('.selchip')?.textContent.includes(n), pickNet.net);
  expect((await chip()).includes(pickNet.net), `the top bar chip names ${pickNet.net}`);
  // One chip, the top bar's: it says how many pins on how many sheets and steps through the pins.
  expect((await page.locator('.selchip, .sel-chip').count()) === 1, 'there is exactly one selection chip in the page');
  expect((await page.locator('.sel-chip').count()) === 0, 'and none on the drawing');
  expect(/\d+ pins? on \d+ sheets?/.test(await chip()), `the chip says how many pins on how many sheets (${(await chip()).replace(/\n/g, ' ')})`);
  const pinNow = () => page.evaluate(() => { const p = document.querySelector('svg.schematic .pin.cur'); return p ? `${p.getAttribute('data-ref')}.${p.getAttribute('data-pin')}` : null; });
  await page.locator('.selchip [aria-label="Next pin"]').click();
  await page.waitForFunction(() => !!document.querySelector('svg.schematic .pin.cur'));
  const p1 = await pinNow();
  await page.locator('.selchip [aria-label="Next pin"]').click();
  await page.waitForFunction((was) => { const p = document.querySelector('svg.schematic .pin.cur'); return p && `${p.getAttribute('data-ref')}.${p.getAttribute('data-pin')}` !== was; }, p1);
  const p2 = await pinNow();
  expect(!!p1 && !!p2 && p1 !== p2, `the chip's next pin button walks the pins (${p1} then ${p2})`);
  await page.locator('.selchip [aria-label="Previous pin"]').click();
  await page.waitForFunction((was) => { const p = document.querySelector('svg.schematic .pin.cur'); return p && `${p.getAttribute('data-ref')}.${p.getAttribute('data-pin')}` === was; }, p1);
  expect((await pinNow()) === p1, 'and the previous pin button goes back');
  if (!(await page.locator('.sinspector').count())) await page.getByRole('button', { name: 'Details' }).click();
  await page.waitForSelector('.sinspector .title');
  expect((await page.locator('.sinspector .title').first().innerText()).trim() === pickNet.net, 'the inspector shows the same net');
  expect(await page.evaluate((n) => document.querySelectorAll(`svg.schematic [data-net="${n}"].on`).length > 0, pickNet.net), 'the net is lit in the drawing');
  await page.keyboard.press('Control+j');
  await page.waitForSelector('.dock');
  expect((await chip()).includes(pickNet.net), 'opening the dock keeps the selection');
  await page.keyboard.press('Control+j');
  await shot('net-picked');

  step('a click on a flag follows the net to the other sheet and selects it there');
  await page.keyboard.press('Escape');
  // The chip's pin buttons walked to a pin on another sheet: come back to the microcontroller's, fitted, so its flags are on screen.
  await openSheet(page, mcu);
  await page.keyboard.press('f');
  await page.waitForTimeout(150);
  const flagNet = await page.evaluate(() => {
    const slot = document.querySelector('svg.schematic > g.sheet-slot:not([style*="none"])');
    const here = slot.getAttribute('data-sheet');
    const carriers = {};
    for (const s of document.querySelectorAll('svg.schematic > g.sheet-slot')) for (const p of s.querySelectorAll('.port[data-net]')) (carriers[p.getAttribute('data-net')] ??= new Set()).add(s.getAttribute('data-sheet'));
    const host = document.querySelector('.schematic-host').getBoundingClientRect();
    for (const p of slot.querySelectorAll('.port[data-net]')) {
      const net = p.getAttribute('data-net');
      const b = p.getBoundingClientRect();
      if (carriers[net].size === 2 && b.left > host.left && b.right < host.right && b.top > host.top && b.bottom < host.bottom) return { net, here, there: [...carriers[net]].find((s) => s !== here) };
    }
    return null;
  });
  expect(!!flagNet, 'a flag whose net is drawn on exactly one other sheet is on screen');
  const from = await activeSheet(page);
  expect(from === flagNet.here, `on ${from}`);
  await page.locator(`.sheet-slot:not([style*="none"]) .port[data-net="${flagNet.net}"]`).first().click();
  await page.waitForFunction((s) => document.querySelector('.sheetpick .crumb-sheet')?.textContent === s, flagNet.there);
  expect((await activeSheet(page)) === flagNet.there, `the flag of ${flagNet.net} led to ${flagNet.there}`);
  expect((await chip()).includes(flagNet.net), 'the chip names the net');
  expect(await page.evaluate((n) => document.querySelectorAll(`svg.schematic > g.sheet-slot:not([style*="none"]) [data-net="${n}"].on`).length > 0, flagNet.net), 'the net is lit on the other sheet');
  expect(await page.evaluate((n) => !!document.querySelector(`svg.schematic > g.sheet-slot:not([style*="none"]) .port[data-net="${n}"]`), flagNet.net), 'with its own flag there');
  await shot('flag-followed');

  step('Esc clears the selection in the drawing, the chip, the inspector and the other views');
  await page.locator('.schematic-host').click({ position: { x: 4, y: 4 } });
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('.selchip'));
  // What the pointer is over lights its nets, which is not the selection: take the pointer off the drawing.
  await page.mouse.move(100, 600);
  expect((await page.locator('.selchip, .sel-chip').count()) === 0, 'no selection chip is left, in the top bar or on the drawing');
  // The source's cursor still sits on the line of the last part found (go to anything puts it there, quietly): the part
  // it declares stays marked, dimmer (`linked`), as it always has. What is selected is not marked.
  const lit = await page.evaluate(() => [...document.querySelectorAll('svg.schematic .part.mark:not(.linked)')].map((e) => e.getAttribute('data-ref')));
  expect(lit.length === 0, `no part is marked as selected (${lit.join(' ')})`);
  const netLit = await page.evaluate((n) => document.querySelectorAll(`svg.schematic [data-net="${n}"].on`).length, flagNet.net);
  expect(netLit === 0, `the net ${flagNet.net} is not lit any more`);
  expect((await page.locator('.sinspector .title:has-text("' + flagNet.net + '")').count()) === 0, 'the inspector no longer shows the net');
  await section('bom');
  await page.waitForSelector('.bom tr');
  expect((await page.locator('.bom tr.sel').count()) === 0, 'no row of the bill of materials is lit');

  step('a pick made in another view shows in the schematic: a line of the bill of materials');
  const row = page.locator('.bom tbody tr').first();
  await row.click();
  await section('schematic');
  await page.waitForFunction(() => !!document.querySelector('.selchip'));
  expect(await page.evaluate(() => document.querySelectorAll('svg.schematic .part.mark:not(.linked)').length > 0), 'the schematic marks the part picked in the bill of materials');
  await shot('bom-pick');

  step('a click on a card of "All sheets" selects the module and opens its sheet');
  await page.keyboard.press('Escape');
  await page.locator('.sheetpick .pick-btn').click();
  await page.locator('.pick-pop .sl-row.all').click();
  await page.waitForSelector('.sheet-overview .ov-card');
  const cards = await page.locator('.sheet-overview .ov-card .ov-name').allInnerTexts();
  const modules = new Set(detail.parts.map((p) => p.path.split('.')[0]).filter((p) => detail.parts.some((q) => q.path.startsWith(`${p}.`))));
  const card = cards.find((n) => modules.has(n));
  expect(!!card, `a card names a module of the board (${cards.join(', ')})`);
  await page.locator('.sheet-overview .ov-card', { has: page.locator('.ov-name', { hasText: new RegExp(`^${card}$`) }) }).click();
  await page.waitForFunction((s) => document.querySelector('.sheetpick .crumb-sheet')?.textContent === s, card);
  expect((await activeSheet(page)) === card, `the sheet ${card} opened`);
  await page.waitForSelector('.selchip.k-module');
  expect((await chip()).includes(card), `the chip names the module ${card}`);
  expect((await page.locator('.selchip').getAttribute('aria-label')).includes('module'), 'the shared selection is a module');
  expect(await page.evaluate(() => document.querySelectorAll('svg.schematic > g.sheet-slot:not([style*="none"]) .part.mark:not(.linked)').length > 0), 'the module\'s parts are marked in the drawing');
  await shot('card-selects-module');
  await page.keyboard.press('Escape');

  step('at 1920 x 1080 the same pick fits the view and the ring is round the part');
  await page.keyboard.press('Escape');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.waitForTimeout(300);
  await goToAnything(first.refdes.toLowerCase());
  await page.waitForSelector('.schematic-host .pulse-ring', { timeout: 3000 });
  await page.waitForTimeout(250);
  expect(await inView(first.refdes), `${first.refdes} is on screen at 1920 x 1080`);
  const z4 = await zoomPercent(page);
  expect(z4 >= 140 && z4 <= 260, `at a zoom that reads (${z4}%)`);
  await shot('wide-found');
}
