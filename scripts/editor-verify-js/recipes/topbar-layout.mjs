// The top bar, the section tabs and the status bar never sit on one another or leave the window, at any width, size and density.
// Round 7: at 125 % in a window up to 1366 px wide the "Go to anything" box covered the theme, shortcuts and settings buttons (it had a
// width of its own and the buttons were in another box), and no recipe looked at boxes. This one measures them: every control of the
// three bars, in each combination, with nothing picked, with a part picked (its chip is the widest thing on the left) and with a net picked
// in the Schematic (the chip has its pin steppers), and asserts that no two boxes of a bar meet, that none is cut by the window or by a
// box that clips it, and that the chip does not shorten the name it has room for.
import { whenIdle } from '../settle.mjs';
export const fixture = 'two_boards';

const WIDTHS = [760, 880, 1000, 1100, 1200, 1280, 1366, 1440, 1600, 1920];
// with something picked the chip is the widest thing in the bar, so every 20 px (a threshold of the bar's folding that was a few rem off would show)
const FINE = Array.from({ length: 48 }, (_, i) => 760 + i * 20).concat([2200, 2560]);
const SIZES = [90, 100, 125];
const DENSITIES = ['comfortable', 'compact'];

// Runs in the page: the boxes of the controls of one bar, the pairs that meet, the ones that are cut, and what the chip shortens.
function audit([vw, vh]) {
  const bars = {
    'top bar': '.topbar .brand, .topbar .boardpick, .topbar .boardpick-empty, .topbar .selchip-main, .topbar .selchip-btn, .topbar .cmdk, .topbar .tb-ghosts > *, .topbar .actions > *',
    'section tabs': '.pane-strip > .tabbar > .tab, .pane-strip > .pane-btn',
    'status bar': '.statusbar .item',
  };
  const label = (el) => {
    const t = (el.getAttribute('aria-label') || el.getAttribute('data-testid') || el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 22);
    return `${String(el.className?.baseVal ?? el.className).split(' ').filter(Boolean).slice(0, 2).join('.')}${t ? ` "${t}"` : ''}`;
  };
  const shown = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width <= 1.5 || r.height <= 1.5) return false;
    const s = getComputedStyle(el);
    return s.display !== 'none' && s.visibility !== 'hidden';
  };
  // the box a control is cut to: the window, and every ancestor that clips (overflow other than visible)
  const clipOf = (el) => {
    let c = { l: 0, t: 0, r: vw, b: vh, by: 'the window' };
    for (let p = el.parentElement; p && p !== document.documentElement; p = p.parentElement) {
      const s = getComputedStyle(p);
      if (s.overflowX === 'visible' && s.overflowY === 'visible') continue;
      const q = p.getBoundingClientRect();
      const n = { l: Math.max(c.l, q.left), t: Math.max(c.t, q.top), r: Math.min(c.r, q.right), b: Math.min(c.b, q.bottom) };
      if (n.l !== c.l || n.r !== c.r || n.t !== c.t || n.b !== c.b) c = { ...n, by: label(p) };
    }
    return c;
  };
  // the words are shortened (an ellipsis) when the text is wider than its box, by a tenth of a pixel too
  const shortened = (e) => {
    const r = document.createRange();
    r.selectNodeContents(e);
    return r.getBoundingClientRect().width > e.getBoundingClientRect().width + 0.25;
  };
  const problems = [];
  const boxes = {};
  for (const [bar, sel] of Object.entries(bars)) {
    const els = [...document.querySelectorAll(sel)].filter(shown);
    // a box that holds another one (a chip's parts are listed, the chip is not) is left out
    const rows = els.map((el) => {
      const r = el.getBoundingClientRect();
      return { el, name: label(el), l: r.left, t: r.top, r: r.right, b: r.bottom };
    });
    boxes[bar] = rows.length;
    for (let i = 0; i < rows.length; i++) {
      const a = rows[i];
      const c = clipOf(a.el);
      if (a.l < c.l - 1 || a.r > c.r + 1 || a.t < c.t - 1 || a.b > c.b + 1) problems.push(`${bar}: ${a.name} is cut by ${c.by} (${a.l.toFixed(0)}..${a.r.toFixed(0)} of ${c.l.toFixed(0)}..${c.r.toFixed(0)})`);
      for (let j = i + 1; j < rows.length; j++) {
        const b = rows[j];
        if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
        const w = Math.min(a.r, b.r) - Math.max(a.l, b.l);
        const h = Math.min(a.b, b.b) - Math.max(a.t, b.t);
        if (w > 1 && h > 1) problems.push(`${bar}: ${a.name} meets ${b.name} by ${w.toFixed(0)} x ${h.toFixed(0)} px`);
      }
    }
  }
  // the chip's name and facts are whole unless the bar is out of room: there is none beside the chip, and go to anything is as narrow as it goes
  const chip = document.querySelector('.topbar .selchip');
  let chipNote = '';
  if (chip && shown(chip)) {
    const left = chip.closest('.tb-left').getBoundingClientRect();
    const room = left.right - chip.getBoundingClientRect().right;
    const cmdk = document.querySelector('.topbar .cmdk');
    const spare = getComputedStyle(cmdk).flexShrink === '0' ? 0 : cmdk.getBoundingClientRect().width - (parseFloat(getComputedStyle(cmdk).minWidth) || 0); // as an icon (a narrow bar) it has nothing more to give
    const cut = [...chip.querySelectorAll('.selchip-title, .selchip-sub')].filter(shown).filter(shortened).map(label);
    chipNote = `room ${room.toFixed(0)} px, go to anything ${spare.toFixed(0)} px over its least, cut: ${cut.join(', ') || 'nothing'}`;
    if (cut.length && (room > 12 || spare > 2)) problems.push(`the chip shortens ${cut.join(', ')} while the bar has room (${chipNote})`);
  }
  return { problems, boxes, chipNote, overflow: document.documentElement.scrollWidth > innerWidth };
}

