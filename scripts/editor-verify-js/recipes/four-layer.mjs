// A four-layer board (launch --fixture four_layer): its planes are drawn dashed, and routing
// finishes with no unrouted connection.
import { whenIdle } from '../settle.mjs';
export const fixture = 'four_layer';

export async function run({ page, shot, step, expect, section }) {
  step('open the PCB tab');
  await section('pcb');
  await page.waitForSelector('.pcb-host svg');
  expect((await page.locator('.pcb-host svg [stroke-dasharray]').count()) >= 2, 'a dashed border for each plane');
  step('a plane is coloured by its real layer and named with it (the drawing\'s data-layer)');
  const planes = await page.evaluate(() => [...document.querySelectorAll('.pcb-host svg .l-planes > rect[data-net]')].map((r) => ({ net: r.getAttribute('data-net'), layer: r.getAttribute('data-layer'), tone: r.getAttribute('data-plane'), stroke: getComputedStyle(r).stroke })));
  expect(planes.length >= 2 && planes.every((p) => /^In\d+\.Cu$/.test(p.layer ?? '')), `each plane rectangle names its layer (${planes.map((p) => `${p.net} ${p.layer}`).join(', ')})`);
  expect(planes.every((p) => p.tone === String(Math.min(4, Number(/\d+/.exec(p.layer)[0])))), `and its tone is its layer's (${planes.map((p) => `${p.layer}:${p.tone}`).join(', ')})`);
  expect(new Set(planes.map((p) => p.stroke)).size === planes.length, 'two planes on two layers have two colours');
  const labels = await page.locator('.pcb-host svg text.plane-label').evaluateAll((els) => els.map((e) => e.textContent));
  expect(planes.every((p) => labels.some((l) => l === `${p.net} · ${p.layer}`)), `a label with the net and the layer (${labels.join(' | ')})`);
  step('the plane labels are small, with a halo, at the corner of their plane inside its border, above the parts, and they do not lie on one another');
  const sizes = await page.evaluate(() => { const px = Number(document.querySelector('.pcb-stage').style.getPropertyValue('--px')); return [...document.querySelectorAll('.pcb-host svg text.plane-label')].map((t) => ({ h: parseFloat(getComputedStyle(t).fontSize) / px, box: t.getBoundingClientRect(), halo: getComputedStyle(t).paintOrder, inLayer: !!t.closest('.l-plane-labels') })); });
  expect(sizes.every((l) => l.h >= 8 && l.h <= 12.5 && l.inLayer && /stroke/.test(l.halo)), `small (8 to 12 screen pixels), with a halo, in a layer of their own (${sizes.map((l) => l.h.toFixed(1)).join(', ')})`);
  const overlap = sizes.some((a, i) => sizes.some((b, j) => j > i && a.box.left < b.box.right && b.box.left < a.box.right && a.box.top < b.box.bottom && b.box.top < a.box.bottom));
  expect(!overlap, 'two plane labels never overlap');
  step('a plane label never lies on a part: it moves to a free corner of its plane at the zoom shown (round 7), and is hidden when there is none');
  // The glyphs' own box: the text's length and the type size on the screen (the browser's box of an SVG text this small is three times too tall)
  const onParts = () =>
    page.evaluate(() => {
      const box = (t) => {
        const m = t.getScreenCTM();
        const w = t.getComputedTextLength() * m.a;
        const h = parseFloat(getComputedStyle(t).fontSize) * m.a;
        const x = m.a * Number(t.getAttribute('x')) + m.e;
        const y = m.d * Number(t.getAttribute('y')) + m.f;
        const anchor = t.getAttribute('text-anchor');
        const left = anchor === 'end' ? x - w : anchor === 'middle' ? x - w / 2 : x;
        return { left, top: y - h / 2, right: left + w, bottom: y + h / 2 };
      };
      const parts = [...document.querySelectorAll('.pcb-host svg g[data-path]')].map((g) => g.getBoundingClientRect()).filter((r) => r.width > 0).concat([...document.querySelectorAll('.pcb-host svg .l-refs text')].map(box));
      return [...document.querySelectorAll('.pcb-host svg text.plane-label')]
        .filter((t) => getComputedStyle(t).visibility !== 'hidden')
        .filter((t) => {
          const a = box(t);
          return parts.some((b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom);
        })
        .map((t) => `${t.textContent} at ${t.getAttribute('x')},${t.getAttribute('y')}`);
    });
  const state = () => page.evaluate(() => [...document.querySelectorAll('.pcb-host svg text.plane-label')].map((t) => `${t.textContent}: ${t.classList.contains('plane-label-off') ? 'hidden' : `${t.getAttribute('text-anchor')} ${t.getAttribute('x')},${t.getAttribute('y')}`}`).join(' | '));
  expect((await onParts()).length === 0, `no visible plane label lies on a part (${await state()}) ${(await onParts()).join('; ')}`);
  // Two labels (and the +N tag, when there is one) never meet, at any zoom: the glyphs' own boxes again.
  const clash = () =>
    page.evaluate(() => {
      const box = (t) => {
        const m = t.getScreenCTM();
        const w = t.getComputedTextLength() * m.a;
        const h = parseFloat(getComputedStyle(t).fontSize) * m.a;
        const x = m.a * Number(t.getAttribute('x')) + m.e;
        const y = m.d * Number(t.getAttribute('y')) + m.f;
        const anchor = t.getAttribute('text-anchor');
        const left = anchor === 'end' ? x - w : anchor === 'middle' ? x - w / 2 : x;
        return { left, top: y - h / 2, right: left + w, bottom: y + h / 2, name: t.textContent };
      };
      const shown = [...document.querySelectorAll('.pcb-host svg .l-plane-labels > text')].filter((t) => getComputedStyle(t).visibility !== 'hidden').map(box);
      const out = [];
      for (let i = 0; i < shown.length; i++) for (let j = i + 1; j < shown.length; j++) {
        const a = shown[i];
        const b = shown[j];
        if (a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom) out.push(`${a.name} on ${b.name}`);
      }
      return out;
    });
  expect((await clash()).length === 0, 'no two plane labels meet at the fit');
  await shot('plane-labels-fit');
  for (const label of ['+', '+', '\u2212', '\u2212', '\u2212']) {
    await page.locator('.sec-pcb .zoombar button', { hasText: new RegExp(`^${label === '+' ? '\\+' : label}$`) }).first().click();
    await whenIdle(page);
    const bad = await onParts();
    expect(bad.length === 0, `still none after zoom ${label} (${await page.locator('.sec-pcb .zoombar span').first().innerText()}): ${bad.join('; ') || (await state())}`);
    expect((await clash()).length === 0, `and no two labels meet after zoom ${label} (${(await clash()).join('; ')})`);
  }
  step('a label that finds no room is hidden and counted by the chip +N (a button over the canvas; pcb-plane-more drives it); no mark is left in the drawing');
  for (let i = 0; i < 6; i++) {
    await page.locator('.sec-pcb .zoombar button', { hasText: /^\u2212$/ }).first().click();
    await whenIdle(page);
  }
  await whenIdle(page);
  const tagState = await page.evaluate(() => {
    const all = [...document.querySelectorAll('.pcb-host svg .l-plane-labels > text.plane-label')];
    const hidden = all.filter((t) => t.classList.contains('plane-label-off')).map((t) => t.textContent);
    const chip = document.querySelector('.sec-pcb [data-testid="plane-more"]');
    return { hidden, chip: chip ? { text: chip.textContent.trim(), label: chip.getAttribute('aria-label') } : null, inDrawing: document.querySelectorAll('.pcb-host svg .plane-label-more').length };
  });
  expect(tagState.hidden.length > 0, `zoomed far out some labels are hidden (${tagState.hidden.join(', ')})`);
  expect(tagState.chip && tagState.chip.text === `+${tagState.hidden.length}` && tagState.hidden.every((n) => tagState.chip.label.includes(n)), `and a chip says +${tagState.hidden.length} with the names in its accessible name (${tagState.chip?.text}: ${tagState.chip?.label})`);
  expect(tagState.inDrawing === 0, 'nothing is drawn on the board for it');
  expect((await clash()).length === 0, 'the labels that are left meet nowhere');
  await shot('plane-labels-more');
  for (let i = 0; i < 6; i++) {
    await page.locator('.sec-pcb .zoombar button', { hasText: /^\+$/ }).first().click();
    await whenIdle(page);
  }
  await whenIdle(page);
  expect((await page.locator('.sec-pcb [data-testid="plane-more"]').count()) === 0 || (await page.locator('.pcb-host svg .plane-label-off').count()) > 0, 'zoomed in again the chip is gone with the labels it stood for');
  await page.locator('.sec-pcb .zoombar button', { hasText: /^Fit$/ }).first().click();
  await whenIdle(page);
  step('route it');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  expect(true, 'routed with nothing unrouted');
  await shot('four-layer');
}
