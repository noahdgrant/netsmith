// The chip `+N` of the PCB view: planes whose label has no room at this zoom. It is a button pinned to the corner of the canvas (the same
// size at every zoom, in the page's tokens), it names the planes, opens a list a keyboard reaches and zooms the board to the plane chosen.
export const fixture = 'four_layer';

const THEMES = ['dark', 'light', 'hc-dark', 'hc-light'];

export async function run({ page, shot, step, expect, section }) {
  const zoomBtn = (label) => page.locator('.sec-pcb .zoombar button', { hasText: new RegExp(`^${label === '+' ? '\\+' : label}$`) }).first();
  const zoomOut = async (n) => {
    for (let i = 0; i < n; i++) {
      await zoomBtn('−').click();
      await page.waitForTimeout(120);
    }
    await page.waitForTimeout(300);
  };
  const chip = () => page.locator('.sec-pcb [data-testid="plane-more"]');
  const hiddenNames = () => page.evaluate(() => [...document.querySelectorAll('.sec-pcb .pcb-host svg .l-plane-labels > text.plane-label.plane-label-off')].map((t) => t.textContent));
  const percent = async () => Number((await page.locator('.sec-pcb .zoombar span').first().innerText()).replace('%', ''));

  step('open the PCB at Fit: every plane has its label, so there is no chip and nothing is drawn for one');
  await section('pcb');
  await page.waitForSelector('.pcb-host svg .l-plane-labels');
  await zoomBtn('Fit').click();
  await page.waitForTimeout(400);
  expect((await chip().count()) === 0, 'no chip while every label has its place');
  expect((await page.locator('.pcb-host svg .plane-label-more').count()) === 0, 'and no mark inside the drawing (the tag moved out of it)');

  step('zoomed far out, the planes with no room are counted by a chip that is a button with an accessible name');
  await zoomOut(6);
  const names = await hiddenNames();
  expect(names.length > 0, `some labels are hidden (${names.join(', ')})`);
  await chip().waitFor();
  const text = (await chip().innerText()).trim();
  expect(text === `+${names.length}`, `the chip says +${names.length} (${text})`);
  const label = await chip().getAttribute('aria-label');
  expect(names.every((n) => label.includes(n)) && /Show the list/.test(label), `its name lists the planes (${label})`);
  expect((await chip().evaluate((e) => e.tagName)) === 'BUTTON' && (await chip().getAttribute('aria-haspopup')) === 'menu', 'it is a button that opens a menu');
  await shot('chip');

  step('it is as large at any zoom, inside the canvas, in the corner, and does not cover the zoom bar');
  const geo = () =>
    page.evaluate(() => {
      const c = document.querySelector('.sec-pcb [data-testid="plane-more"]').getBoundingClientRect();
      const host = document.querySelector('.sec-pcb .panzoom').getBoundingClientRect();
      const z = document.querySelector('.sec-pcb .zoombar').getBoundingClientRect();
      const fs = parseFloat(getComputedStyle(document.querySelector('.sec-pcb [data-testid="plane-more"] .pm-n')).fontSize);
      return { w: c.width, h: c.height, fs, inside: c.left >= host.left && c.right <= host.right && c.top >= host.top && c.bottom <= host.bottom, clear: c.right <= z.left || c.left >= z.right, near: c.left - host.left < 40 && host.bottom - c.bottom < 40 };
    });
  const far = await geo();
  await zoomBtn('−').click();
  await page.waitForTimeout(300);
  const farther = await geo();
  expect(Math.abs(far.w - farther.w) < 0.5 && Math.abs(far.h - farther.h) < 0.5, `the same size at two zooms (${far.w}x${far.h}, ${farther.w}x${farther.h})`);
  expect(far.fs >= 11 && far.h >= 24, `readable and a target of at least 24 px (${far.fs}px type, ${far.h}px high)`);
  expect(far.inside && far.clear && far.near, 'in the bottom left corner of the canvas, clear of the zoom bar');

  step('a click opens the list: one row for each hidden plane, the first focused; the board outlines the plane the pointer is on');
  await chip().click();
  await page.waitForSelector('[data-testid="plane-more-list"]');
  const items = await page.locator('[data-testid="plane-more-item"]').evaluateAll((els) => els.map((e) => ({ text: e.querySelector('.pm-name').textContent, net: e.getAttribute('data-net'), layer: e.getAttribute('data-layer') })));
  const now = await hiddenNames();
  expect(items.length === now.length && now.every((n) => items.some((i) => i.text === n)), `the list names the hidden planes (${items.map((i) => i.text).join(', ')})`);
  expect((await chip().getAttribute('aria-expanded')) === 'true', 'aria-expanded is true');
  expect(await page.evaluate(() => document.activeElement?.matches('[data-testid="plane-more-item"]')), 'the first row has the focus');
  await shot('list');
  const second = page.locator('[data-testid="plane-more-item"]').nth(items.length - 1);
  await second.hover();
  const peeked = await page.evaluate(() => [...document.querySelectorAll('.pcb-host svg .l-planes > rect.plane-peek')].map((r) => `${r.getAttribute('data-net')}|${r.getAttribute('data-layer')}`));
  const want = items[items.length - 1];
  expect(peeked.length === 1 && peeked[0] === `${want.net}|${want.layer}`, `pointing at ${want.text} outlines that plane (${peeked.join(', ')})`);
  await shot('peek');

  step('the keys: arrows move in the list and do not move a part, Escape closes and gives the focus back to the chip');
  const undoBefore = await page.locator('[data-testid="undo"]').isDisabled();
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  expect(await page.evaluate(() => document.activeElement?.matches('[data-testid="plane-more-item"]:last-of-type')), 'End goes to the last row');
  expect((await page.locator('[data-testid="undo"]').isDisabled()) === undoBefore, 'no part was nudged by an arrow');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(150);
  expect((await page.locator('[data-testid="plane-more-list"]').count()) === 0, 'Escape closed the list');
  expect(await page.evaluate(() => document.activeElement?.matches('[data-testid="plane-more"]')), 'the focus is back on the chip');
  expect((await page.locator('.pcb-host svg rect.plane-peek').count()) === 0, 'and no plane is outlined any more');
  await page.keyboard.press('Enter');
  await page.waitForSelector('[data-testid="plane-more-list"]');
  expect(true, 'Enter on the chip opens it again');

  step('choosing a plane zooms the board to it, so its label comes back');
  const before = await percent();
  const choice = items[0];
  await page.keyboard.press('Home');
  await page.keyboard.press('Enter');
  await page.waitForTimeout(700);
  const after = await percent();
  expect(after > before, `the zoom grew (${before}% to ${after}%)`);
  const back = await page.evaluate((net) => {
    const t = [...document.querySelectorAll('.sec-pcb .pcb-host svg .l-plane-labels > text.plane-label')].find((e) => e.getAttribute('data-net') === net);
    return !!t && !t.classList.contains('plane-label-off');
  }, choice.net);
  expect(back, `the label of ${choice.text} is drawn again`);
  expect((await page.locator('[data-testid="plane-more-list"]').count()) === 0, 'the list closed');
  await shot('zoomed-to-plane');

  step('with the planes layer switched off no label is hidden for want of room: no chip, no labels');
  await zoomBtn('Fit').click();
  await page.waitForTimeout(300);
  await zoomOut(6);
  await chip().waitFor();
  await page.locator('.sec-pcb details.layers summary').click();
  await page.locator('.sec-pcb .layer-list label', { hasText: 'Planes' }).locator('input').uncheck();
  await page.waitForTimeout(400);
  expect((await chip().count()) === 0, 'the chip is gone');
  expect(await page.evaluate(() => getComputedStyle(document.querySelector('.sec-pcb .pcb-host svg .l-plane-labels')).display === 'none'), 'and the labels with the planes');
  await page.locator('.sec-pcb .layer-list label', { hasText: 'Planes' }).locator('input').check();
  await page.waitForTimeout(400);
  await page.locator('.sec-pcb details.layers summary').click();
  await chip().waitFor();
  expect(true, 'switched on again, the chip is back');

  step('in every theme: the chip and its list are legible (type 4.5:1 against the chip) and take the theme\'s colours');
  await chip().click();
  await page.waitForSelector('[data-testid="plane-more-list"]');
  for (const id of THEMES) {
    await page.evaluate((t) => { document.documentElement.dataset.theme = t; }, id);
    await page.waitForTimeout(200);
    const r = await page.evaluate(() => {
      const lum = (c) => {
        const m = c.match(/[\d.]+/g).map(Number);
        const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
        return 0.2126 * f(m[0]) + 0.7152 * f(m[1]) + 0.0722 * f(m[2]);
      };
      const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
      const bg = (e) => getComputedStyle(e).backgroundColor;
      const chipEl = document.querySelector('.sec-pcb [data-testid="plane-more"]');
      const list = document.querySelector('.sec-pcb [data-testid="plane-more-list"]');
      const out = { chip: ratio(getComputedStyle(chipEl.querySelector('.pm-n')).color, bg(chipEl)) };
      out.name = ratio(getComputedStyle(list.querySelector('.pm-name')).color, bg(list));
      out.act = ratio(getComputedStyle(list.querySelector('.pm-act')).color, bg(list));
      out.head = ratio(getComputedStyle(list.querySelector('.pm-head')).color, bg(list));
      out.swatch = list.querySelector('.pm-sw').getBoundingClientRect().width > 0 && getComputedStyle(list.querySelector('.pm-sw')).backgroundColor !== 'rgba(0, 0, 0, 0)';
      return out;
    });
    expect(r.chip >= 4.5 && r.name >= 4.5 && r.act >= 4.5 && r.head >= 4.5 && r.swatch, `${id}: chip ${r.chip.toFixed(1)}, name ${r.name.toFixed(1)}, hint ${r.act.toFixed(1)}, head ${r.head.toFixed(1)}:1`);
    await shot(`theme-${id}`);
  }
}
