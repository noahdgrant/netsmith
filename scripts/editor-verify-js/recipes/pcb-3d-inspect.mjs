// The 3D view's ways of looking (features/pcb-3d.md, "Tools"): x-ray (V) makes the board and the
// bodies see-through; a net picked in the nets list is lit on every layer and the rest dimmed;
// colour by kind and by height margin (a height map, with its legend); the height preview stands
// the parts of unknown height up in the picture only, under a "preview" label, and the coverage
// chip does not move; a finding picked in the problems dock flies the camera to it and lights its
// limit. The fixture has parts of several kinds under and beside a 3 mm lid: one too tall, one
// with little to spare, two of unknown height.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { fresh, openPcb, partOf, scene } from '../pcb-helpers.mjs';

export const fixture = 'inspect_3d';

const here = path.dirname(fileURLToPath(import.meta.url));
const BOARD = 'Inspected';
const HOST = '[data-testid=pcb-3d]';

const ask = (page, what, ...args) => page.evaluate(([sel, w, a]) => document.querySelector(sel).view3d[w](...a), [HOST, what, args]);

async function placeOf(page, path) {
  const at = await ask(page, 'project', path);
  if (!at) throw new Error(`the 3D view finds no point of the window that picks the part ${path}`);
  return at;
}

async function theme(page, label, id) {
  await page.keyboard.press('Control+Shift+P');
  await page.keyboard.type(`Theme: ${label}`);
  await page.keyboard.press('Enter');
  await page.waitForFunction((t) => document.documentElement.dataset.theme === t, id);
}

/** Let the view draw what changed (a frame, and a camera move of 180 ms). */
const settle = (page) => page.waitForTimeout(350);

