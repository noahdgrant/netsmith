// The Schematic's keys are commands of the editor's registry: in the command palette and in the
// cheat sheet under a "Schematic" group with their chords, working only while the Schematic is the
// view in use and no text field has the focus; the toolbar's `?` opens the cheat sheet scrolled to
// the group, and the view has no key list of its own.
import { activeSheet, zoomPercent } from '../schematic-helpers.mjs';

export const fixture = 'schematic_bench';

const TITLES = [
  ['Previous module', '['],
  ['Next module', ']'],
  ['Fit the module', 'F'],
  ['Fit the width of the module', 'W'],
  ['Zoom to 100%', 'Shift'],
  ['Zoom to the selection', 'Shift'],
  ['Zoom in', '+'],
  ['Zoom out', '-'],
  ['Pan left', '←'],
  ['Pan right', '→'],
  ['Pan up', '↑'],
  ['Pan down', '↓'],
  ['Next pin of the selected net', 'Tab'],
  ['Previous pin of the selected net', 'Tab'],
  ['Open the selected part in Source', 'Enter'],
];

export async function run({ page, shot, step, expect, section }) {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.waitForSelector('svg.schematic');
  await page.waitForSelector('.sheetpick .crumb-sheet');
  const transform = () => page.locator('.schematic-host .canvas').evaluate((e) => e.style.transform);
  const canvas = () => page.locator('.schematic-host').click({ position: { x: 6, y: 6 } });
  // Start on the first sheet, fitted, with the focus on the drawing.
  await page.locator('.sheetnav .sl-row').nth(1).click();
  await canvas();
  await page.keyboard.press('f');

  step('the palette lists every key of the view, with its chord');
  await page.keyboard.press('Control+Shift+P');
  await page.waitForSelector('.qp');
  await page.keyboard.type('Schematic');
  await page.waitForTimeout(150);
  const rows = await page.locator('.qp-row').allInnerTexts();
  console.log(`    palette rows for "Schematic": ${rows.map((r) => r.replace(/\n/g, ' ')).join(' | ')}`);
  await page.keyboard.press('Control+a');
  await page.keyboard.type('>');
  await page.waitForTimeout(150);
  const all = (await page.locator('.qp-row').allInnerTexts()).map((r) => r.replace(/\n/g, ' '));
  // A net is not selected here, so the commands that need one are not offered.
  for (const [title] of TITLES.filter(([t]) => !/selected|selection/.test(t))) {
    expect(all.some((r) => r.includes(title)), `the palette has "${title}"`);
  }
  expect(!all.some((r) => r.includes('Next pin of the selected net')), 'the pin commands are not offered with no net selected');
  await page.keyboard.type('fit the module');
  await page.waitForTimeout(150);
  const fit = (await page.locator('.qp-row').first().innerText()).replace(/\n/g, ' ');
  expect(fit.includes('Fit the module') && /\bF\b/.test(fit), `with its chord (${fit})`);
  await shot('palette-fit');
  await page.keyboard.press('Escape');

  step('the cheat sheet has a Schematic group with each key as its chord, and the view has no list of its own');
  await page.keyboard.press('?');
  await page.waitForSelector('.cheat[role=dialog]');
  const groups = await page.locator('.cheat h3').allTextContents();
  expect(groups.includes('Schematic'), `a Schematic group (${groups.join(', ')})`);
  expect(groups.filter((g) => g === 'Schematic').length === 1, 'once');
  const group = page.locator('.cheat section[data-group="Schematic"]');
  const text = await group.textContent();
  for (const [title] of TITLES) expect(text.includes(title), `the cheat sheet has "${title}"`);
  expect((await group.locator('kbd').allInnerTexts()).join(' ').includes('Shift'), 'the chords are in key caps');
  expect((await page.locator('.shortcuts').count()) === 0, 'there is no key list of the schematic\'s own');
  await shot('cheat-sheet');
  await page.keyboard.press('Escape');

  step('the toolbar\'s ? button opens the cheat sheet scrolled to the Schematic group');
  expect((await page.locator('.sheetbar [aria-label="Keyboard shortcuts"]').count()) === 1, 'the toolbar has the ? button');
  await page.locator('.sheetbar [aria-label="Keyboard shortcuts"]').click();
  await page.waitForSelector('.cheat[role=dialog]');
  await page.waitForTimeout(300);
  const inside = await page.evaluate(() => {
    const box = document.querySelector('.cheat-cols').getBoundingClientRect();
    const h = document.querySelector('.cheat section[data-group="Schematic"]').getBoundingClientRect();
    return { top: h.top >= box.top - 2 && h.top < box.bottom, bottom: h.bottom <= box.bottom + 2, left: h.left >= box.left - 2 && h.right <= box.right + 2, aimed: document.querySelector('.cheat section[data-group="Schematic"]').classList.contains('aimed') };
  });
  expect(inside.top && inside.bottom && inside.left, `the Schematic group is in view (${JSON.stringify(inside)})`);
  expect(inside.aimed, 'and marked');
  await shot('cheat-sheet-from-button');
  await page.keyboard.press('Escape');
  expect((await page.locator('.cheat').count()) === 0, 'Esc closes it');
  expect((await page.locator('.shortcuts').count()) === 0, 'and no list of the view\'s own opens');

  step('the keys work on the drawing: modules, fit, width, 100%, zoom, pan');
  await canvas();
  const first = await activeSheet(page);
  await page.keyboard.press(']');
  await page.waitForFunction((s) => document.querySelector('.sheetpick .crumb-sheet')?.textContent !== s, first);
  const second = await activeSheet(page);
  expect(second !== first, `] opens the next module (${first} to ${second})`);
  await page.keyboard.press('[');
  await page.waitForFunction((s) => document.querySelector('.sheetpick .crumb-sheet')?.textContent === s, first);
  expect((await activeSheet(page)) === first, '[ goes back');
  await page.keyboard.press('Shift+1');
  await page.waitForTimeout(100);
  expect((await zoomPercent(page)) === 100, 'Shift+1 is 100%');
  await page.keyboard.press('f');
  await page.waitForTimeout(100);
  const fitted = await zoomPercent(page);
  expect(fitted !== 100, `F fits (${fitted}%)`);
  await page.keyboard.press('Shift+1');
  await page.keyboard.press('0');
  await page.waitForTimeout(100);
  expect((await zoomPercent(page)) === fitted, '0 fits too');
  await page.keyboard.press('w');
  await page.waitForTimeout(100);
  const wide = await zoomPercent(page);
  expect(wide > 0, `W fits the width (${wide}%)`);
  await page.keyboard.press('f');
  await page.waitForTimeout(100);
  const z0 = await zoomPercent(page);
  await page.keyboard.press('+');
  await page.waitForTimeout(100);
  const z1 = await zoomPercent(page);
  expect(z1 > z0, `+ zooms in, once (${z0}% to ${z1}%)`);
  await page.keyboard.press('-');
  await page.waitForTimeout(100);
  expect((await zoomPercent(page)) === z0, '- zooms out by the same');
  const t0 = await transform();
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(100);
  const t1 = await transform();
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(100);
  expect(t1 !== t0 && (await transform()) !== t1, 'the arrow keys pan');

  step('Tab and Shift+Tab walk the pins of a selected net; Shift+2 zooms to it');
  await page.keyboard.press('f');
  const wire = await page.evaluate(() => {
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
  expect(!!wire, 'a named wire is on screen');
  await page.mouse.click(wire.x, wire.y);
  await page.waitForSelector('.selchip');
  const pinNow = () => page.evaluate(() => { const p = document.querySelector('svg.schematic .pin.cur'); return p ? `${p.getAttribute('data-ref')}.${p.getAttribute('data-pin')}` : null; });
  await page.keyboard.press('Tab');
  await page.waitForFunction(() => !!document.querySelector('svg.schematic .pin.cur'));
  const a = await pinNow();
  await page.keyboard.press('Tab');
  await page.waitForFunction((was) => { const p = document.querySelector('svg.schematic .pin.cur'); return p && `${p.getAttribute('data-ref')}.${p.getAttribute('data-pin')}` !== was; }, a);
  const b = await pinNow();
  expect(a !== b, `Tab walks the pins (${a} then ${b})`);
  await page.keyboard.press('Shift+Tab');
  await page.waitForFunction((was) => { const p = document.querySelector('svg.schematic .pin.cur'); return p && `${p.getAttribute('data-ref')}.${p.getAttribute('data-pin')}` === was; }, a);
  expect((await pinNow()) === a, 'Shift+Tab goes back');
  await page.keyboard.press('f');
  await page.waitForTimeout(100);
  const zf = await zoomPercent(page);
  await page.keyboard.press('Shift+2');
  await page.waitForTimeout(150);
  expect((await zoomPercent(page)) !== zf, 'Shift+2 zooms to the net');
  await page.keyboard.press('Control+Shift+P');
  await page.keyboard.type('pin of the selected');
  await page.waitForTimeout(150);
  const pinRows = (await page.locator('.qp-row').allInnerTexts()).map((r) => r.replace(/\n/g, ' '));
  expect(pinRows.length === 2 && pinRows.some((r) => r.includes('Next pin') && r.includes('Tab')), `with a net selected the palette offers the pin commands (${pinRows.join(' | ')})`);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');

  step('a text field keeps its keys: F typed in the part list is a letter, not Fit');
  if (!(await page.locator('.sinspector').count())) await page.getByRole('button', { name: 'Details' }).click();
  const field = page.locator('.sinspector input').first();
  expect((await field.count()) === 1, 'the inspector has a filter field');
  await canvas();
  await page.keyboard.press('Shift+1');
  await page.waitForTimeout(100);
  const z100 = await zoomPercent(page);
  await field.click();
  await page.keyboard.type('fw+-[]');
  await page.waitForTimeout(100);
  expect((await field.inputValue()) === 'fw+-[]', `the letters went into the field (${await field.inputValue()})`);
  expect((await zoomPercent(page)) === z100, 'the zoom did not change');
  expect((await activeSheet(page)) === first, 'nor the module');
  await field.fill('');

  step('away from the Schematic the keys are not offered or run');
  await section('bom');
  await page.keyboard.press('Control+Shift+P');
  await page.keyboard.type('fit the module');
  await page.waitForTimeout(150);
  const left = (await page.locator('.qp-row').allInnerTexts()).map((r) => r.replace(/\n/g, ' '));
  expect(!left.some((r) => r.includes('Fit the module')), `the palette has no Fit the module in the BOM (${left.join(' | ')})`);
  await page.keyboard.press('Escape');
  await page.keyboard.press('f');
  await section('schematic');
  expect((await zoomPercent(page)) === z100, 'F in the BOM did not touch the schematic\'s zoom');

  step('at 1920 x 1080 the ? button shows the Schematic group in view');
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.waitForTimeout(300);
  await page.locator('.sheetbar [aria-label="Keyboard shortcuts"]').click();
  await page.waitForSelector('.cheat[role=dialog]');
  await page.waitForTimeout(300);
  const visible = await page.evaluate(() => {
    const box = document.querySelector('.cheat-cols').getBoundingClientRect();
    const h = document.querySelector('.cheat section[data-group="Schematic"]').getBoundingClientRect();
    return h.top >= box.top - 2 && h.bottom <= box.bottom + 2 && h.left >= box.left - 2 && h.right <= box.right + 2;
  });
  expect(visible, 'the Schematic group is in view at 1920 x 1080');
  await shot('cheat-sheet-wide');
  await page.keyboard.press('Escape');
}
