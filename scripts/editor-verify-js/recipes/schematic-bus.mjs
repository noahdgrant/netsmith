// Busses in the schematic, on the invented board `bus_bench` (`launch --fixture bus_bench`): a
// processor module (`brain`), a modem module (`link`) and a header module (`probe`) share one UART,
// and the board's own parts have an SPI between two chips.
//   a. hovering a bus's thick line lights its member nets; a click picks the bus: all members are
//      lit, the top bar chip says `bus uart0 (4 nets)`, the inspector lists the instance, the
//      interface, the members with their nets as links and the pins of each net by module; a click on
//      one of the nets picks that net alone.
//   b. a click on a bundle flag follows it to the module that carries the same member nets (the other
//      end names the interface apart), with those nets lit; where two modules carry them a small menu
//      asks which.
//   c. go to anything finds a bus (`#uart`): it is picked and framed.
//   d. a module on screen is not boxed: opening `brain` from the navigator (the pointer stays on its row) or from the overview's card (which selects the
//      module) leaves every part without a halo; a part picked in it has exactly its own; in the overview a selected module's parts are marked in its card.
import { activeSheet, openSheet, visibleHalos } from '../schematic-helpers.mjs';

export const fixture = 'bus_bench';

/** A point on the screen that is on the line of a drawn thing and hits it, for a mouse to use. */
const pointOn = (page, selector) =>
  page.evaluate((sel) => {
    const host = document.querySelector('.schematic-host').getBoundingClientRect();
    const slot = document.querySelector('svg.schematic > g.sheet-slot:not([style*="none"])');
    for (const el of slot.querySelectorAll(sel)) {
      const m = el.getScreenCTM();
      const len = el.getTotalLength();
      for (const f of [0.5, 0.35, 0.65, 0.2, 0.8, 0.1, 0.9]) {
        const p = el.getPointAtLength(len * f);
        const x = m.a * p.x + m.c * p.y + m.e;
        const y = m.b * p.x + m.d * p.y + m.f;
        if (x < host.left + 10 || x > host.right - 10 || y < host.top + 10 || y > host.bottom - 10) continue;
        if (document.elementFromPoint(x, y)?.closest(sel.replace(/ \.(hit|line)$/, '')) !== null) return { x, y };
      }
    }
    return null;
  }, selector);

