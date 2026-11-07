// The schematic section: click a part in the drawing and the source opens at its declaration
// (Back returns); put the cursor on a declaration and the part is marked in the drawing. The notes (doc comments and `==` relations drawn as
// cards, ADR 0060) are the page's: its colours in all four themes, a hover, a click that opens the source at the line they are written at,
// and no overlap with a part.
import { THEMES, contrastOf, noteFacts, overlaps, partBoxes, setTheme, visibleHalos, zoomPercent } from '../schematic-helpers.mjs';

export const fixture = 'two_boards';

export async function run({ page, shot, step, expect, section }) {
  await page.waitForSelector('.schematic');
  step('pick the LED board, whose parts are plain');
  await page.selectOption('.boardpick select', 'TwoBoards/led');
  await page.waitForSelector('.schematic[data-board="LedBoard"]');
  await shot('led-board');

  step('click the resistor: it is selected and the nets dim; a double-click opens the Source at its declaration');
  await page.locator('.schematic .part[data-ref="R1"]').click();
  await page.waitForSelector('.selchip');
  expect((await page.locator('.selchip').innerText()).includes('R1'), 'the selection chip names R1');
  expect((await page.locator('.selchip, .sel-chip').count()) === 1, 'there is one chip, the top bar\'s, and none on the drawing');
  const wear = (state) =>
    page.evaluate(() => {
      const part = document.querySelector('.schematic .part[data-ref="R1"]');
      const sel = getComputedStyle(document.documentElement).getPropertyValue('--sel').trim();
      const probe = document.createElement('i');
      probe.style.color = 'var(--sel)';
      document.body.appendChild(probe);
      const selRgb = getComputedStyle(probe).color;
      probe.remove();
      const body = getComputedStyle(part.querySelector(':scope > .s'));
      const halo = getComputedStyle(part.previousElementSibling);
      const text = getComputedStyle(part.querySelector('text.ref'));
      return { sel, selRgb, stroke: body.stroke, dash: body.strokeDasharray, width: body.strokeWidth, haloStroke: halo.stroke, haloFill: halo.fill, textFill: text.fill, classes: part.getAttribute('class') };
    });
  // The halo's stroke and fill are CSS transitions (style.css, --dur-fast): a stroke goes from `none` to a colour at the
  // middle of one, so a read on the first frame of the pick saw `none` on a slow runner (CI). Read when they have run;
  // a halo that never comes is then what the next line reports.
  await page
    .waitForFunction(() => {
      const halo = document.querySelector('.schematic .part[data-ref="R1"]')?.previousElementSibling;
      return !!halo && halo.getAnimations().length === 0 && getComputedStyle(halo).stroke !== 'none';
    }, null, { timeout: 5000 })
    .catch(() => {});
  const picked = await wear();
  expect(picked.haloStroke === picked.selRgb, `a picked part has a halo in the selection's blue, behind it (${picked.haloStroke})`);
  expect(picked.dash === 'none' && picked.stroke !== picked.selRgb && picked.textFill !== picked.selRgb, `and its own strokes and labels are as drawn: not recoloured, not dashed (${picked.stroke}, ${picked.dash})`);
  const litNet = await page.evaluate(() => { const e = document.querySelector('.schematic .wire.on .line'); const c = e && getComputedStyle(e); return c ? { stroke: c.stroke, hit: getComputedStyle(document.querySelector('.schematic-host svg.schematic')).getPropertyValue('--hit').trim() } : null; });
  expect(litNet !== null, 'the nets of the picked part are lit');
  await shot('picked-halo');
  await page.locator('.schematic .part[data-ref="R1"]').dblclick();
  await page.waitForSelector('.section.sec-source:not(.away)');
  await page.waitForFunction(() => document.querySelector('.cm-activeLine')?.textContent.includes('.r1'));
  const line = await page.locator('.cm-activeLine').innerText();
  expect(line.includes('.r1: Resistor'), `the cursor on the line that declares R1 (${line.trim()})`);
  const marks = await page.locator('.cm-selectionMatch').count();
  expect(marks < 5, `a click-through does not mark every occurrence of what it selected (${marks} marks)`);
  expect((await page.locator('.statusbar').innerText()).match(/Ln (\d+)/) !== null, 'the status bar shows the line');
  expect((await page.locator('.back-btn').innerText()).includes('Schematic'), 'a Back button that names the schematic');
  await shot('clicked-r1');

  step('Escape goes back to the schematic');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  expect((await page.locator('.back-btn').count()) === 0, 'the Back button is gone');

  step('put the cursor on the LED declaration in the source');
  await section('source');
  await page.locator('.cm-line', { hasText: '.led: Led' }).first().click();
  await section('schematic');
  await page.waitForSelector('.schematic .part.mark[data-ref="D1"]');
  expect(true, 'D1 marked in the drawing');
  await shot('cursor-marks-d1');

  step('open the details and pick a part in the list: it is selected, the inspector shows it');
  if (!(await page.locator('table.parts').count())) await page.getByRole('button', { name: 'Details' }).click();
  await page.locator('table.parts tr', { has: page.locator('td.ref', { hasText: /^D1$/ }) }).click();
  await page.waitForSelector('.sinspector .title:has-text("D1")');
  expect((await page.locator('.sinspector .title').innerText()).includes('Led'), 'the inspector shows D1 as a Led');
  expect((await page.locator('.sinspector table.parts tr.sel').count()) === 1, 'the part list highlights it');
  await page.keyboard.press('Escape');

  // ---- notes ---- (the cursor of the source goes to the first line, which declares no part, so nothing is marked)
  await section('source');
  await page.locator('.cm-line').first().click();
  await section('schematic');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.schematic .note');
  await page.waitForFunction(() => document.querySelectorAll('svg.schematic .part.mark').length === 0);
  const facts = await noteFacts(page);
  step('the LED board draws its doc comments and its relation as notes, with the classes the drawing gives them');
  expect(facts.notes.length >= 3, `three notes or more are drawn (${facts.notes.length})`);
  expect(facts.notes.some((n) => n.cls.includes('note-doc')) && facts.notes.some((n) => n.cls.includes('note-relation')), 'a doc note and a relation note');
  const shape = await page.evaluate(() => [...document.querySelectorAll('svg.schematic g.note')].map((n) => ({ frame: !!n.querySelector('rect.note-frame'), text: !!n.querySelector('text.note-text tspan'), at: /^.+:\d+$/.test(n.getAttribute('data-at') ?? ''), index: /^\d+$/.test(n.getAttribute('data-n') ?? ''), group: n.parentElement.classList.contains('notes') })));
  expect(shape.every((n) => n.frame && n.text && n.at && n.index && n.group), 'each has a frame, selectable text in lines, data-at as file:line, data-n and sits in the notes group');
  expect(facts.notes.some((n) => n.ref === 'D1'), 'a note of a part says which (data-ref)');

  step('the words are readable on the card and the frame stands out from the sheet, in all four themes, and the page\u2019s colours win over the drawing\u2019s own');
  const original = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
  for (const theme of THEMES) {
    await setTheme(page, theme);
    await page.waitForTimeout(250);
    const f = await noteFacts(page);
    const min = theme.startsWith('hc') ? 7 : 4.5;
    for (const n of f.notes) {
      const text = contrastOf(n.textFill, n.frameFill);
      expect(text !== null && text >= min, `${theme}: a note's words are ${text?.toFixed(1)}:1 on its card (at least ${min}:1)`);
      // A note whose part is lit (the source's cursor is on D1's declaration) has the selection's blue for its frame.
      const stroke = n.lit ? f.tokens.sel : f.tokens.stroke;
      expect(n.frameFill === f.tokens.fill && n.textFill === f.tokens.text && n.frameStroke === stroke, `${theme}: the card, the frame${n.lit ? ' (lit: the selection\u2019s blue)' : ''} and the words are the theme's tokens (${n.frameFill}, ${n.frameStroke}, ${n.textFill})`);
      if (n.headingFill) expect(n.headingFill === f.tokens.heading && contrastOf(n.headingFill, n.frameFill) >= min, `${theme}: and so is the heading`);
      const frame = contrastOf(stroke, f.tokens.sheet);
      expect(frame !== null && frame >= 3, `${theme}: the frame is ${frame?.toFixed(1)}:1 against the sheet (at least 3:1)`);
    }
    if (theme === 'light' || theme === 'dark') await shot(`notes-${theme}`);
  }
  await page.evaluate((t) => (t === null ? document.documentElement.removeAttribute('data-theme') : document.documentElement.setAttribute('data-theme', t)), original);

  step('no note lies on a part');
  const boxes = await partBoxes(page);
  const noteBoxes = (await noteFacts(page)).notes;
  const clashes = [];
  for (const n of noteBoxes) for (const p of boxes) if (overlaps(n.box, p)) clashes.push(`${n.at} on ${p.ref}`);
  expect(boxes.length >= 3 && clashes.length === 0, `${noteBoxes.length} notes and ${boxes.length} parts, no overlap (${clashes.join(', ')})`);

  step('the pointer over a note lifts its card and lights nothing else');
  const rest = await page.locator('.schematic g.note .note-frame').first().evaluate((e) => ({ width: getComputedStyle(e).strokeWidth, filter: getComputedStyle(e).filter }));
  await page.locator('.schematic g.note').first().hover();
  await page.waitForTimeout(300);
  const lifted = await page.locator('.schematic g.note .note-frame').first().evaluate((e) => ({ width: getComputedStyle(e).strokeWidth, filter: getComputedStyle(e).filter }));
  expect(parseFloat(lifted.width) > parseFloat(rest.width) && lifted.filter !== 'none' && rest.filter === 'none', `the frame is heavier and has a soft shadow (${rest.width} ${rest.filter} to ${lifted.width})`);
  expect((await page.locator('svg.schematic .hover-lit').count()) === 0 && !(await page.evaluate(() => document.querySelector('svg.schematic').classList.contains('has-on'))), 'no part is lit, nothing is dimmed');
  expect((await visibleHalos(page)).length === 0, 'and no part has a halo');
  await shot('note-hover');

  step('a click on a note opens the Source at the line it is written at and selects nothing; Back returns');
  const file = await page.evaluate(async () => (await (await fetch('/api/file?path=src%2Fmain.nsm')).json()).text);
  const target = noteBoxes.find((n) => n.ref === 'D1') ?? noteBoxes[0];
  await page.locator(`.schematic g.note[data-at="${target.at}"]`).click();
  await page.waitForSelector('.section.sec-source:not(.away)');
  const written = file.split('\n')[Number(target.at.split(':').pop()) - 1].trim();
  await page.waitForFunction((t) => document.querySelector('.cm-activeLine')?.textContent.trim() === t, written, { timeout: 5000 }).catch(() => {});
  const active = (await page.locator('.cm-activeLine').innerText()).trim();
  expect(active === written && written.length > 0, `the cursor is on line ${target.at} (${written.slice(0, 50)})`);
  expect((await page.locator('.selchip').count()) === 0, 'no selection chip: a note is not a part');
  expect((await page.locator('.back-btn').innerText()).includes('Schematic'), 'a Back button that names the schematic');
  await shot('note-clicked-source');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  expect((await page.locator('.back-btn').count()) === 0, 'Escape goes back to the schematic');
  expect((await page.locator('svg.schematic .part.mark').count()) === 0, 'and no part was marked or picked');

  step('a part that is picked lights its own note, and its halo is the only halo');
  await page.locator('.schematic .part[data-ref="D1"]').hover();
  await page.waitForFunction(() => !!document.querySelector('svg.schematic .part[data-ref="D1"].hover-lit'));
  expect((await page.locator('svg.schematic g.note.hover-lit').count()) === 0, 'the shared hover lights the part D1 and not its notes (they carry its designator but are no part)');
  await page.locator('.schematic .part[data-ref="D1"]').click();
  await page.waitForSelector('.selchip');
  const withPick = await noteFacts(page);
  const own = withPick.notes.find((n) => n.ref === 'D1');
  expect(own && own.lit && own.opacity === '1', 'the note of D1 is lit with it');
  const halos = await visibleHalos(page);
  expect(halos.length === 1 && halos[0].ref === 'D1', `exactly one halo shows, D1's (${halos.map((h) => h.ref).join(',')})`);
  expect(!withPick.notes.some((n) => n.ref !== 'D1' && n.lit && n.ref !== null), 'the notes of other parts are not');
  const after = await partBoxes(page);
  expect(withPick.notes.every((n) => after.every((p) => !overlaps(n.box, p))), 'and still no note lies on a part');
  await shot('part-and-notes');
  await page.keyboard.press('Escape');

  // ---- notes by keyboard ----
  step('every note takes Tab, is a button with a label (Note: the first words), and says every part it is about');
  const kb = await page.evaluate(() =>
    [...document.querySelectorAll('svg.schematic g.note')].map((n) => ({ tab: n.getAttribute('tabindex'), role: n.getAttribute('role'), label: n.getAttribute('aria-label'), anchors: (n.getAttribute('data-anchors') ?? '').split(' ').filter(Boolean), rel: n.classList.contains('note-relation'), at: n.getAttribute('data-at') })),
  );
  expect(kb.length >= 3 && kb.every((n) => n.tab === '0' && n.role === 'button' && /^Note: .+/.test(n.label ?? '') && n.label.length <= 'Note: '.length + 61), `${kb.length} notes: tabindex 0, role button, aria-label "Note: ..." of at most 60 characters of the text`);
  const relation = kb.find((n) => n.rel);
  expect(relation && relation.anchors.length >= 2 && relation.anchors[0] === 'D1' && relation.anchors.includes('R1'), `the relation note names its parts, not only the first (${relation?.anchors.join(', ')})`);
  const order = await page.evaluate(() => {
    const all = [...document.querySelectorAll('svg.schematic g.note')];
    const focusable = [...document.querySelectorAll('svg.schematic [tabindex="0"], .schematic-host [tabindex="0"]')];
    return { firstNote: focusable.indexOf(all[0]), lastPart: Math.max(-1, ...[...document.querySelectorAll('svg.schematic g.part[tabindex="0"]')].map((p) => focusable.indexOf(p))) };
  });
  expect(order.firstNote > order.lastPart, 'the notes come after the parts in the Tab order');

  step('Tab from the canvas reaches the first note, a ring shows, and the next Tab goes to the next note');
  await page.locator('.schematic g.note').first().focus();
  expect(await page.evaluate(() => document.activeElement?.matches?.('g.note') ?? false), 'a note has the focus');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Shift+Tab');
  await page.waitForTimeout(350);
  const ring = await page.evaluate(() => {
    const n = document.activeElement;
    const frame = n?.querySelector?.('.note-frame');
    const rg = n?.querySelector?.('.note-ring');
    const probe = document.createElement('i');
    probe.style.color = 'var(--focus)';
    document.body.appendChild(probe);
    const focus = getComputedStyle(probe).color;
    probe.remove();
    const cs = rg && getComputedStyle(rg);
    const fcs = frame && getComputedStyle(frame);
    const unit = n?.ownerSVGElement?.getScreenCTM()?.a ?? 1;
    const f = frame?.getBoundingClientRect();
    const r = rg?.getBoundingClientRect();
    // From the outside of the frame's own stroke to the inside of the ring's 2 px line, in screen pixels.
    const gap = f && r ? f.left - r.left - (parseFloat(fcs.strokeWidth) * unit) / 2 - 1 : NaN;
    return { note: n?.matches?.('g.note'), stroke: cs?.stroke, focus, width: cs && parseFloat(cs.strokeWidth), display: cs?.display, vector: cs?.vectorEffect, filter: cs?.filter, frameFilter: fcs?.filter, frameWidth: fcs && parseFloat(fcs.strokeWidth), gap };
  });
  expect(ring.note && ring.stroke === ring.focus && ring.width === 2 && ring.display !== 'none' && ring.vector === 'non-scaling-stroke', `a crisp ring in the focus colour, 2 px at any zoom (${ring.stroke} ${ring.width}px ${ring.vector})`);
  expect(ring.filter === 'none' && ring.frameFilter === 'none' && ring.frameWidth <= 0.5, `no halo and the frame itself is not made heavier (${ring.filter}, ${ring.frameFilter}, ${ring.frameWidth})`);
  expect(Math.abs(ring.gap - 2) < 0.75, `the ring stands 2 px clear of the frame (${ring.gap.toFixed(2)} px)`);
  await shot('note-focus-ring');

  step('the ring keeps its 2 px, 2 px clear of the frame while the zoom changes under a note that keeps the focus (two levels)');
  const ringNow = () =>
    page.evaluate(() => {
      const n = document.activeElement;
      const frame = n?.querySelector?.('.note-frame');
      const rg = n?.querySelector?.('.note-ring');
      if (!n?.matches?.('g.note') || !frame || !rg) return null;
      const unit = n.ownerSVGElement?.getScreenCTM()?.a ?? 1;
      const f = frame.getBoundingClientRect();
      const r = rg.getBoundingClientRect();
      const stroke = (parseFloat(getComputedStyle(frame).strokeWidth) * unit) / 2;
      // From the outside of the frame's own stroke to the inside of the ring's 2 px line, on each side, in screen pixels.
      const gaps = [f.left - r.left, r.right - f.right, f.top - r.top, r.bottom - f.bottom].map((g) => g - stroke - 1);
      return { gaps, width: parseFloat(getComputedStyle(rg).strokeWidth), shown: getComputedStyle(rg).display !== 'none', unit, frameWidth: f.width };
    });
  // The note nearest the middle of the canvas, so it stays in view when the zoom (about the middle) changes.
  const middle = await page.evaluate(() => {
    const box = document.querySelector('.schematic-host')?.getBoundingClientRect();
    const notes = [...document.querySelectorAll('svg.schematic g.note')];
    const dist = (n) => {
      const r = n.querySelector('.note-frame').getBoundingClientRect();
      return Math.hypot(r.x + r.width / 2 - (box.x + box.width / 2), r.y + r.height / 2 - (box.y + box.height / 2));
    };
    return notes.indexOf(notes.reduce((a, b) => (dist(b) < dist(a) ? b : a)));
  });
  await page.locator('.schematic g.note').nth(middle).focus();
  await page.mouse.move(2, 2);
  const before = await ringNow();
  expect(before && before.shown, 'a ring is shown on the note that has the focus');
  const percentBefore = await zoomPercent(page);
  for (let i = 0; i < 4; i++) await page.keyboard.press('+');
  await page.waitForFunction((p0) => Number(document.querySelector('.zoombar span:not(.zsep)')?.textContent.replace('%', '')) > p0 * 1.5, percentBefore);
  await page.waitForTimeout(250);
  const zoomedIn = await ringNow();
  expect(zoomedIn && zoomedIn.frameWidth > before.frameWidth * 1.5, `the drawing is larger (note ${before?.frameWidth.toFixed(0)} to ${zoomedIn?.frameWidth.toFixed(0)} px wide)`);
  expect(zoomedIn.shown && zoomedIn.gaps.every((g) => Math.abs(g - 2) < 0.75) && zoomedIn.width === 2, `zoomed in the ring is still 2 px and 2 px clear on every side (${zoomedIn.gaps.map((g) => g.toFixed(2)).join(', ')} px)`);
  await shot('note-focus-ring-zoomed-in');
  for (let i = 0; i < 9; i++) await page.keyboard.press('-');
  await page.waitForFunction((p0) => Number(document.querySelector('.zoombar span:not(.zsep)')?.textContent.replace('%', '')) < p0 * 0.8, percentBefore);
  await page.waitForTimeout(250);
  const zoomedOut = await ringNow();
  expect(zoomedOut && zoomedOut.frameWidth < before.frameWidth * 0.8, `the drawing is smaller (note ${zoomedOut?.frameWidth.toFixed(0)} px wide)`);
  expect(zoomedOut.shown && zoomedOut.gaps.every((g) => Math.abs(g - 2) < 0.75) && zoomedOut.width === 2, `zoomed out the ring is still 2 px and 2 px clear on every side (${zoomedOut.gaps.map((g) => g.toFixed(2)).join(', ')} px)`);
  await shot('note-focus-ring-zoomed-out');
  for (let i = 0; i < 5; i++) await page.keyboard.press('+');
  await page.waitForTimeout(250);
  const second = await page.evaluate(() => {
    const all = [...document.querySelectorAll('svg.schematic g.note')];
    return all.indexOf(document.activeElement);
  });
  await page.keyboard.press('Tab');
  const third = await page.evaluate(() => [...document.querySelectorAll('svg.schematic g.note')].indexOf(document.activeElement));
  expect(third === second + 1 || third === -1, `Tab goes to the next note (${second} then ${third})`);
  await page.locator('.schematic g.note').first().focus();
  step('focus on a note lights the parts it is about in every view (the shared hover), and leaving puts the light out');
  const relEl = page.locator('.schematic g.note.note-relation').first();
  await relEl.focus();
  await page.waitForTimeout(300);
  const lit = await page.evaluate(() => [...document.querySelectorAll('svg.schematic g.part.hover-lit')].map((p) => p.getAttribute('data-ref')));
  expect(lit.length >= 2, `the relation's parts are lit (${lit.join(', ')})`);
  await page.locator('.schematic .stage, .schematic-host').first().click({ position: { x: 5, y: 5 } }).catch(() => {});
  await page.keyboard.press('Escape');

  step('Enter and Space on a note open the Source at its line, as a click does; Back returns');
  const doc = kb.find((n) => !n.rel) ?? kb[0];
  await page.locator(`.schematic g.note[data-at="${doc.at}"]`).focus();
  await page.keyboard.press('Enter');
  await page.waitForSelector('.section.sec-source:not(.away)');
  const wanted = file.split('\n')[Number(doc.at.split(':').pop()) - 1].trim();
  await page.waitForFunction((t) => document.querySelector('.cm-activeLine')?.textContent.trim() === t, wanted, { timeout: 5000 }).catch(() => {});
  expect((await page.locator('.cm-activeLine').innerText()).trim() === wanted, `Enter put the cursor on line ${doc.at}`);
  await page.keyboard.press('Escape');
  await page.waitForSelector('.section.sec-schematic:not(.away)');
  await page.locator(`.schematic g.note[data-at="${doc.at}"]`).focus();
  await page.keyboard.press(' ');
  await page.waitForSelector('.section.sec-source:not(.away)');
  expect(true, 'Space opens the source too');
  await page.keyboard.press('Escape');
  await page.waitForSelector('.section.sec-schematic:not(.away)');

  step('a relation about several parts is lit when any of them is: picking R1 (not its first part) lights it, and the unrelated notes dim');
  await page.locator('.schematic .part[data-ref="R1"]').click();
  await page.waitForSelector('.selchip');
  const rl = await page.evaluate(() => [...document.querySelectorAll('svg.schematic g.note')].map((n) => ({ rel: n.classList.contains('note-relation'), on: n.classList.contains('on'), anchors: n.getAttribute('data-anchors'), opacity: getComputedStyle(n).opacity })));
  const relation2 = rl.find((n) => n.rel);
  expect(relation2.on && relation2.anchors.split(' ').includes('R1') && relation2.anchors.split(' ')[0] !== 'R1', `the relation note is lit by R1, which is not its first part (${relation2.anchors})`);
  expect(rl.filter((n) => !n.on).every((n) => parseFloat(n.opacity) < 1), 'the notes of other parts dim');
  await shot('relation-lit-by-r1');
  await page.keyboard.press('Escape');
}