export async function run({ page, shot, step, expect, url, section }) {
  const board = 'TwoBoards/power';
  const detail = await (async () => {
    await page.selectOption('.boardpick select', board);
    return page.evaluate(async (b) => (await fetch(`/api/board?name=${encodeURIComponent(b)}`)).json(), board);
  })();
  // the part with the longest value is the widest chip; the net is one the Schematic can step along
  const part = [...detail.parts].sort((x, y) => String(y.value ?? '').length - String(x.value ?? '').length)[0];
  const net = detail.nets.find((n) => n.named && n.pins.length > 1);
  const pick = async (text) => {
    await page.keyboard.press('Control+p');
    await page.waitForSelector('.qp[role=dialog]');
    await page.keyboard.type(text);
    await page.waitForSelector('.qp-row[role=option]');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.qp', { state: 'detached' });
    await page.waitForSelector('.selchip');
    await whenIdle(page);
  };

  const failures = [];
  let measured = 0;
  const sweep = async (tag, widths = WIDTHS) => {
    for (const w of widths) {
      await page.setViewportSize({ width: w, height: 800 });
      await whenIdle(page);
      const r = await page.evaluate(audit, [w, 800]);
      measured++;
      for (const p of r.problems) failures.push(`${tag} ${w}px: ${p}`);
      if (r.overflow) failures.push(`${tag} ${w}px: the document scrolls sideways`);
    }
  };

  step(`the boxes of the three bars, ${WIDTHS.length} widths from ${WIDTHS[0]} to ${WIDTHS[WIDTHS.length - 1]} px (every 20 px with a pick), sizes ${SIZES.join('/')} %, comfortable and compact, with nothing picked, a part, a net`);
  for (const scale of SIZES) {
    for (const density of DENSITIES) {
      await page.setViewportSize({ width: 1500, height: 900 });
      await page.evaluate((s) => localStorage.setItem('netsmith.settings', JSON.stringify(s)), { density, scale, motion: 'system' });
      await page.reload();
      await page.waitForSelector('.boardpick select option', { state: 'attached' });
      await page.selectOption('.boardpick select', board);
      await section('schematic');
      await whenIdle(page);
      const tag = `${scale} % ${density}`;
      await sweep(`${tag}, nothing picked,`);
      await pick(part.refdes.toLowerCase());
      await sweep(`${tag}, ${part.refdes} picked,`, FINE);
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => !document.querySelector('.selchip'));
      await pick(`#${net.name}`);
      await sweep(`${tag}, net ${net.name} picked,`, FINE);
      await page.keyboard.press('Escape');
      await page.waitForFunction(() => !document.querySelector('.selchip'));
    }
  }
  for (const f of failures) console.log(`    ${f}`);
  expect(failures.length === 0, `no box meets another or is cut, in ${measured} combinations (${failures.length} problems${failures.length ? `, the first: ${failures[0]}` : ''})`);

  step('the first look at the combination that overlapped: 125 %, 1280 x 800, compact; and the same with a part picked');
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.evaluate((s) => localStorage.setItem('netsmith.settings', JSON.stringify(s)), { density: 'compact', scale: 125, motion: 'system' });
  await page.reload();
  await page.waitForSelector('.boardpick select option', { state: 'attached' });
  await page.selectOption('.boardpick select', board);
  await page.setViewportSize({ width: 1280, height: 800 });
  await whenIdle(page);
  await shot('125-compact-1280');
  await pick(part.refdes.toLowerCase());
  await shot('125-compact-1280-picked');
  await page.evaluate((s) => localStorage.setItem('netsmith.settings', JSON.stringify(s)), { density: 'comfortable', scale: 100, motion: 'system' });
}
