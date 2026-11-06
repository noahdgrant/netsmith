// Themes: every view in every theme (light, dark, high contrast light and dark, system), with no
// console error, no horizontal overflow and no text under 4.5:1, a screenshot of each; the choice
// is kept and applied before the first paint; the palette and the status bar menu choose it;
// storage that throws does not break the page; a 200 % zoom, the keyboard and reduced motion.
export const fixture = 'two_boards';

const THEMES = [
  ['light', 'Light'],
  ['dark', 'Dark'],
  ['hc-light', 'High contrast light'],
  ['hc-dark', 'High contrast dark'],
];
const VIEWS = ['source', 'schematic', 'power', 'system', 'bom', 'pcb', 'build'];

/** Overflow, and every piece of text under AA against the colours it is drawn on (what is
 * disabled, in a closed menu or inside the source editor is left out). */
const audit = () => {
  const parse = (s) => {
    const m = s.match(/rgba?\(([^)]+)\)/);
    if (m) {
      const p = m[1].split(/[ ,/]+/).map(Number);
      return [p[0], p[1], p[2], p[3] ?? 1];
    }
    const c = s.match(/color\(srgb ([^)]+)\)/);
    if (c) {
      const p = c[1].split(/[ /]+/).map(Number);
      return [p[0] * 255, p[1] * 255, p[2] * 255, p[3] ?? 1];
    }
    return null;
  };
  const lum = ([r, g, b]) => {
    const f = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
  };
  const blend = (fg, bg) => [0, 1, 2].map((i) => fg[i] * fg[3] + bg[i] * (1 - fg[3])).concat(1);
  const vis = (e) => {
    const r = e.getBoundingClientRect();
    const c = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && c.visibility !== 'hidden' && c.display !== 'none';
  };
  const backdrop = (e) => {
    let base = [255, 255, 255, 1];
    const chain = [];
    for (let n = e; n; n = n.parentElement) chain.push(n);
    for (const n of chain.reverse()) {
      const c = parse(getComputedStyle(n).backgroundColor);
      if (c && c[3] > 0) base = blend(c, base);
    }
    return base;
  };
  const low = [];
  const seen = new Set();
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let t; (t = walker.nextNode()); ) {
    const text = t.textContent.trim();
    const e = t.parentElement;
    if (!text || !e || e.closest('svg, .away, script, style, [disabled], [aria-disabled=true], .cm-wrap, .toast.leaving, details:not([open]) > :not(summary)') || !vis(e)) continue;
    const key = e.className + text.slice(0, 12);
    if (seen.has(key)) continue;
    seen.add(key);
    const cs = getComputedStyle(e);
    let alpha = 1;
    for (let n = e; n; n = n.parentElement) alpha *= Number(getComputedStyle(n).opacity);
    const fg0 = parse(cs.color);
    if (!fg0) continue;
    const bg = backdrop(e);
    const fg = blend([fg0[0], fg0[1], fg0[2], fg0[3] * alpha], bg);
    const [hi, lo] = [lum(fg), lum(bg)].sort((a, b) => b - a);
    const ratio = (hi + 0.05) / (lo + 0.05);
    const size = parseFloat(cs.fontSize);
    const large = size >= 24 || (size >= 18.66 && Number(cs.fontWeight) >= 700);
    if (ratio < (large ? 3 : 4.5)) low.push(`${ratio.toFixed(2)} "${text.slice(0, 28)}" in .${String(e.className).split(' ')[0]}`);
  }
  return { overflow: document.documentElement.scrollWidth > innerWidth, low };
};

