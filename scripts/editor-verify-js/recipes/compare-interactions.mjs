// The comparison with a git ref next to what the rounds before it built: the status bar (the chip, the source's items, the
// connection) at three widths, the top bar's dialogs over the chip, the marks of the comparison that share an element with the
// hover and the selection, the diff view beside the source editor's own Escape, hover card and outline, the size and the density
// on the Compare section, the keys, and a board whose planes are layers. It needs `launch --git` (see compare.mjs).
export const fixture = 'two_boards';
export const needs = 'git';

export async function run({ page, shot, step, expect, section, resetRepo }) {
  resetRepo();
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await page.waitForSelector('.sidebar > *', { state: 'attached' });
  await page.setViewportSize({ width: 1920, height: 1080 });
  const settle = (ms = 300) => page.waitForTimeout(ms);
  const text = (sel) => page.locator(sel).first().innerText();
  const visible = (sel) => page.locator(`.section:not(.away) ${sel}`);
  const palette = async (what) => {
    await page.keyboard.press('Control+Shift+P');
    await page.waitForSelector('.qp-row');
    await page.keyboard.type(what);
    await settle(250);
    await page.keyboard.press('Enter');
    await settle(400);
  };

  // ------------------------------------------------------------------ the comparison to work with
  step('compare with main and turn the marks on');
  await page.keyboard.press('8');
  await page.waitForSelector('[data-testid="cmp-start"]');
  await visible('[data-testid="ref-button"]').click();
  await page.locator('[data-testid="ref-item"][data-name="main"]').click();
  await page.waitForSelector('[data-testid="cmp-summary"]', { timeout: 60000 });
  await page.selectOption('[data-testid="cmp-board"]', 'TwoBoards/led');
  await settle();
  await palette('show changes vs');
  expect(await page.evaluate(() => document.documentElement.classList.contains('cmp-on')), 'the marks are on');

  // ------------------------------------------------------------------ (a) the status bar
  /** What the status bar looks like: does anything run into another, is anything cut off, is the chip there. */
  const bar = () =>
    page.evaluate(() => {
      const b = document.querySelector('.statusbar');
      const r = b.getBoundingClientRect();
      const items = [...b.querySelectorAll('.item')].filter((e) => e.offsetWidth > 0).map((e) => ({ name: e.className.replace(/\bitem\b/, '').trim().split(/\s+/)[0] || 'item', box: e.getBoundingClientRect() }));
      const overlaps = [];
      for (let i = 0; i < items.length; i++)
        for (let j = i + 1; j < items.length; j++) {
          const a = items[i].box;
          const c = items[j].box;
          if (a.left < c.right - 0.5 && c.left < a.right - 0.5) overlaps.push(`${items[i].name}/${items[j].name}`);
        }
      const out = items.filter((i) => i.box.left < r.left - 0.5 || i.box.right > r.right + 0.5).map((i) => i.name);
      const left = b.querySelector('.left').getBoundingClientRect();
      const right = b.querySelector('.right').getBoundingClientRect();
      return { names: items.map((i) => i.name), overlaps, out, gap: Math.round(right.left - left.right), scrolls: b.scrollWidth > b.clientWidth + 1, chip: !!b.querySelector('[data-testid="compare-chip"]') && b.querySelector('[data-testid="compare-chip"]').offsetWidth > 0 };
    });
  for (const width of [1920, 1280, 1000, 800]) {
    step(`the status bar at ${width} px in the Source: nothing runs into another and nothing is cut off`);
    await page.setViewportSize({ width, height: width <= 1000 ? 700 : 900 });
    await section('source');
    await settle(500);
    const s = await bar();
    expect(s.overlaps.length === 0, `no two items overlap (${s.overlaps.join(', ') || 'none'}; shown: ${s.names.join(', ')})`);
    expect(s.out.length === 0 && !s.scrolls, `nothing leaves the bar (${s.out.join(', ') || 'none'})`);
    expect(s.gap >= 0, `the two sides do not touch (${s.gap} px apart)`);
    expect(s.chip, 'the comparison chip is still there: the questions about the design come before the text being edited');
    await shot(`statusbar-${width}`);
    if (width <= 1000) {
      // the busiest bar there is: an edit that is not saved, and the editor's job running
      await page.locator('.sec-source .cm-content').click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('x');
      await settle(300);
      const busy = await bar();
      expect(busy.names.includes('unsaved'), `an unsaved file is in the bar (${busy.names.join(', ')})`);
      expect(busy.overlaps.length === 0 && busy.out.length === 0 && !busy.scrolls && busy.gap >= 0, `and still nothing overlaps or leaves it (${busy.overlaps.join(', ') || 'none'}; ${busy.out.join(', ') || 'none'}; gap ${busy.gap})`);
      expect(busy.chip, 'with the chip');
      await shot(`statusbar-${width}-unsaved`);
      await page.keyboard.press('Control+Z');
      await settle(200);
    }
  }

  for (const width of [800, 1000]) {
    step(`the chip's menu at ${width} px opens upward from the chip, inside the window, above the bar`);
    await page.setViewportSize({ width, height: 700 });
    await settle(300);
    await page.click('[data-testid="compare-chip"]');
    await page.waitForSelector('.cmp-chip-menu');
    const menu = await page.evaluate(() => {
      const m = document.querySelector('.cmp-chip-menu').getBoundingClientRect();
      const sb = document.querySelector('.statusbar').getBoundingClientRect();
      return { left: m.left, right: m.right, top: m.top, bottom: m.bottom, bar: sb.top, w: innerWidth };
    });
    expect(menu.left >= 0 && menu.right <= menu.w && menu.top >= 0 && menu.bottom <= menu.bar + 1, `the menu is inside the window and above the bar (${JSON.stringify(menu)})`);
    await shot(`chip-menu-${width}`);
    await page.keyboard.press('Escape');
    await page.waitForSelector('.cmp-chip-menu', { state: 'detached' });
  }

  step('the settings panel and About open over the chip and the chip does not show through; opening one closes the other');
  await page.setViewportSize({ width: 1280, height: 800 });
  await settle(300);
  await page.click('[data-testid="compare-chip"]');
  await page.waitForSelector('.cmp-chip-menu');
  await page.click('[data-testid="menu-settings"]');
  await page.waitForSelector('[data-testid="panel-settings"]');
  expect((await page.locator('.cmp-chip-menu').count()) === 0, 'the settings panel closes the chip\'s menu');
  await shot('settings-over-chip');
  await page.keyboard.press('Escape');
  await page.click('[data-testid="menu-help"]');
  await page.click('[data-testid="help-about"]');
  await page.waitForSelector('[data-testid="about"], .about, [role="dialog"][aria-label*="About"]');
  await settle(300);
  const over = await page.evaluate(() => {
    const chip = document.querySelector('[data-testid="compare-chip"]').getBoundingClientRect();
    const top = document.elementFromPoint(chip.left + chip.width / 2, chip.top + chip.height / 2);
    return top ? top.closest('.statusbar') === null : true;
  });
  expect(over, 'what is on top at the chip is not the status bar: the dialog\'s backdrop covers it');
  await shot('about-over-chip');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-testid="about"], .about', { state: 'detached' }).catch(() => undefined);
  await settle(200);
  expect((await page.locator('.statusbar [data-testid="compare-chip"]').count()) === 1, 'the chip is back in the bar');

  // ------------------------------------------------------------------ (b) marks, hover and selection on the same elements
  step('schematic: a part that is marked as changed and hovered keeps both: the hover\'s light and the mark\'s colour');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await section('schematic');
  await page.waitForSelector('svg.schematic .part.cmp-changed');
  const changed = await page.evaluate(() => document.querySelector('svg.schematic .part.cmp-changed').getAttribute('data-ref'));
  const part = page.locator(`svg.schematic .part.cmp-changed[data-ref="${changed}"]`);
  const look = (sel) =>
    page.evaluate((s) => {
      const e = document.querySelector(s);
      const c = getComputedStyle(e);
      const shape = e.querySelector('.s');
      return { classes: [...e.classList].sort().join(' '), filter: c.filter, stroke: shape ? getComputedStyle(shape).stroke : '', dash: shape ? getComputedStyle(shape).strokeDasharray : '' };
    }, sel);
  const css = `svg.schematic .part.cmp-changed[data-ref="${changed}"]`;
  const plain = await look(css);
  await page.mouse.move(5, 5);
  await part.hover();
  await settle(400);
  const hovered = await look(css);
  expect(/hover-lit/.test(hovered.classes) && /cmp-changed/.test(hovered.classes), `hovering keeps the mark's class (${hovered.classes})`);
  expect(hovered.filter !== plain.filter, `hover changes the glow of a marked part (${plain.filter} -> ${hovered.filter})`);
  expect(hovered.stroke === plain.stroke, 'the outline stays the mark\'s colour while hovered');
  await shot('schematic-hover-marked');
  step('schematic: a part that is marked and picked shows it is picked, and still shows it changed');
  await part.click();
  await page.mouse.move(5, 5);
  await settle(400);
  const picked = await look(css);
  const token = (name) =>
    page.evaluate((n) => {
      const e = document.createElement('i');
      e.style.color = `var(${n})`;
      document.body.appendChild(e);
      const c = getComputedStyle(e).color;
      e.remove();
      return c;
    }, name);
  expect(/\bmark\b/.test(picked.classes) && /cmp-changed/.test(picked.classes), `picked and marked (${picked.classes})`);
  expect(picked.stroke === (await token('--sch-mark')), `the pick wins the outline (${picked.stroke})`);
  expect(/drop-shadow/.test(picked.filter), `the change is still shown by the glow (${picked.filter})`);
  await shot('schematic-picked-marked');
  await page.keyboard.press('Escape');

  step('the power tree: a card that is marked, hovered and picked keeps each signal');
  await section('power');
  await page.waitForSelector('.pt-node.cmp-changed');
  await settle(400);
  // the cards that a click picks (a rail of the other board is shown but the board in front picks its own)
  const marked = await page.evaluate(() => [...document.querySelectorAll('.pt-node[class*="cmp-"]')].map((e) => e.getAttribute('data-node')));
  let probe = null;
  for (const id of marked) {
    await page.locator(`.pt-node[data-node="${id}"] .pt-head`).first().click();
    await settle(250);
    // (a click on a rail of the board that is not in front picks the same net on the board that is)
    probe = await page.evaluate(() => document.querySelector('.pt-node.selected[class*="cmp-"]')?.getAttribute('data-node') ?? null);
    if (probe) break;
  }
  expect(probe !== null, `a marked card can be picked (${marked.join(', ')} -> ${probe})`);
  await page.keyboard.press('Escape');
  await page.mouse.move(5, 5);
  await settle(300);
  const sel = `.pt-node[data-node="${probe}"]`;
  const cardLook = () =>
    page.evaluate((q) => {
      const e = document.querySelector(q);
      const c = getComputedStyle(e);
      return { border: c.borderTopColor, shadow: c.boxShadow, style: c.borderTopStyle, classes: e.className };
    }, sel);
  const cardPlain = await cardLook();
  expect(/cmp-/.test(cardPlain.classes) && !/selected/.test(cardPlain.classes), `it is marked and not picked (${cardPlain.classes})`);
  await page.locator(sel).first().hover();
  await settle(400);
  const cardHover = await cardLook();
  expect(/hover-lit/.test(cardHover.classes) && /cmp-/.test(cardHover.classes) && cardHover.shadow !== cardPlain.shadow, `hovered it keeps its mark and adds the hover's ring (${cardHover.classes})`);
  await page.locator(`${sel} .pt-head`).first().click();
  await page.mouse.move(5, 5);
  await settle(400);
  const cardPick = await cardLook();
  const pickColour = await token('--sel');
  expect(/selected/.test(cardPick.classes) && /cmp-changed|cmp-added/.test(cardPick.classes), `picked, the card is rendered again and the mark is put back (${cardPick.classes})`);
  expect(cardPick.border === pickColour, `the pick has the card's border (${cardPick.border}, the pick is ${pickColour})`);
  expect(cardPick.shadow.split('rgb').length - 1 + cardPick.shadow.split('color(').length - 1 >= 3, `and its ring, and the mark's outside it (${cardPick.shadow})`);
  await shot('power-picked-marked');
  await page.keyboard.press('Escape');

  step('the board: a marked part has a glow, hovered it has the hover\'s too');
  await section('pcb');
  await page.waitForSelector('.pcb-host svg g[data-ref]');
  await settle(500);
  const pcbRef = await page.evaluate(() => {
    const e = document.querySelector('.pcb-host svg g[data-ref].cmp-changed') ?? document.querySelector('.pcb-host svg g[data-ref].cmp-added');
    return e ? e.getAttribute('data-ref') : null;
  });
  expect(pcbRef !== null, `a part of the board is marked (${pcbRef})`);
  const pcbSel = `.pcb-host svg g[data-ref="${pcbRef}"]`;
  const glow = () => page.evaluate((s) => getComputedStyle(document.querySelector(s)).filter, pcbSel);
  const glowPlain = await glow();
  await page.locator(pcbSel).first().hover({ force: true });
  await settle(400);
  const lit = await page.evaluate((s) => document.querySelector(s).classList.contains('hover-lit'), pcbSel);
  const glowHover = await glow();
  expect(lit && glowHover !== glowPlain, `hover adds to the mark's glow (${glowPlain} -> ${glowHover})`);
  await page.mouse.move(5, 5);

  // ------------------------------------------------------------------ the marks are classes only
  step('the two subscribers do not take each other\'s classes: switching the marks off leaves the hover on, and Escape ends the hover and keeps the marks');
  await section('schematic');
  await page.waitForSelector('svg.schematic .part.cmp-changed');
  await page.locator(css).hover();
  await settle(300);
  await palette('hide changes');
  const partOnly = `svg.schematic .part[data-ref="${changed}"]`;
  const afterOff = await page.evaluate((s) => ({ lit: document.querySelector(s).classList.contains('hover-lit'), marks: document.querySelectorAll('[class*="cmp-changed"], [class*="cmp-added"], [class*="cmp-renumbered"], [class*="cmp-net-"], [data-cmp-delta]').length }), partOnly);
  expect(afterOff.lit && afterOff.marks === 0, `the hover stays, every mark goes (${JSON.stringify(afterOff)})`);
  await palette('show changes vs');
  const afterOn = await page.evaluate((s) => ({ lit: document.querySelector(s)?.classList.contains('hover-lit'), mark: document.querySelector(s)?.classList.contains('cmp-changed') }), partOnly);
  expect(afterOn.lit && afterOn.mark, `turned on again, the part has both (${JSON.stringify(afterOn)})`);
  await page.mouse.move(5, 5);

  // ------------------------------------------------------------------ (c) the diff view and the source editor
  step('Source: the comparison over the editor; Escape closes it and nothing else, even after a click-through');
  await section('compare');
  await page.keyboard.press('Escape');
  await page.click('.cmp-tab[data-tab="parts"]');
  const row = page.locator('[data-testid="cmp-row"]', { has: page.locator('.cr-title', { hasText: /^r1$/ }) });
  await row.locator('[data-testid="link-head"]').click();
  await page.waitForSelector('.section.sec-source:not(.away) .cm-editor');
  await settle(500);
  const backFrom = await page.evaluate(() => location.hash || document.title);
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('show changes in source');
  await settle(250);
  await page.keyboard.press('Enter');
  await page.waitForSelector('.sec-source .diff-panel .cm-editor');
  await settle(400);
  await shot('diff-over-source');
  await page.locator('.sec-source .diff-panel').focus();
  await page.keyboard.press('Escape');
  await page.waitForSelector('.diff-panel', { state: 'detached' });
  await settle(300);
  expect(await page.evaluate(() => !document.querySelector('.section.sec-source')?.classList.contains('away')), `the diff closed and the Source is still on show (${backFrom})`);

  step('the diff view has no hover cards, hints, outline or breadcrumbs of its own; the editor under it keeps its own');
  await page.locator('[data-testid="source-changes"]').click().catch(() => undefined);
  await page.waitForSelector('.sec-source .diff-panel .cm-editor');
  const own = await page.evaluate(() => {
    const d = document.querySelector('.diff-panel');
    return { cards: d.querySelectorAll('[data-testid="hover-card"]').length, hints: d.querySelectorAll('.cm-hint, .cm-inlay').length, outline: d.querySelectorAll('[data-testid="outline"]').length, readonly: d.querySelector('.cm-content').getAttribute('aria-readonly'), editors: document.querySelectorAll('.sec-source .cm-editor').length };
  });
  expect(own.cards === 0 && own.hints === 0 && own.outline === 0 && own.readonly === 'true', `the diff view is plain text, read only (${JSON.stringify(own)})`);
  await page.locator('.sec-source .diff-panel .cm-content').first().hover();
  await settle(900);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'hovering in the diff view opens no hover card');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.diff-panel', { state: 'detached' });
  step('the editor still works after the diff: a hover card at the cursor, Escape closes it and nothing else');
  await page.waitForSelector('.cm-editor .cm-content');
  const kw = page.locator('.sec-source .cm-content .cm-line', { hasText: '.r1: Resistor' }).first();
  await kw.click();
  await page.keyboard.press('Home');
  for (let i = 0; i < 6; i++) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Control+Alt+i');
  await page.waitForSelector('[data-testid="hover-card"]', { timeout: 3000 });
  expect(true, 'a hover card opens at the cursor');
  await shot('card-after-diff');
  await page.keyboard.press('Escape');
  await settle(300);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'Escape closes the card first');
  expect(await page.evaluate(() => !document.querySelector('.section.sec-source')?.classList.contains('away')), 'and the Source stays on show, though a click-through came from the Compare section');

  // ------------------------------------------------------------------ (d) size and density on the Compare section
  step('Compare at 125 % and compact: the rows and the tiles follow the root size and the density; nothing overflows');
  const geometry = () =>
    page.evaluate(() => {
      const rect = (s) => document.querySelector(s)?.getBoundingClientRect();
      const row = rect('[data-testid="cmp-row"]');
      const tile = rect('[data-testid="tile-parts"]');
      const font = parseFloat(getComputedStyle(document.querySelector('.cmp-sentence')).fontSize);
      return { row: row?.height ?? 0, tile: tile?.height ?? 0, font, overflow: document.documentElement.scrollWidth > innerWidth, panel: document.querySelector('.sec-compare').scrollWidth - document.querySelector('.sec-compare').clientWidth };
    });
  await section('compare');
  await page.selectOption('[data-testid="cmp-board"]', 'all');
  await settle(400);
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 100, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');
  await section('compare');
  await page.waitForSelector('[data-testid="cmp-row"]');
  const g100 = await geometry();
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 125, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');
  await section('compare');
  await page.waitForSelector('[data-testid="cmp-row"]');
  const g125 = await geometry();
  expect(g125.font / g100.font > 1.2 && g125.tile / g100.tile > 1.15 && g125.row / g100.row > 1.15, `at 125 % the sentence, tiles and rows grow (font ${g100.font}->${g125.font}, tile ${g100.tile}->${g125.tile}, row ${g100.row}->${g125.row})`);
  expect(!g125.overflow && g125.panel <= 1, `and the page does not overflow (${JSON.stringify(g125)})`);
  await shot('compare-125');
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'compact', scale: 100, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');
  await section('compare');
  await page.waitForSelector('[data-testid="cmp-row"]');
  const gc = await geometry();
  expect(gc.row < g100.row && !gc.overflow, `compact makes a row shorter (${g100.row} -> ${gc.row})`);
  await shot('compare-compact');
  const diffLook = () =>
    page.evaluate(() => ({
      head: document.querySelector('.diff-head').getBoundingClientRect().height,
      font: getComputedStyle(document.querySelector('.diff-panel .cm-content')).fontSize,
      own: getComputedStyle(document.documentElement).getPropertyValue('--source-fs').trim(),
    }));
  const openBase = async () => {
    await page.locator('[data-testid="cmp-row"] [data-testid="link-base"]:not([disabled])').first().click();
    await page.waitForSelector('[data-testid="diff-panel"] .cm-editor');
    await settle(400);
  };
  await openBase();
  const dCompact = await diffLook();
  await shot('diff-compact');
  await page.keyboard.press('Escape');
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 125, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');
  await section('compare');
  await openBase();
  const d125 = await diffLook();
  expect(d125.head > dCompact.head * 1.25, `the diff view's bar follows the density and the size (${dCompact.head} compact, ${d125.head} at 125 %)`);
  expect(d125.font === dCompact.font && d125.font === d125.own, `its text is the source editor's own size, as the Source's (${d125.font} = ${d125.own}: the page's size does not change it)`);
  await shot('diff-125');
  await page.keyboard.press('Escape');
  await page.evaluate(() => localStorage.setItem('netsmith.settings', JSON.stringify({ density: 'comfortable', scale: 100, motion: 'system' })));
  await page.reload();
  await page.waitForSelector('.app');

  // ------------------------------------------------------------------ (e) the key
  step('the key 8 is the Compare section; the palette and the cheat sheet list it');
  await section('source');
  await page.keyboard.press('8');
  await page.waitForSelector('.section.sec-compare:not(.away)');
  expect(true, 'key 8 shows Compare');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp-row');
  await page.keyboard.type('compare');
  await settle(250);
  const rows = await page.locator('.qp-row').allInnerTexts();
  expect(rows.some((r) => /Compare/i.test(r) && /\b8\b/.test(r)), `the palette lists the Compare section with its key (${rows.slice(0, 4).map((r) => r.replace(/\s+/g, ' ')).join(' | ')})`);
  await page.keyboard.press('Escape');
  await page.keyboard.press('?');
  await page.waitForSelector('[data-testid="cheat-sheet"], .cheat, [role="dialog"][aria-label*="shortcut" i]');
  const sheet = await page.locator('[data-testid="cheat-sheet"], .cheat, [role="dialog"][aria-label*="shortcut" i]').first().innerText();
  expect(/Compare/.test(sheet) && /8/.test(sheet), 'the cheat sheet lists Compare and its key');
  await shot('cheat-sheet');
  await page.keyboard.press('Escape');
}
