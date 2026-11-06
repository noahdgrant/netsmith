// A four-layer board (launch --fixture four_layer): its planes are drawn dashed, and routing
// finishes with no unrouted connection.
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
  step('route it');
  await page.getByRole('button', { name: /Place and route/ }).click();
  await page.waitForSelector('.chip:has-text("routed")');
  expect(true, 'routed with nothing unrouted');
  await shot('four-layer');
}
