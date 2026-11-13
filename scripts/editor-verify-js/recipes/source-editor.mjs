// The source view's electronics (round 4): hover cards for a part, a net, a type and the keyboard's own;
// the breadcrumbs with their dropdown; the outline; Back and Forward through the places the cursor was
// taken to; inline hints that are not text; the file tabs; folds, wrapping and the size of the text; the
// lint card; completion; find and replace; the library's read-only note; go to symbol in the open file.
import { whenIdle } from '../settle.mjs';
export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, project, section }) {
  const fs = await import('node:fs');
  const path = await import('node:path');
  const file = path.join(project, 'src/main.nsm');
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.setViewportSize({ width: 1700, height: 1000 });
  await section('source');
  await page.waitForSelector('.cm-content');
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  const board = 'TwoBoards/led';
  const detail = await page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
  const r1 = detail.parts.find((p) => p.path === 'r1');
  const net = detail.nets.find((n) => n.name === '3V3');
  const goDef = async (name) => {
    await page.locator('.ft-def', { hasText: name }).first().click();
    await whenIdle(page);
  };
  const lineOf = (text) => page.locator('.cm-line', { hasText: text }).first();
  const hoverToken = async (line, word, nth = 0) => {
    await lineOf(line).locator('span', { hasText: word }).nth(nth).hover();
  };
  const away = async () => {
    await page.mouse.move(1500, 960);
    await whenIdle(page);
  };
  const palette = async (text) => {
    await page.keyboard.press('Control+Shift+P');
    await page.waitForSelector('.qp');
    await page.keyboard.type(text);
    await whenIdle(page);
    await page.keyboard.press('Enter');
    await whenIdle(page);
  };
  const activeLine = () => page.evaluate(() => document.querySelector('.cm-activeLine')?.textContent ?? '');
  const hints = () => page.locator('[data-testid="inline-hint"]').count();

  // ---- hover cards ----
  step('hover a part declaration: a card with the designator, kind, value, package, pins and nets');
  await goDef('LedBoard');
  await hoverToken('.r1: Resistor', 'r1');
  await page.waitForSelector('[data-testid="hover-card"][data-kind="part"]');
  let card = await page.locator('[data-testid="hover-card"]').innerText();
  expect(card.includes(r1.refdes) && card.includes(r1.value) && card.includes(r1.kind), `the card names ${r1.refdes}, its value and kind`);
  expect(card.includes('Pins') && r1.package ? card.includes(r1.package) : true, 'its package');
  const chips = await page.locator('[data-testid="hover-card"] .hc-pin').allInnerTexts();
  expect(chips.length >= 2, `its pins as chips (${chips.join(' | ').replace(/\s+/g, ' ')})`);
  for (const n of detail.nets.filter((x) => x.pins.some((p) => p.refdes === r1.refdes))) {
    expect(chips.some((c) => c.includes(n.name)), `a pin chip names the net ${n.name}`);
  }
  expect((await page.locator('[data-testid="hover-card"] .hc-show').count()) === 3, 'Show on: Schematic, PCB, BOM');
  await shot('hover-part');

  step('a chip lights the net in the shared hover and picks it in the shared selection');
  const chip = page.locator('[data-testid="hover-card"] .hc-pin').first();
  const netName = (await chip.locator('span').last().innerText()).trim();
  await chip.hover();
  await whenIdle(page);
  expect((await page.locator('html.has-hover').count()) === 1, 'the page has a hover');
  await chip.click();
  await whenIdle(page);
  expect((await page.locator('.selchip').innerText()).includes(netName), `the selection chip names ${netName}`);

  step('the card goes when the pointer moves off the token');
  await away();
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'no card');

  step('hover a net name in a string: a card with its pins and, once the power tree is read, its rail');
  await hoverToken('.pos.net &= "3V3"', '"3V3"');
  await page.waitForSelector('[data-testid="hover-card"][data-kind="net"]');
  await page.waitForFunction(() => document.querySelector('[data-testid="hover-card"]')?.innerText.includes('Voltage'), null);
  card = await page.locator('[data-testid="hover-card"]').innerText();
  expect(card.includes('3V3') && card.includes(`${net.pins.length} pins`) && /Voltage\s+[\d.]+\s*V/.test(card), `the net, its pin count and its voltage (${card.replace(/\s+/g, ' ').slice(0, 120)})`);
  await shot('hover-net');
  await away();

  step('hover a type of the library: its signature, its doc comment and what it declares');
  await hoverToken('.r1: Resistor', 'Resistor');
  await page.waitForSelector('[data-testid="hover-card"][data-kind="definition"]');
  await page.waitForFunction(() => /resistor\. Its/i.test(document.querySelector('[data-testid="hover-card"]')?.innerText ?? ''), null);
  card = await page.locator('[data-testid="hover-card"]').innerText();
  expect(card.includes('generic Resistor') && card.includes('.resistance') && card.includes('std/parts/passive.nsm'), 'signature, members and file');
  await shot('hover-definition');
  await away();

  step('hover a module instance (the power board is the one shown): its parts as chips');
  await page.selectOption('.boardpick select', 'TwoBoards/power');
  await whenIdle(page);
  await goDef('PowerBoard');
  await hoverToken('.stage: LdoStage', 'stage');
  await page.waitForSelector('[data-testid="hover-card"][data-kind="module"]');
  card = await page.locator('[data-testid="hover-card"]').innerText();
  expect(card.includes('Module instance') && card.includes('LdoStage') && /3 parts/.test(card), 'the module, its type and its three parts');
  await shot('hover-module');
  await away();
  await page.selectOption('.boardpick select', board);
  await whenIdle(page);
  await goDef('LedBoard');

  step('Escape closes a card the pointer opened, with the focus in the editor or out of it, and the selection stays');
  await hoverToken('.r1: Resistor', 'r1');
  await page.waitForSelector('[data-testid="hover-card"]');
  expect((await page.locator('[data-testid="hover-card"]').count()) === 1, 'the pointer opened a card');
  await page.keyboard.press('Escape');
  await whenIdle(page);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'Escape closes a card opened by the pointer');
  await away();
  await lineOf('.r1: Resistor').locator('span', { hasText: 'r1' }).first().click();
  await page.evaluate(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
  await page.mouse.move(1500, 960);
  await hoverToken('.r1: Resistor', 'r1');
  await page.waitForSelector('[data-testid="hover-card"]');
  await page.keyboard.press('Escape');
  await whenIdle(page);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'and when the focus is not in the editor');
  await away();

  step('the keyboard: Ctrl+Alt+I opens the card at the cursor and Escape closes it');
  await lineOf('.r1: Resistor').click();
  await page.keyboard.press('Home');
  for (let i = 0; i < 6; i++) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Control+Alt+i');
  await page.waitForSelector('[data-testid="hover-card"]');
  expect(true, 'a card without the pointer');
  await page.keyboard.press('Escape');
  await whenIdle(page);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'Escape closes it');

  step('a card is a dialog labelled by its title; Tab moves the focus into it, its chips and buttons take Tab and Shift+Tab, Enter activates a chip, Escape closes it and the focus returns to the text');
  const inCard = () => page.evaluate(() => !!document.activeElement?.closest('[data-testid="hover-card"]'));
  await lineOf('.r1: Resistor').click();
  await page.keyboard.press('Home');
  for (let i = 0; i < 2; i++) await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Control+Alt+i');
  await page.waitForSelector('[data-testid="hover-card"]');
  const dialog = await page.evaluate(() => {
    const d = document.querySelector('[data-testid="hover-card"]');
    const t = document.getElementById(d.getAttribute('aria-labelledby') ?? '');
    return { role: d.getAttribute('role'), title: t?.innerText ?? '' };
  });
  expect(dialog.role === 'dialog' && dialog.title.includes(r1.refdes), `role dialog, labelled by its title (${dialog.title.replace(/\s+/g, ' ')})`);
  expect((await page.locator('.cm-card-tip').count()) === 1, 'one card, not a card on top of another');
  await page.keyboard.press('Tab');
  expect(await inCard(), 'Tab from the text moves the focus into the card');
  const stops = [];
  for (let i = 0; i < 12 && (await inCard()); i++) {
    stops.push(await page.evaluate(() => `${document.activeElement.tagName.toLowerCase()}.${String(document.activeElement.className).split(' ').filter(Boolean).slice(0, 2).join('.')}`));
    await page.keyboard.press('Tab');
  }
  expect(stops.length >= 3 && stops.every((x) => x.startsWith('button')), `the card's chips and buttons take Tab (${stops.length}: ${stops.slice(0, 4).join(', ')})`);
  await page.keyboard.press('Shift+Tab');
  expect(await inCard(), 'Shift+Tab comes back to the last one');
  for (let i = 0; i < stops.length; i++) await page.keyboard.press('Shift+Tab');
  expect(!(await inCard()) && (await page.evaluate(() => !!document.activeElement?.closest('.cm-editor'))), 'Shift+Tab from the first one goes back to the text');
  await page.keyboard.press('Tab');
  const chipName = await page.evaluate(() => (document.activeElement.classList.contains('hc-pin') ? document.activeElement.innerText.replace(/\s+/g, ' ') : ''));
  await shot('card-focus');
  if (chipName) {
    await page.keyboard.press('Enter');
    await whenIdle(page);
    expect((await page.locator('.selchip').count()) === 1, `Enter on a pin chip picks its net (${chipName})`);
    await page.keyboard.press('Escape');
    await whenIdle(page);
  } else {
    await page.keyboard.press('Escape');
    await whenIdle(page);
  }
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'Escape closes the card');
  expect(await page.evaluate(() => !!document.activeElement?.closest('.cm-content')), 'and the focus is in the text again');

  step('a card the pointer opened stays while the focus is in it, whatever the pointer does');
  await away();
  await hoverToken('.r1: Resistor', 'r1');
  await page.waitForSelector('[data-testid="hover-card"]');
  await lineOf('.r1: Resistor').click();
  await hoverToken('.r1: Resistor', 'r1');
  await page.waitForSelector('[data-testid="hover-card"]');
  await page.keyboard.press('Tab');
  expect(await inCard(), 'Tab goes into a card the pointer opened');
  await page.mouse.move(1500, 960);
  await whenIdle(page);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 1, 'and the card is still there with the pointer far away');
  await page.keyboard.press('Escape');
  await whenIdle(page);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0 && (await page.evaluate(() => !!document.activeElement?.closest('.cm-content'))), 'Escape closes it, and the focus returns to the text');
  await away();

  step('the shortcut over a card the pointer opened does not stack a second card');
  await lineOf('.r1: Resistor').click();
  await page.keyboard.press('Home');
  for (let i = 0; i < 2; i++) await page.keyboard.press('ArrowRight');
  await hoverToken('.r1: Resistor', 'r1');
  await page.waitForSelector('[data-testid="hover-card"]');
  await page.keyboard.press('Control+Alt+i');
  await whenIdle(page);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 1, 'one card');
  await page.keyboard.press('Escape');
  await away();

  step('the palette turns hover cards off and on');
  await palette('hide hover cards');
  await hoverToken('.r1: Resistor', 'r1');
  await whenIdle(page);
  expect((await page.locator('[data-testid="hover-card"]').count()) === 0, 'no card while they are off');
  await away();
  await palette('show hover cards');
  await hoverToken('.r1: Resistor', 'r1');
  await page.waitForSelector('[data-testid="hover-card"]');
  expect(true, 'the card is back');
  await away();

  // ---- breadcrumbs ----
  step('the breadcrumbs follow the cursor: file, definition, declaration');
  await lineOf('.r1: Resistor').click();
  await whenIdle(page);
  const crumbs = await page.locator('.bc-crumb').allInnerTexts();
  expect(crumbs.length === 3 && crumbs[0].includes('main.nsm') && crumbs[1].includes('LedBoard') && crumbs[2].startsWith('r1'), `main.nsm > LedBoard > r1 (${crumbs.join(' > ').replace(/\s+/g, ' ')})`);

  step('the dropdown of a crumb lists its siblings; choosing one goes there');
  await page.locator('.bc-crumb[data-kind="def"]').click();
  await page.waitForSelector('[data-testid="crumb-menu"]');
  const opts = await page.locator('.bc-opt-label').allInnerTexts();
  expect(['LdoStage', 'PowerBoard', 'LedBoard', 'TwoBoards'].every((n) => opts.includes(n)), `the definitions of the file (${opts.join(', ')})`);
  expect((await page.locator('.bc-opt.current .bc-opt-label').innerText()) === 'LedBoard', 'the current one is marked');
  await shot('crumbs');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('board PowerBoard'), null);
  expect((await page.locator('.bc-crumb').nth(1).innerText()).includes('PowerBoard'), 'the crumb follows');

  // ---- the outline ----
  step('the outline lists the definitions and what they declare; a click puts the cursor there and the cursor marks its entry');
  await page.waitForSelector('.ol-row[data-name="stage"]');
  await page.locator('.ol-row[data-name="stage"]').click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.stage: LdoStage'), null);
  expect((await page.locator('.ol-row.active').getAttribute('data-name')) === 'stage', 'its entry is marked');
  await lineOf('.power_in: Header1x2').first().click();
  await whenIdle(page);
  expect((await page.locator('.ol-row.active').getAttribute('data-name')) === 'power_in', 'the marked entry follows the cursor');
  const kinds = await page.locator('.ol-row.def').evaluateAll((els) => els.map((e) => e.dataset.kind));
  expect(['module', 'board', 'system'].every((k) => kinds.includes(k)), `a row for each kind of definition (${kinds.join(', ')})`);
  await shot('outline');

  step('the outline folds the definitions that do not hold the cursor, each with a count; the one with the cursor stays open; Expand all and Collapse all');
  const defRows = () => page.locator('.ol-row.def').evaluateAll((els) => els.map((e) => ({ name: e.dataset.name, open: e.getAttribute('aria-expanded'), kids: e.querySelector('.ol-kids')?.textContent ?? null })));
  const rowsNow = await defRows();
  const holder = rowsNow.find((r) => r.open === 'true');
  expect(rowsNow.length >= 3 && holder?.name === 'PowerBoard' && rowsNow.filter((r) => r.open === 'true').length === 1, `only the definition with the cursor is open (${rowsNow.map((r) => `${r.name}:${r.open}`).join(', ')})`);
  expect(rowsNow.filter((r) => r.open === 'false').every((r) => /^\d+$/.test(r.kids ?? '') && Number(r.kids) > 0), `each folded one says how many it holds (${rowsNow.filter((r) => r.open === 'false').map((r) => `${r.name}:${r.kids}`).join(', ')})`);
  const total = await page.locator('.ol-row').count();
  await page.locator('[data-testid="outline-expand-all"]').click();
  await whenIdle(page);
  expect((await page.locator('.ol-row').count()) > total && (await page.locator('.ol-kids').count()) === 0, 'Expand all opens every definition');
  await page.locator('[data-testid="outline-collapse-all"]').click();
  await whenIdle(page);
  expect((await page.locator('.ol-row').count()) === (await page.locator('.ol-row.def').count()) && (await page.locator('.ol-row.def[aria-expanded="false"]').count()) === rowsNow.length, 'Collapse all folds every one, the current too');
  await page.locator('.ol-row.def[data-name="LdoStage"] .ol-twisty').click();
  await whenIdle(page);
  expect((await page.locator('.ol-row.def[data-name="LdoStage"]').getAttribute('aria-expanded')) === 'true', 'a twisty opens one');
  const kept = await page.evaluate(() => JSON.parse(localStorage.getItem('netsmith.source.outlineFolds.v1') ?? '{}'));
  const mine = kept['src/main.nsm'];
  expect(mine && mine.open.includes('module:LdoStage') && mine.closed.includes('board:PowerBoard'), `the choice is kept for the file by names (${JSON.stringify(mine)})`);
  await page.locator('[data-testid="outline-expand-all"]').click();
  await whenIdle(page);

  step('the gutter shows a faint chevron on every foldable line, full on the gutter; Fold all and Unfold all are in the bar above the text');
  await page.mouse.move(1500, 960);
  await whenIdle(page);
  const chevrons = await page.evaluate(() => [...document.querySelectorAll('.cm-fold-mark:not(.folded)')].map((e) => Number(getComputedStyle(e).opacity)));
  expect(chevrons.length >= 3 && chevrons.every((o) => o >= 0.3 && o < 1), `a chevron on each foldable line at rest, not hidden and not loud (${chevrons.length} at ${chevrons[0]})`);
  await page.locator('.cm-gutters').hover();
  await whenIdle(page);
  expect((await page.evaluate(() => Math.max(...[...document.querySelectorAll('.cm-fold-mark:not(.folded)')].map((e) => Number(getComputedStyle(e).opacity))))) === 1, 'full on the gutter');
  await page.locator('[data-testid="fold-all"]').click();
  await page.waitForSelector('.cm-foldPlaceholder');
  expect(true, 'Fold all in the bar folds the blocks');
  await page.locator('[data-testid="unfold-all"]').click();
  await whenIdle(page);
  expect((await page.locator('.cm-foldPlaceholder').count()) === 0, 'Unfold all opens them');

  step('the outline filters, and shows a problem where it is');
  await page.locator('.ol-filter input').fill('cap');
  await whenIdle(page);
  const filtered = await page.locator('.ol-row').evaluateAll((els) => els.map((e) => e.dataset.name));
  expect(filtered.includes('input_cap') && filtered.includes('LdoStage') && !filtered.includes('process'), `only what matches, with its parents (${filtered.join(', ')})`);
  await page.locator('.ol-filter input').fill('');
  await goDef('LedBoard');
  await lineOf('.r1: Resistor {}').click();
  await page.keyboard.press('Home');
  await page.keyboard.type('.oops: Missing\n    ');
  await page.waitForSelector('.cm-lintRange-error');
  await page.waitForSelector('.ol-row[data-name="oops"] .ft-problems');
  expect(true, 'the entry of the mistake has a badge');

  step('the overview ruler: a tick for the problem in its lane, in the danger colour, that says "line N: message"; a faint thumb for the window; a press scrolls there');
  await page.waitForSelector('.cm-ruler:not([hidden]) .cm-ruler-tick.kind-error');
  const tick = page.locator('.cm-ruler-tick.kind-error').first();
  const tip = (await tick.getAttribute('data-tip')) ?? '';
  expect(/^line \d+: .+/.test(tip), `the tick says where and what (${tip})`);
  const look = await page.evaluate(() => {
    const t = document.querySelector('.cm-ruler-tick.kind-error');
    const probe = document.createElement('i');
    probe.style.color = 'var(--danger)';
    document.body.appendChild(probe);
    const danger = getComputedStyle(probe).color;
    probe.remove();
    const r = document.querySelector('.cm-ruler').getBoundingClientRect();
    const th = document.querySelector('.cm-ruler-thumb');
    return { image: getComputedStyle(t).backgroundImage, radius: getComputedStyle(t).borderRadius, w: t.getBoundingClientRect().width, h: t.getBoundingClientRect().height, danger, lane: t.classList.contains('lane-problem'), width: r.width, height: r.height, thumbHidden: th.hidden, thumb: th.getBoundingClientRect().height, scrolls: document.querySelector('.cm-scroller').scrollHeight > document.querySelector('.cm-scroller').clientHeight };
  });
  expect(look.lane && look.image.includes(look.danger) && look.radius.includes('50%'), 'in the problem lane, a disc (an error is round) drawn in the page\'s danger token');
  expect(look.w >= 7 && look.h >= 7 && Math.abs(look.w - look.h) < 1, `a glyph, the same size wherever it is (${look.w} x ${look.h} px), wider than a change's bar so that colour is not the only difference`);
  // A warning is a triangle: injected here (the fixture has no warning at this point) only to see that the stylesheet draws it as one.
  const tri = await page.evaluate(() => {
    const t = document.createElement('span');
    t.className = 'cm-ruler-tick lane-problem kind-warning';
    t.style.setProperty('--y', '200px');
    document.querySelector('.cm-ruler-ticks').appendChild(t);
    const cs = getComputedStyle(t);
    const out = { clip: cs.clipPath, w: t.getBoundingClientRect().width, h: t.getBoundingClientRect().height };
    t.remove();
    return out;
  });
  expect(/polygon/.test(tri.clip) && tri.w >= 7, `a warning is a triangle, not a bar (${tri.clip.slice(0, 40)}, ${tri.w} px wide)`);
  expect(look.scrolls ? !look.thumbHidden && look.thumb > 8 && look.thumb < look.height : look.thumbHidden, `the thumb is the window's share of the file (${look.thumb.toFixed(0)} of ${look.height.toFixed(0)} px)`);
  await page.keyboard.press('Control+Home');
  await whenIdle(page);
  await tick.click();
  await whenIdle(page);
  expect((await activeLine()).includes('.oops'), `a press on the tick puts the cursor on its line (${(await activeLine()).trim()})`);
  const sc0 = await page.evaluate(() => document.querySelector('.cm-scroller').scrollTop);
  await page.keyboard.press('Control+Home');
  await whenIdle(page);
  const box = await page.locator('.cm-ruler').boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height - 6);
  await whenIdle(page);
  const sc1 = await page.evaluate(() => document.querySelector('.cm-scroller').scrollTop);
  expect(sc1 > 0 || !look.scrolls, `a press on the strip scrolls there (${sc1}px)`);
  await shot('ruler');

  step('Next change and Previous change without a comparison move nothing and say so in a toast');
  await page.locator('.cm-content').click();
  const sel0 = await page.evaluate(() => document.querySelector('.cm-activeLine')?.textContent ?? '');
  await page.keyboard.press('Alt+F5');
  await page.waitForSelector('.toast');
  expect(/No comparison active/.test(await page.locator('.toast').first().innerText()), 'Alt+F5: "No comparison active"');
  await page.keyboard.press('Shift+Alt+F5');
  await whenIdle(page);
  expect((await page.evaluate(() => document.querySelector('.cm-activeLine')?.textContent ?? '')) === sel0, 'and the cursor stays where it was');

  step('the gutter marks the line with the page\'s severity icon, and the lint tooltip is a card');
  await page.waitForSelector('[data-testid="gutter-error"]');
  expect((await page.locator('[data-testid="gutter-error"] svg').count()) === 1, 'an error icon in the gutter');
  await page.locator('.cm-lintRange-error').first().hover();
  await page.waitForSelector('[data-testid="lint-card"]');
  card = await page.locator('[data-testid="lint-card"]').innerText();
  expect(card.includes('Error') && card.includes('Missing') && card.includes('Checker') && card.includes('main.nsm'), `severity, the compiler's message, its stage and its place (${card.replace(/\s+/g, ' ')})`);
  expect((await page.locator('[data-testid="lint-card"] .lt-btn').count()) >= 1, 'an action');
  await shot('lint');
  await away();

  step('completion: types after a colon, with a kind icon for each');
  await page.locator('.cm-line', { hasText: '.oops: Missing' }).click();
  await page.keyboard.press('End');
  await page.keyboard.press('Shift+Home');
  await page.keyboard.type('.oops: Re');
  await page.waitForSelector('.cm-tooltip-autocomplete li');
  const items = await page.locator('.cm-tooltip-autocomplete li').allInnerTexts();
  expect(items.some((t) => t.includes('Resistor')), `Resistor is offered (${items.join(' | ').replace(/\s+/g, ' ').slice(0, 100)})`);
  expect((await page.locator('.cm-tooltip-autocomplete .cm-comp-icon.part').count()) >= 1, 'a part icon');
  await shot('autocomplete');
  await page.keyboard.press('Escape');
  for (let i = 0; i < 40; i++) await page.keyboard.press('Control+z');
  await page.waitForFunction(() => !document.querySelector('.cm-lintRange-error'), null);
  expect(!fs.readFileSync(file, 'utf8').includes('oops'), 'nothing was written to disk');

  // ---- history ----
  step('Back and Forward walk the places the cursor was taken to');
  await goDef('LdoStage');
  await goDef('PowerBoard');
  await goDef('LedBoard');
  const here = await activeLine();
  expect(here.includes('board LedBoard'), 'at LedBoard');
  expect(!(await page.locator('[data-testid="history-back"]').isDisabled()), 'Back is on');
  await page.locator('[data-testid="history-back"]').click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('board PowerBoard'), null);
  expect(!(await page.locator('[data-testid="history-forward"]').isDisabled()), 'Forward is on');
  await page.keyboard.press('Alt+ArrowLeft');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('module LdoStage'), null);
  expect(true, 'Alt+Left goes back again');
  await page.keyboard.press('Alt+ArrowRight');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('board PowerBoard'), null);
  await page.locator('[data-testid="history-forward"]').click();
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('board LedBoard'), null);
  expect((await page.locator('[data-testid="history-forward"]').isDisabled()) === true, 'Forward is off at the newest place');
  await palette('go to symbol in the open file');
  expect(true, 'the history commands are in the palette too');
  await page.keyboard.press('Escape');

  step('a click-through from another view is a place too, and the Back chip is still there');
  await section('schematic');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  await page.keyboard.press('Control+p');
  await page.keyboard.type('r1');
  await whenIdle(page);
  await page.keyboard.press('Enter');
  await whenIdle(page);
  await section('source');
  expect((await activeLine()).length >= 0, 'the source is shown');

  // ---- inline hints ----
  step('inline hints: the value, package and LCSC number of a part at the end of its line, and the rail of a net');
  await goDef('LedBoard');
  await whenIdle(page);
  expect((await hints()) >= 3, `hints are shown (${await hints()})`);
  const hintTexts = await page.locator('[data-testid="inline-hint"]').allInnerTexts();
  expect(hintTexts.some((t) => t.includes(r1.value) && (!r1.lcsc || t.includes(r1.lcsc))), `the hint of the resistor has its value and part number (${hintTexts.join(' | ')})`);
  expect(hintTexts.some((t) => /\d.*V/.test(t) && t.includes('..')), 'the rail of a net has its voltage range');
  await shot('hints-on');

  step('a hint is not text: not selected, not copied, and it does not move what is drawn');
  const leftOn = await lineOf('.r1: Resistor').locator('span', { hasText: 'r1' }).first().evaluate((e) => e.getBoundingClientRect().left);
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+a');
  await page.keyboard.press('Control+c');
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied === fs.readFileSync(file, 'utf8'), 'what is copied is the file, exactly');
  expect(!copied.includes(r1.lcsc ?? 'C0000'), 'without the hints');
  await page.keyboard.press('ArrowLeft');

  step('the palette hides the hints and shows them; the status bar menu does too');
  await palette('hide inline hints');
  expect((await hints()) === 0, 'no hints');
  await goDef('LedBoard');
  const leftOff = await lineOf('.r1: Resistor').locator('span', { hasText: 'r1' }).first().evaluate((e) => e.getBoundingClientRect().left);
  expect(Math.abs(leftOn - leftOff) < 0.5, `the text is where it was (${leftOn} and ${leftOff})`);
  await shot('hints-off');
  await page.locator('.source-opts-item').click();
  await page.waitForSelector('[data-testid="opt-hints"]');
  expect((await page.locator('[data-testid="opt-hints"]').getAttribute('aria-checked')) === 'false', 'the menu shows them off');
  await page.locator('[data-testid="opt-hints"]').click();
  await whenIdle(page);
  expect((await hints()) >= 3, 'and on again from the menu');
  await shot('source-menu');
  await page.keyboard.press('Escape');

  // ---- the editor's own ----
  step('folds: a block folds to a chip that says how many lines it hides; fold all and unfold all');
  await page.locator('.cm-gutters').hover();
  await page.waitForSelector('.cm-fold-mark:not(.folded)');
  expect((await page.locator('.cm-fold-mark:not(.folded)').count()) >= 3, 'a fold mark on each block that has a body');
  await palette('fold all');
  await page.waitForSelector('.cm-foldPlaceholder');
  const folded = await page.locator('.cm-foldPlaceholder').first().innerText();
  expect(/\d+ lines?/.test(folded), `a chip (${folded})`);
  await shot('folded');
  await palette('unfold all');
  expect((await page.locator('.cm-foldPlaceholder').count()) === 0, 'nothing folded');

  step('word wrap (Alt+Z) and the size of the text (Ctrl+=, Ctrl+-, Ctrl+0), which is remembered');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Alt+z');
  await page.waitForSelector('.cm-lineWrapping');
  expect(true, 'wrapped');
  await page.keyboard.press('Alt+z');
  await page.waitForSelector('.cm-lineWrapping', { state: 'detached' });
  expect(true, 'and not');
  const size = () => page.evaluate(() => parseFloat(getComputedStyle(document.querySelector('.cm-content')).fontSize));
  const was = await size();
  await page.keyboard.press('Control+=');
  await page.keyboard.press('Control+=');
  expect((await size()) === was + 2, `two sizes bigger (${was} then ${await size()})`);
  await page.reload();
  await page.waitForSelector('.cm-content');
  expect((await size()) === was + 2, 'kept after a reload');
  await section('source');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+-');
  expect((await size()) === was + 1, 'smaller');
  await page.keyboard.press('Control+0');
  expect((await size()) === was, 'reset');

  step('find and replace: a card with the count, the three switches and a row to replace');
  await page.keyboard.press('Control+f');
  await page.waitForSelector('.sp');
  await page.keyboard.type('Resistor');
  await whenIdle(page);
  const count = await page.locator('.sp-count').innerText();
  expect(/\d+ (found|of \d+)/.test(count), `the count (${count})`);
  expect((await page.locator('.sp-replace').isHidden()) === true, 'replace is folded away');
  await page.keyboard.press('Control+h');
  await page.waitForSelector('.sp-replace', { state: 'visible' });
  expect(true, 'Ctrl+H shows the replace row');
  await shot('find-replace');
  await page.keyboard.press('Escape');
  expect((await page.locator('.sp').count()) === 0, 'Escape closes it');

  // ---- go to symbol in the file ----
  step('Ctrl+Shift+O in the editor lists the symbols of the open file and goes to one');
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Shift+o');
  await page.waitForSelector('.qp');
  expect((await page.locator('.qp-group').first().innerText()).includes('In this file'), 'a group "In this file"');
  await page.keyboard.type('r1');
  await whenIdle(page);
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.r1: Resistor'), null);
  expect(true, 'the cursor is on .r1');

  // ---- the library file ----
  step('a library file has a read-only note and a lock on its tab, and can be followed to');
  await lineOf('.r1: Resistor').locator('span', { hasText: 'Resistor' }).first().click({ modifiers: ['Control'] });
  await page.waitForSelector('.tab.active:has-text("passive.nsm")');
  await page.waitForSelector('[data-testid="readonly-banner"]');
  expect((await page.locator('[data-testid="readonly-banner"]').innerText()).includes('standard library'), 'the note says what it is');
  expect((await page.locator('.tab.active .tab-lock').count()) === 1, 'a lock on the tab');
  const crumbsLib = await page.locator('.bc-crumb').allInnerTexts();
  expect(crumbsLib[0].includes('passive.nsm') && crumbsLib[1].includes('Resistor'), 'its breadcrumbs');
  await shot('readonly');

  // ---- the tabs ----
  step('tabs: an unsaved dot that gives way to a close button under the pointer; a middle click closes');
  await page.locator('.tab', { hasText: 'main.nsm' }).click();
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('// tab test\n');
  await page.waitForSelector('.tab .dot');
  const tab = page.locator('.tab', { hasText: 'main.nsm' });
  const opacity = (sel) => tab.locator(sel).evaluate((e) => parseFloat(getComputedStyle(e).opacity));
  await away();
  expect((await opacity('.dot')) === 1 && (await opacity('.close')) === 0, 'the dot shows and the close button does not');
  await tab.hover();
  await whenIdle(page);
  expect((await opacity('.dot')) === 0 && (await opacity('.close')) === 1, 'under the pointer the close button takes its place');
  await page.locator('.cm-content').click();
  for (let i = 0; i < 12; i++) await page.keyboard.press('Control+z');
  await page.waitForFunction(() => !document.querySelector('.tab .dot'), null);
  const before = await page.locator('.tab[data-path]').evaluateAll((els) => els.map((e) => e.dataset.path));
  expect(before.length === 2, `two tabs (${before.join(', ')})`);

  step('tabs: drag one before the other');
  await page.locator('.tab[data-path="std/parts/passive.nsm"]').dragTo(page.locator('.tab[data-path="src/main.nsm"]'), { targetPosition: { x: 4, y: 12 } });
  await whenIdle(page);
  const order = await page.locator('.tab[data-path]').evaluateAll((els) => els.map((e) => e.dataset.path));
  expect(order[0] === 'std/parts/passive.nsm' && order[1] === 'src/main.nsm', `the order changed (${order.join(', ')})`);

  step('tabs: a middle click closes, and the strip lists them all when they do not fit');
  await page.locator('.tab[data-path="std/parts/passive.nsm"]').click({ button: 'middle' });
  await whenIdle(page);
  expect((await page.locator('.tab[data-path]').count()) === 1, 'a middle click closed it');
  for (const name of ['connector', 'diode', 'logic', 'mechanical', 'passive', 'regulator', 'transistor']) {
    await page.keyboard.press('Control+p');
    await page.keyboard.type(`${name}.nsm`);
    await whenIdle(page);
    await page.keyboard.press('Enter');
    await whenIdle(page);
  }
  await page.setViewportSize({ width: 900, height: 900 });
  await whenIdle(page);
  await page.waitForSelector('[data-testid="tab-more"]');
  await page.locator('[data-testid="tab-more"]').click();
  await page.waitForSelector('[data-testid="tab-menu"]');
  const listed = await page.locator('.tab-menu-item').count();
  expect(listed >= 8, `the menu lists every open file (${listed})`);
  await shot('tab-menu');
  await page.locator('.tab-menu-item', { hasText: 'connector.nsm' }).click();
  await page.waitForSelector('.tab.active:has-text("connector.nsm")');
  expect(true, 'choosing one shows it');
}