export async function run(tools) {
  const { page, shot, step, expect } = tools;
  await fresh(tools, path.join(here, '../fixtures/inspect_3d.pcb.toml'));
  await openPcb(page, BOARD);
  await theme(page, 'Dark', 'dark');
  const s = await scene(page, BOARD);
  const tall = partOf(s, 'power_in');
  const led = partOf(s, 'led');
  const cap = partOf(s, 'input_cap');
  await page.getByTestId('pcb-3d-toggle').click();
  await page.waitForSelector(`${HOST}[data-ready]`, { timeout: 30000 });
  await page.locator(HOST).focus();
  // Nearer, so the pads and the bodies' colours show in the pictures.
  await page.keyboard.press('+');
  await page.keyboard.press('+');
  await settle(page);
  const chipBefore = await page.getByTestId('height-coverage').innerText();

  step('x-ray: the key V makes the board, the mask and the bodies see-through, and again puts them back');
  let capAt = await placeOf(page, cap.path);
  const solid = await ask(page, 'colourAt', capAt.x, capAt.y);
  await page.keyboard.press('v');
  await page.waitForFunction(() => document.querySelector('[data-testid=view-xray]')?.getAttribute('aria-pressed') === 'true');
  await settle(page);
  const seen = await ask(page, 'colourAt', capAt.x, capAt.y);
  expect(seen !== solid, `the capacitor's body is see-through in x-ray (${solid} to ${seen})`);
  await shot('xray');
  await page.keyboard.press('v');
  await page.waitForFunction(() => document.querySelector('[data-testid=view-xray]')?.getAttribute('aria-pressed') === 'false');
  await settle(page);
  expect((await ask(page, 'colourAt', capAt.x, capAt.y)) === solid, 'x-ray off: the body is solid again');

  step('a net picked in the nets list is lit on every layer, the rest dimmed');
  const nets = await ask(page, 'nets');
  await page.locator('.side-tabs button', { hasText: /^Nets/ }).click();
  const row = page.locator('.net-row', { hasText: 'VIN' }).first();
  await row.click();
  await page.waitForFunction(() => document.querySelector('.net-row.sel')?.innerText.includes('VIN'));
  await settle(page);
  // VIN is on the header's first pad (through the board, so on both layers), the regulator's input and the input capacitor.
  expect(nets.includes('VIN') && nets.includes('GND'), `the scene has the nets' copper (${nets.join(', ')})`);
  const lit = await ask(page, 'litReport');
  expect(lit.layers.includes('F.Cu') && lit.layers.includes('B.Cu'), `VIN is lit on every layer it has copper on: ${lit.layers.join(', ')}`);
  expect(lit.barrels >= 1, `its plated barrels are lit (${lit.barrels}; the board is not routed, so it has no vias: ${lit.vias})`);
  expect(lit.dimmed > 0, `the other copper is dimmed (${lit.dimmed} pieces)`);
  await shot('net-lit');
  await row.click();
  await page.waitForFunction(() => !document.querySelector('.net-row.sel'));
  // Off the row: its hover lights the net too.
  await page.mouse.move(5, 5);
  await page.waitForFunction(() => !document.documentElement.classList.contains('has-hover'));


  step('colour by kind: the menu (and the key C), and a legend of the kinds on the board');
  capAt = await placeOf(page, cap.path);
  const near = await ask(page, 'colourAt', capAt.x, capAt.y);
  await page.getByTestId('colour-by').selectOption('kind');
  await page.waitForSelector('[data-testid=kind-legend]');
  const kinds = await page.getByTestId('kind-legend').innerText();
  expect(/Connector\s*1/.test(kinds) && /Capacitor\s*2/.test(kinds) && /IC\s*1/.test(kinds) && /Diode\s*1/.test(kinds), `the legend counts the kinds: ${kinds.replace(/\n/g, ' | ')}`);
  expect(/Unknown height\s*2/.test(kinds), 'the parts of unknown height stay hatched, and are counted apart');
  await settle(page);
  const capKind = await ask(page, 'colourAt', capAt.x, capAt.y);
  expect(capKind !== near, `the capacitor takes its kind's colour (${near} to ${capKind})`);
  await shot('colour-kind');

  step('colour by height margin: a height map with its legend, green to amber to red, grey where no limit is over a part');
  await page.keyboard.press('c');
  await page.waitForSelector('[data-testid=margin-legend]');
  const margin = await page.getByTestId('margin-legend').innerText();
  expect(/Over\s*1/.test(margin) && /Under 1 mm\s*1/.test(margin) && /1 mm\+\s*1/.test(margin) && /No limit over it\s*2/.test(margin), `the bands: ${margin.replace(/\n/g, ' | ')}`);
  const note = await page.getByTestId('margin-note').innerText();
  expect(note.includes('1 mm to spare') && note.includes('not a check'), `the legend says what green means: ${note}`);
  await settle(page);
  const ledAt = await placeOf(page, led.path);
  const [capM, ledM] = [await ask(page, 'colourAt', capAt.x, capAt.y), await ask(page, 'colourAt', ledAt.x, ledAt.y)];
  expect(capM !== ledM, `plenty to spare and little to spare are two colours (${capM}, ${ledM})`);
  await page.locator(HOST).hover({ position: { x: 5, y: 5 } });
  await shot('height-map');
  await page.keyboard.press('c');
  await page.waitForSelector('[data-testid=height-legend]');

  step('the height preview: off at the start; on, the parts of unknown height stand up under a "preview" label, and the coverage does not move');
  expect((await page.getByTestId('height-preview').getAttribute('aria-checked')) === 'false', 'off by default');
  expect((await ask(page, 'previewCount')) === 0, 'nothing stood up');
  await page.getByTestId('preview-mm').fill('2.5');
  await page.getByTestId('preview-mm').press('Enter');
  await page.getByTestId('height-preview').click();
  await page.waitForSelector('[data-testid=preview-label]');
  const label = await page.getByTestId('preview-label').innerText();
  expect(/PREVIEW/i.test(label) && label.includes('2 parts of unknown height drawn 2.5 mm tall') && label.includes('not a height'), `the label: ${label.replace(/\n/g, ' ')}`);
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.previewCount() === 2, HOST);
  expect(true, 'the two parts of unknown height are stood up');
  expect((await page.getByTestId('height-coverage').innerText()) === chipBefore, `the coverage chip is the same: ${chipBefore}`);
  const parts3d = await ask(page, 'parts');
  expect(parts3d.filter((p) => p.heightMm === null).length === 2, 'the parts of unknown height still have no height');
  await settle(page);
  await shot('preview');
  await page.keyboard.press('h');
  await page.waitForFunction(() => !document.querySelector('[data-testid=preview-label]'));
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.previewCount() === 0, HOST);
  expect(true, 'the key H turns it off, and nothing is left standing');

  step('x-ray and a lit net with the section plane and with the layers apart: the caps still show, the net stays lit');
  await page.locator('.side-tabs button', { hasText: /^Nets/ }).click();
  await page.locator('.net-row', { hasText: 'VIN' }).first().click();
  await page.mouse.move(5, 5);
  await page.locator(HOST).focus();
  await page.keyboard.press('v');
  await page.keyboard.press('y');
  await page.waitForSelector('[data-testid=section-panel]');
  await page.getByTestId('section-at').fill('12');
  await page.getByTestId('section-at').press('Enter');
  await page.getByTestId('view-front').click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.preset() === 'front', HOST);
  await settle(page);
  const caps = await ask(page, 'caps');
  expect(caps.includes('substrate') || caps.some((c) => /core|prepreg|substrate/.test(c)), `x-ray with a section keeps the laminate's caps: ${caps.join(', ')}`);
  expect((await ask(page, 'litReport')).layers.length === 2, 'VIN is still lit on both layers through the cut');
  const cutSample = await ask(page, 'sample');
  expect(cutSample.drawn > 0.02, `the cut is drawn (${(cutSample.drawn * 100).toFixed(1)} % of the canvas)`);
  await shot('xray-section');
  await page.keyboard.press('s');
  await page.waitForSelector('[data-testid=section-panel]', { state: 'detached' });
  await page.keyboard.press('i');
  await page.keyboard.press('e');
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.exploded(), HOST);
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.slots().some((s) => Math.abs(s.now - (s.zTop + s.zBottom) / 2) > 1), HOST, { timeout: 15000 });
  expect((await ask(page, 'litReport')).layers.length === 2, 'the net stays lit with the layers apart');
  await shot('xray-exploded');
  await page.keyboard.press('e');
  await page.waitForFunction((sel) => !document.querySelector(sel).view3d.exploded(), HOST);
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.slots().every((s) => Math.abs(s.now - (s.zTop + s.zBottom) / 2) < 1e-6), HOST, { timeout: 15000 });
  await page.keyboard.press('v');
  await page.locator('.net-row', { hasText: 'VIN' }).first().click();
  await page.mouse.move(5, 5);
  await page.waitForFunction(() => !document.querySelector('.net-row.sel'));

  step('at 1000 px wide, with measure, the section and the preview on, no two of the bars overlap');
  const size = page.viewportSize();
  await page.setViewportSize({ width: 1000, height: 760 });
  await page.locator(HOST).focus();
  await page.keyboard.press('m');
  await page.waitForSelector('[data-testid=measure-panel]');
  await page.keyboard.press('s');
  await page.waitForSelector('[data-testid=section-panel]');
  await page.keyboard.press('h');
  await page.waitForSelector('[data-testid=preview-label]');
  await settle(page);
  const boxes = await page.evaluate(() =>
    ['.v3-bar', '.v3-tools', '.v3-info', '[data-testid=section-panel]', '[data-testid=measure-panel]', '.v3-inspect', '.v3-wrap .zoombar', '.v3-preview-tag']
      .map((sel) => {
        const el = document.querySelector(sel);
        if (!el) return null;
        const r = el.getBoundingClientRect();
        return { sel, x0: r.left, y0: r.top, x1: r.right, y1: r.bottom };
      })
      .filter(Boolean),
  );
  const hits = [];
  for (let i = 0; i < boxes.length; i++)
    for (let j = i + 1; j < boxes.length; j++) {
      const [a, b] = [boxes[i], boxes[j]];
      if (a.x0 < b.x1 - 1 && b.x0 < a.x1 - 1 && a.y0 < b.y1 - 1 && b.y0 < a.y1 - 1) hits.push(`${a.sel} and ${b.sel}`);
    }
  expect(boxes.length === 8, `every bar is there (${boxes.map((b) => b.sel).join(', ')})`);
  expect(hits.length === 0, `no overlap: ${hits.join('; ') || 'none'}`);
  await shot('bars-1000');
  await page.keyboard.press('h');
  await page.keyboard.press('s');
  await page.keyboard.press('Escape');
  await page.setViewportSize(size);
  await settle(page);

  step('a finding picked in the problems dock flies the camera to it and lights its limit');
  await page.keyboard.press('i');
  await settle(page);
  await page.keyboard.press('Control+j');
  await page.waitForSelector('[data-testid=dock]');
  const problem = page.locator('[data-testid=problem]', { hasText: tall.refdes }).filter({ hasText: 'lid' }).first();
  await problem.waitFor({ timeout: 20000 });
  const before = await placeOf(page, tall.path);
  await problem.click();
  await page.waitForFunction((sel) => document.querySelector(sel).view3d.focused().includes('lid:top'), HOST);
  await page.waitForTimeout(1000);
  expect((await page.getByTestId('pcb-3d').count()) === 1, 'the PCB stays in 3D');
  const after = await placeOf(page, tall.path);
  expect(Math.hypot(after.x - before.x, after.y - before.y) > 10, `the camera moved to the finding (${before.x},${before.y} to ${after.x},${after.y})`);
  await page.waitForFunction((t) => document.querySelector('[data-testid=part-inspector] h3')?.innerText.startsWith(t), tall.designator ?? tall.refdes);
  await shot('finding-focused');

  step('the light theme of the height map');
  await page.keyboard.press('Control+j');
  await page.getByTestId('colour-by').selectOption('margin');
  await theme(page, 'Light', 'light');
  await page.keyboard.press('Escape');
  await settle(page);
  await shot('height-map-light');
}