// A toast on its way out (`.toast.leaving`: fading to nothing over 160 ms, then dropped) is not text a person reads, and its
// opacity makes any ratio 1:1: the audit leaves it out, as it leaves out what is hidden. A toast that stays is audited.
export async function run({ page, shot, step, expect, url, section }) {
  const attr = () => page.evaluate(() => document.documentElement.dataset.theme);
  const stored = () => page.evaluate(() => localStorage.getItem('netsmith.theme'));
  /** Choose a theme from the status bar menu, as a person would. */
  const choose = async (label) => {
    await page.click('.theme-item');
    await page.locator('.theme-option', { hasText: new RegExp(`^\\s*●?\\s*${label}$`) }).first().click();
    await page.waitForTimeout(200);
  };

  step('the page was themed before its first paint, from the stored choice');
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.evaluate(() => localStorage.setItem('netsmith.theme', '"hc-dark"'));
  // The boot script has run when the document becomes interactive, before the app's own script.
  await page.addInitScript(() => {
    document.addEventListener('readystatechange', () => {
      if (document.readyState === 'interactive') window.__boot = document.documentElement.dataset.theme;
    });
  });
  await page.reload();
  await page.waitForSelector('.app');
  expect((await page.evaluate(() => window.__boot)) === 'hc-dark', 'html[data-theme] is hc-dark when the document is interactive');
  expect((await stored()) === '"hc-dark"', 'the choice is kept as a JSON string');

  step('the status bar menu: five themes, the current one checked, Arrow keys and Enter choose');
  await page.click('.theme-item');
  await page.waitForSelector('.status-popover [role=radiogroup]');
  expect((await page.locator('.theme-option').count()) === 5, 'five themes');
  expect((await page.locator('.theme-option[aria-checked=true]').innerText()).includes('High contrast dark'), 'the current one is checked');
  await shot('menu');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await page.waitForTimeout(300);
  expect((await attr()) === 'hc-light', 'ArrowUp then Enter chose High contrast light');
  expect((await page.locator('.status-popover').count()) === 0, 'the menu closed');

  step('every theme: every view has no console error, no horizontal overflow and no text under AA');
  const failures = [];
  for (const [id, label] of THEMES) {
    await choose(label);
    expect((await attr()) === id, `${label} sets data-theme to ${id}`);
    expect((await stored()) === `"${id}"`, `${label} is kept`);
    for (const view of VIEWS) {
      await section(view);
      await page.waitForTimeout(450);
      const r = await page.evaluate(audit);
      if (r.overflow) failures.push(`${id} ${view}: horizontal overflow`);
      for (const l of r.low) failures.push(`${id} ${view}: contrast ${l}`);
      await shot(`${id}-${view}`);
    }
  }
  expect(failures.length === 0, `the audit found nothing (${failures.slice(0, 6).join('; ')}${failures.length > 6 ? ` and ${failures.length - 6} more` : ''})`);

  step('1920 x 1080: the Schematic and the Power tree in light and dark');
  await page.setViewportSize({ width: 1920, height: 1080 });
  for (const [id, label] of [THEMES[0], THEMES[1]]) {
    await choose(label);
    for (const view of ['schematic', 'power']) {
      await section(view);
      await page.waitForTimeout(450);
      await shot(`wide-${id}-${view}`);
    }
  }
  await page.setViewportSize({ width: 1280, height: 800 });

  step('the palette: Theme: ... chooses, and says so');
  await page.keyboard.press('Control+Shift+p');
  await page.waitForSelector('.qp');
  await page.keyboard.type('theme: high contrast dark');
  await page.waitForTimeout(250);
  await shot('palette');
  await page.keyboard.press('Enter');
  await page.waitForTimeout(350);
  expect((await attr()) === 'hc-dark', 'the command set hc-dark');
  expect((await page.locator('.toast', { hasText: 'Theme: High contrast dark' }).count()) === 1, 'a toast says which theme');
  await page.keyboard.press('Control+Shift+p');
  await page.keyboard.type('toggle light and dark');
  await page.keyboard.press('Enter');
  await page.waitForTimeout(300);
  expect((await attr()) === 'light', 'Toggle light and dark from a dark theme gives light');

  step('the choice survives a reload; System follows the browser, live');
  await choose('Dark');
  await page.reload();
  await page.waitForSelector('.app');
  expect((await attr()) === 'dark', 'dark after a reload');
  await choose('System');
  await page.emulateMedia({ colorScheme: 'light' });
  await page.waitForTimeout(200);
  expect((await attr()) === 'light', 'System is light when the browser asks for light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(200);
  expect((await attr()) === 'dark', 'System follows the browser to dark without a reload');
  await page.emulateMedia({ contrast: 'more' });
  await page.waitForTimeout(200);
  expect((await attr()) === 'hc-dark', 'System is high contrast when the browser asks for more contrast');
  await page.emulateMedia({ colorScheme: 'light', contrast: 'more' });
  await page.waitForTimeout(200);
  expect((await attr()) === 'hc-light', 'and high contrast light with a light scheme');
  await page.emulateMedia({ contrast: 'no-preference', colorScheme: 'light' });
  await page.waitForTimeout(200);
  await shot('system-light');
  await page.reload();
  await page.waitForSelector('.app');
  expect((await attr()) === 'light', 'System + a light browser is light after a reload (no flash to dark)');

  step('storage that throws: the page loads, follows the system and says the choice is not kept');
  const browser = page.context().browser();
  const blocked = await browser.newContext({ viewport: { width: 1280, height: 800 }, colorScheme: 'light' });
  await blocked.addInitScript(() => {
    Object.defineProperty(window, 'localStorage', {
      get() {
        throw new DOMException('blocked', 'SecurityError');
      },
    });
  });
  const p2 = await blocked.newPage();
  const errors = [];
  p2.on('pageerror', (e) => errors.push(e.message));
  await p2.goto(url);
  await p2.waitForSelector('.app');
  await p2.waitForTimeout(500);
  expect((await p2.evaluate(() => document.documentElement.dataset.theme)) === 'light', 'light, from the system, with no storage');
  await p2.click('.theme-item');
  await p2.locator('.theme-option', { hasText: /^\s*●?\s*Dark$/ }).click();
  await p2.waitForTimeout(300);
  expect((await p2.evaluate(() => document.documentElement.dataset.theme)) === 'dark', 'the theme still changes for this visit');
  expect((await p2.locator('.toast.bad', { hasText: 'not remembered' }).count()) === 1, 'a toast says it is not remembered');
  expect(errors.length === 0, `no page error without storage (${errors.join('; ')})`);
  await blocked.close();

  step('200 % zoom of a 1280 px window (640 x 400): no overflow in Source, Schematic, Power, BOM, and the Problems dock');
  await choose('Light');
  await page.setViewportSize({ width: 640, height: 400 });
  for (const view of ['source', 'schematic', 'power', 'bom']) {
    await section(view);
    await page.waitForTimeout(300);
    const r = await page.evaluate(audit);
    expect(!r.overflow, `${view}: no horizontal overflow at 640 px`);
    await shot(`zoom200-${view}`);
  }
  await page.keyboard.press('Control+j');
  await page.waitForSelector('[data-testid=dock]');
  await page.waitForTimeout(300);
  expect(!(await page.evaluate(audit)).overflow, 'the dock: no horizontal overflow at 640 px');
  await shot('zoom200-dock');
  await page.keyboard.press('Control+j');
  await page.setViewportSize({ width: 1280, height: 800 });

  step('keyboard: a skip link first, the tab strip takes arrow keys, every stop shows a focus ring');
  await page.reload();
  await page.waitForSelector('.app');
  await page.waitForTimeout(400);
  await page.keyboard.press('Tab');
  expect((await page.evaluate(() => document.activeElement?.className)).includes('skip-link'), 'the first Tab stop is the skip link');
  const noRing = [];
  const order = [];
  for (let i = 0; i < 14; i++) {
    await page.keyboard.press('Tab');
    const r = await page.evaluate(() => {
      const e = document.activeElement;
      const c = getComputedStyle(e);
      const ring = c.outlineStyle !== 'none' && parseFloat(c.outlineWidth) > 0;
      const inputLike = e.matches('input, select, textarea');
      const shadow = c.boxShadow !== 'none';
      const name = `${e.tagName.toLowerCase()}${e.getAttribute('data-section') ? `[${e.getAttribute('data-section')}]` : ''}.${String(e.className).split(' ')[0]}`;
      return { ring: ring || shadow || inputLike, name };
    });
    order.push(r.name);
    if (!r.ring) noRing.push(r.name);
  }
  expect(noRing.length === 0, `every Tab stop shows a ring (${noRing.join(', ')}) in ${order.join(' > ')}`);
  const tabIndex = order.findIndex((n) => n.startsWith('button[') && n.includes('tab'));
  expect(tabIndex > order.findIndex((n) => n.startsWith('select')), 'the board picker comes before the section tabs');
  await section('source');
  await page.locator('.tab[role=tab][aria-selected=true][data-section]').focus();
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(250);
  const sel = await page.locator('.tab[role=tab][aria-selected=true][data-section]').getAttribute('data-section');
  expect(sel === 'schematic', 'ArrowRight on the Source tab shows the Schematic');
  await page.keyboard.press('End');
  await page.waitForTimeout(250);
  expect((await page.locator('.tab[role=tab][aria-selected=true][data-section]').getAttribute('data-section')) === 'compare', 'End goes to the last section');
  await page.keyboard.press('Home');
  await page.waitForTimeout(250);
  expect((await page.locator('.tab[role=tab][aria-selected=true][data-section]').getAttribute('data-section')) === 'source', 'Home goes to the first');
  await shot('focus-tabs');

  step('reduced motion: transitions and animations take no time');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.keyboard.press('Control+j');
  await page.waitForSelector('[data-testid=dock]');
  const motion = await page.evaluate(() => {
    const d = getComputedStyle(document.querySelector('[data-testid=dock]'));
    const b = getComputedStyle(document.querySelector('.tool'));
    return { anim: parseFloat(d.animationDuration), trans: parseFloat(b.transitionDuration) };
  });
  expect(motion.anim < 0.001 && motion.trans < 0.001, `animation ${motion.anim}s and transition ${motion.trans}s are nil`);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  const normal = await page.evaluate(() => parseFloat(getComputedStyle(document.querySelector('.tool')).transitionDuration));
  expect(normal >= 0.1 && normal <= 0.2, `without the request a button transition is ${normal}s (120 ms)`);
  await page.keyboard.press('Control+j');
}