export async function run({ page, shot, step, expect }) {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.waitForSelector('svg.schematic');
  await page.waitForSelector('.sheetpick .crumb-sheet');
  const chip = async () => ((await page.locator('.selchip').count()) ? (await page.locator('.selchip').innerText()).replace(/\s+/g, ' ') : '');
  const litNets = () =>
    page.evaluate(() => {
      const slot = document.querySelector('svg.schematic > g.sheet-slot:not([style*="none"])');
      return [...new Set([...slot.querySelectorAll('.wire.on[data-net]')].map((w) => w.getAttribute('data-net')))].sort();
    });
  const busLit = (path) => page.evaluate((p) => !!document.querySelector(`svg.schematic g.bus-group.on[data-bus="${p}"]`), path);
  const flagPoint = (path) =>
    page.evaluate((p) => {
      const slot = document.querySelector('svg.schematic > g.sheet-slot:not([style*="none"])');
      const el = slot.querySelector(`.bus-flag[data-bus="${p}"] polygon`);
      const b = el.getBoundingClientRect();
      return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
    }, path);
  const MEMBERS = ['MODEM_CTS', 'MODEM_RTS', 'MODEM_RX', 'MODEM_TX'];

  step('the processor module draws its UART as one bus: a line, a count, a name and a flag');
  await openSheet(page, 'brain');
  expect((await activeSheet(page)) === 'brain', 'the module brain is open');
  await page.waitForSelector('svg.schematic > g.sheet-slot:not([style*="none"]) .bus-group[data-bus="brain.uart0"]');
  const texts = await page.evaluate(() =>
    [...document.querySelectorAll('svg.schematic > g.sheet-slot:not([style*="none"]) .bus-group[data-bus="brain.uart0"] text')].map((t) => t.textContent),
  );
  expect(texts.includes('uart0') && texts.includes('4'), `the bus is named uart0 and counted 4 (${texts.join(' | ')})`);
  expect((await page.locator('svg.schematic > g.sheet-slot:not([style*="none"]) .bus-flag').count()) === 1, 'one flag, not four');
  expect((await page.locator('svg.schematic > g.sheet-slot:not([style*="none"]) .port').count()) === 0, 'and no flag for a single member');
  await page.keyboard.press('f');
  await page.waitForTimeout(300);
  await shot('brain');

  step('the pointer over the line lights the four member nets and the line, not a net beside them');
  const onLine = await pointOn(page, '.bus[data-bus="brain.uart0"] .hit');
  expect(!!onLine, 'a point on the line is on screen');
  await page.mouse.move(onLine.x, onLine.y);
  await page.waitForTimeout(150);
  expect(JSON.stringify(await litNets()) === JSON.stringify(MEMBERS), `the four members are lit (${(await litNets()).join(', ')})`);
  expect(await busLit('brain.uart0'), 'and the thick line');
  expect((await page.locator('.selchip').count()) === 0, 'nothing is selected by hovering');
  expect(/bus/i.test(await page.locator('.sinspector .ih-eyebrow').first().innerText()), 'the inspector shows the bus under the pointer');
  await page.mouse.move(5, 5);

  step('a click on the line picks the bus: the chip, the inspector and the lit members agree');
  await page.mouse.click(onLine.x, onLine.y);
  await page.waitForFunction(() => document.querySelector('.selchip')?.textContent.includes('bus uart0'));
  const c1 = await chip();
  expect(/bus uart0/.test(c1) && /\(4 nets\)/.test(c1), `the chip says bus uart0 (4 nets) (${c1})`);
  expect((await page.locator('.selchip.k-bus').count()) === 1, 'it is the bus kind of chip');
  // Round 7: the chip kept `bus uar… (4 ne…` at 1280 px with room to spare beside it; it is as wide as its words, and shortens them only when the bar has no room (go to anything gives up its width first).
  for (const [w, h] of [[1920, 1080], [1600, 900], [1280, 800]]) {
    await page.setViewportSize({ width: w, height: h });
    await page.waitForTimeout(150);
    const fit = await page.evaluate(() => {
      const chip = document.querySelector('.selchip');
      const room = chip.closest('.tb-left').getBoundingClientRect().right - chip.getBoundingClientRect().right;
      const cmdk = document.querySelector('.topbar .cmdk');
      const spare = getComputedStyle(cmdk).flexShrink === '0' ? 0 : cmdk.getBoundingClientRect().width - (parseFloat(getComputedStyle(cmdk).minWidth) || 0); // as an icon (a narrow bar) it has nothing more to give
      const cut = [...chip.querySelectorAll('.selchip-title, .selchip-sub')].filter((e) => e.getClientRects().length && (() => { const r = document.createRange(); r.selectNodeContents(e); return r.getBoundingClientRect().width > e.getBoundingClientRect().width + 0.25; })()).map((e) => e.textContent);
      return { room, spare, cut, text: chip.innerText.replace(/\s+/g, ' ') };
    });
    expect(!(fit.cut.length && (fit.room > 12 || fit.spare > 2)), `at ${w} px the bus chip shortens its words only when the bar has no room (${fit.text}; ${Math.round(fit.room)} px beside it, go to anything ${Math.round(fit.spare)} px over its least, cut: ${fit.cut.join(' | ') || 'nothing'})`);
  }
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.waitForTimeout(150);
  expect(JSON.stringify(await litNets()) === JSON.stringify(MEMBERS), 'all four members are lit');
  expect(await busLit('brain.uart0'), 'and the line');
  expect(await page.evaluate(() => document.querySelector('svg.schematic').classList.contains('has-on')), 'the rest of the drawing is dimmed');
  const panel = page.locator('[data-testid="bus-panel"]');
  await panel.waitFor();
  const panelText = (await panel.innerText()).replace(/\s+/g, ' ');
  expect(/uart0/.test(panelText) && /Uart/.test(panelText) && /4 nets/.test(panelText), `the inspector names the instance, the interface and the count (${panelText.slice(0, 90)})`);
  expect(/brain\.uart0/.test(panelText), 'and the instance path');
  const rows = await panel.locator('.bus-member').evaluateAll((els) =>
    els.map((e) => ({ name: e.querySelector('.bus-member-name').textContent, net: e.querySelector('.net-link').textContent, groups: [...e.querySelectorAll('.net-group .grp > span:not(.count)')].map((g) => g.textContent) })),
  );
  expect(rows.map((r) => `${r.name}=${r.net}`).join(' ') === 'tx=MODEM_TX rx=MODEM_RX rts=MODEM_RTS cts=MODEM_CTS', `the members with their nets, in the order of the pins (${rows.map((r) => `${r.name}=${r.net}`).join(' ')})`);
  expect(rows.every((r) => r.groups.includes('brain') && r.groups.includes('link') && r.groups.includes('probe')), `each net's pins are listed by module: brain, link and probe (${rows[0].groups.join(', ')})`);
  expect((await panel.locator('.bus-member .chip').count()) >= 12, 'with each pin as a link');
  expect(/also on/i.test(await panel.innerText()), 'and where else the bus goes');
  await shot('picked');

  step('in a window of 1280 x 800 the chip still says the count (the usual chip drops its second line there)');
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.waitForTimeout(300);
  expect(/bus uart0 \(4 nets\)/.test(await chip()), `the chip at 1280 px: ${await chip()}`);
  await shot('narrow');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.waitForTimeout(300);

  step('the other views have the first member net as their selection: the bill of materials lights the parts on it');
  await page.locator('.pane-strip [data-section="bom"]').first().click();
  await page.waitForSelector('.section.sec-bom:not(.away)');
  await page.waitForFunction(() => document.querySelectorAll('.sec-bom tr.sel').length > 0);
  expect(/bus uart0/.test(await chip()), 'the chip is the same in the bill of materials');
  expect((await page.locator('.sec-bom tr.sel').count()) >= 2, `the parts on MODEM_TX are lit there (${await page.locator('.sec-bom tr.sel').count()} lines)`);
  await page.locator('.pane-strip [data-section="schematic"]').first().click();
  await page.waitForSelector('.section.sec-schematic:not(.away)');

  step('a click on one of the member nets in the inspector picks that net alone');
  await panel.locator('.bus-member', { hasText: 'MODEM_RX' }).locator('.net-link').click();
  await page.waitForFunction(() => /MODEM_RX/.test(document.querySelector('.selchip')?.textContent ?? '') && !/bus/.test(document.querySelector('.selchip')?.textContent ?? ''));
  expect((await page.locator('[data-testid="bus-panel"]').count()) === 0, 'the bus panel is gone for the net panel');
  await page.waitForFunction(() => document.querySelectorAll('svg.schematic > g.sheet-slot:not([style*="none"]) .wire.on[data-net]').length > 0 && ![...document.querySelectorAll('svg.schematic > g.sheet-slot:not([style*="none"]) .wire.on[data-net]')].some((w) => w.getAttribute('data-net') !== 'MODEM_RX'));
  expect((await litNets()).join() === 'MODEM_RX', `only that net is lit (${(await litNets()).join(', ')})`);
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('.selchip'));

  step('a click on the bundle flag follows it: two other modules carry the nets, so a small menu asks which');
  const f1 = await flagPoint('brain.uart0');
  await page.mouse.click(f1.x, f1.y);
  await page.waitForSelector('.flag-menu');
  const menu = (await page.locator('.flag-menu').innerText()).replace(/\s+/g, ' ');
  expect(/uart0 also on/i.test(menu) && /link/.test(menu) && /probe/.test(menu), `the menu names uart0 and the two modules (${menu})`);
  await shot('flag-menu');

  step('choosing a module opens it with the bus there picked and its nets lit, though the other end calls it serial');
  await page.locator('.flag-menu .menu-item', { hasText: 'link' }).click();
  await page.waitForFunction(() => document.querySelector('.sheetpick .crumb-sheet')?.textContent === 'link');
  await page.waitForFunction(() => document.querySelector('.selchip')?.textContent.includes('bus serial'));
  const c2 = await chip();
  expect(/bus serial/.test(c2) && /\(4 nets\)/.test(c2), `the chip names the bus of that module (${c2})`);
  expect(JSON.stringify(await litNets()) === JSON.stringify(MEMBERS), `the same four nets are lit there (${(await litNets()).join(', ')})`);
  expect(await busLit('link.serial'), 'and its line');
  await shot('followed');

  step('the modem\'s flag leads back: the processor and the probe both carry the nets, so the menu asks, and the processor is where its uart0 is');
  const f2 = await flagPoint('link.serial');
  await page.mouse.click(f2.x, f2.y);
  await page.waitForSelector('.flag-menu');
  expect(/probe/.test(await page.locator('.flag-menu').innerText()), 'the probe is in the menu with the processor');
  await page.locator('.flag-menu .menu-item', { hasText: 'brain' }).click();
  await page.waitForFunction(() => document.querySelector('.sheetpick .crumb-sheet')?.textContent === 'brain');
  expect(/bus uart0/.test(await chip()), `back at the processor's uart0 (${await chip()})`);
  await page.keyboard.press('Escape');

  step('a bus between two parts of one module: a trunk, no flag, and the same picking');
  await openSheet(page, 'BusBench');
  await page.waitForSelector('svg.schematic > g.sheet-slot:not([style*="none"]) .bus-group[data-bus="aux.spi"]');
  expect((await page.locator('svg.schematic > g.sheet-slot:not([style*="none"]) .bus-flag').count()) === 0, 'no flag: both ends are here');
  await page.keyboard.press('f');
  await page.waitForTimeout(300);
  const spi = await pointOn(page, '.bus[data-bus="aux.spi"] .hit');
  expect(!!spi, 'a point on the SPI trunk is on screen');
  await page.mouse.click(spi.x, spi.y);
  await page.waitForFunction(() => document.querySelector('.selchip')?.textContent.includes('bus spi'));
  expect(/bus spi \(4 nets\)/.test(await chip()), `the chip says bus spi (4 nets) (${await chip()})`);
  expect((await litNets()).length === 4, 'four nets lit');
  expect(!/also on/i.test(await page.locator('[data-testid="bus-panel"]').innerText()), 'and no other end listed: nothing else carries them');
  await page.keyboard.press('Escape');

  step('go to anything: # lists the buses before the nets; choosing one picks it and shows it');
  await openSheet(page, 'link');
  await page.keyboard.press('Control+p');
  await page.waitForSelector('.qp[role=dialog]');
  await page.keyboard.press('Control+a');
  await page.keyboard.type('#uart');
  await page.waitForSelector('.qp-row[role=option]');
  const groups = (await page.locator('.qp-group').allInnerTexts()).map((t) => t.split('\n')[0].trim());
  expect(/^buses$/i.test(groups[0]), `the buses are the first group (${groups.join(', ')})`);
  const rowsText = await page.locator('.qp-row[role=option]').allInnerTexts();
  expect(rowsText.some((t) => /uart0/.test(t)) && rowsText.some((t) => /serial/.test(t)) && rowsText.some((t) => /tap/.test(t)), `all three ends of the UART are listed (${rowsText.map((t) => t.replace(/\s+/g, ' ')).join(' | ')})`);
  await shot('palette');
  await page.locator('.qp-row[role=option]', { hasText: 'uart0' }).first().click();
  await page.waitForSelector('.qp[role=dialog]', { state: 'detached' });
  await page.waitForFunction(() => document.querySelector('.sheetpick .crumb-sheet')?.textContent === 'brain');
  expect(/bus uart0 \(4 nets\)/.test(await chip()), `the bus is picked and its module opened (${await chip()})`);
  expect(await busLit('brain.uart0'), 'and lit');
  await page.waitForTimeout(400);
  await shot('found');
  // The pointer is where the list's row was, over the drawing: moved off it, or it would light what it is over.
  await page.mouse.move(5, 5);
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => !document.querySelector('.selchip'));
  await page.waitForFunction(() => !document.querySelector('svg.schematic .wire.on, svg.schematic .bus-group.on'));
  expect((await litNets()).length === 0, 'Esc clears it, and nothing stays lit');

  // ---- d. the module on screen is not boxed ----
  const partsOnShow = () => page.evaluate(() => [...document.querySelectorAll('svg.schematic > g.sheet-slot:not([style*="none"]) g.part[data-ref]')].filter((p) => p.getClientRects().length).map((p) => p.getAttribute('data-ref')));
  const classesOnShow = () => page.evaluate(() => [...document.querySelectorAll('svg.schematic > g.sheet-slot:not([style*="none"]) g.part[data-ref]')].filter((p) => p.getClientRects().length).map((p) => p.getAttribute('class')));

  step('opening a module from the navigator: its parts have no halo while the pointer rests on its row');
  await openSheet(page, 'link');
  await page.mouse.move(5, 5);
  await page.waitForTimeout(300);
  await page.locator('.sheetnav .sl-row', { has: page.locator('.sl-name', { hasText: /^brain$/ }) }).click();
  await page.waitForFunction(() => document.querySelector('.sheetpick .crumb-sheet')?.textContent === 'brain');
  await page.waitForTimeout(600);
  const brainParts = await partsOnShow();
  expect(brainParts.length === 8, `brain draws its ${brainParts.length} parts`);
  expect((await visibleHalos(page)).length === 0, 'no part has a halo with a fill or a stroke (the computed style of each), the pointer still on the row');
  expect((await classesOnShow()).every((c) => c === 'part'), 'and none is marked or lit (the shared hover lights nothing of the sheet that is open)');
  await shot('brain-from-navigator');

  step('opening it from its card in the overview selects the module, and still no part is boxed');
  await openSheet(page, 'All modules');
  await page.waitForSelector('.sheet-overview .ov-card');
  await page.locator('.sheet-overview .ov-card', { has: page.locator('.ov-name', { hasText: /^brain$/ }) }).click();
  await page.waitForFunction(() => document.querySelector('.sheetpick .crumb-sheet')?.textContent === 'brain');
  await page.waitForFunction(() => /module/i.test(document.querySelector('.selchip')?.textContent ?? ''));
  expect(/brain/.test(await chip()), `the module is the selection: the chip says so (${(await chip()).replace(/\n/g, ' ')})`);
  await page.mouse.move(5, 5);
  await page.waitForTimeout(600);
  expect((await visibleHalos(page)).length === 0, 'no part has a halo');
  expect((await classesOnShow()).every((c) => !/\bmark\b|\bon\b/.test(c)), 'none is marked, and the drawing is not dimmed');
  expect(!(await page.evaluate(() => document.querySelector('svg.schematic > g.sheet-slot:not([style*="none"])').closest('svg').classList.contains('has-on'))), 'nothing is dimmed (has-on is off)');
  await shot('brain-from-card');

  step('a part picked inside the module has exactly its own halo');
  const one = brainParts[brainParts.length - 1];
  await page.locator(`svg.schematic > g.sheet-slot:not([style*="none"]) g.part[data-ref="${one}"]`).click();
  await page.waitForFunction((r) => document.querySelector('.selchip')?.textContent.includes(r), one);
  await page.mouse.move(5, 5);
  await page.waitForTimeout(600);
  const halos = await visibleHalos(page);
  expect(halos.length === 1 && halos[0].ref === one, `exactly one halo shows, ${one}'s (${halos.map((h) => h.ref).join(',')})`);
  await shot('brain-part-picked');

  step('the module selected again, in the overview its card marks its parts and the others none; on another sheet nothing changes');
  await page.keyboard.press('Escape');
  await openSheet(page, 'All modules');
  await page.locator('.sheet-overview .ov-card', { has: page.locator('.ov-name', { hasText: /^brain$/ }) }).click();
  await page.waitForFunction(() => document.querySelector('.sheetpick .crumb-sheet')?.textContent === 'brain');
  await openSheet(page, 'All modules');
  await page.waitForSelector('.sheet-overview .ov-card');
  await page.mouse.move(5, 5);
  await page.waitForTimeout(600);
  const cards = await page.evaluate(() =>
    [...document.querySelectorAll('.sheet-overview .ov-card')].map((card) => {
      const name = card.querySelector('.ov-name')?.textContent;
      const marked = [...card.querySelectorAll('g.part.mark')].length;
      const boxed = [...card.querySelectorAll('.part-halo')].filter((h) => { const c = getComputedStyle(h); return c.stroke !== 'none' && !/\/ 0\)$/.test(c.stroke); }).length;
      return { name, marked, boxed };
    }),
  );
  const brainCard = cards.find((c) => c.name === 'brain');
  expect(brainCard.marked === 8 && brainCard.boxed === 8, `the card of brain marks its 8 parts, as before (${JSON.stringify(brainCard)})`);
  expect(cards.filter((c) => c.name !== 'brain').every((c) => c.marked === 0 && c.boxed === 0), 'and the cards of the other modules none');
  await shot('overview-module-selected');
  await openSheet(page, 'link');
  await page.mouse.move(5, 5);
  await page.waitForTimeout(400);
  expect((await visibleHalos(page)).length === 0 && (await classesOnShow()).every((c) => c === 'part'), 'link, another sheet, shows no mark of the selection of brain');
  await page.keyboard.press('Escape');
}
