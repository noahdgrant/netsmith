// What the schematic recipes share (this file is not a recipe: recipes are in recipes/).
import { whenIdle } from './settle.mjs';

/** The sheet the toolbar's picker names: `light1`. */
export const activeSheet = (page) => page.locator('.sheetpick .crumb-sheet').innerText();

/** Open the toolbar's sheet picker and return the names of its sheets (the overview first). */
export async function sheetNames(page) {
  await page.locator('.sheetpick .pick-btn').click();
  const names = await page.locator('.pick-pop .sl-row .sl-name').allInnerTexts();
  await page.keyboard.press('Escape');
  return names;
}

/** Open a sheet by its name (`modem`, or `All sheets`) from the toolbar's picker. */
export async function openSheet(page, name) {
  await page.locator('.sheetpick .pick-btn').click();
  await page.locator('.pick-pop .sl-row', { has: page.locator('.sl-name', { hasText: new RegExp(`^${name.replace(/[()/]/g, '\\$&')}$`) }) }).first().click();
  await whenIdle(page);
}

/** The zoom the zoom bar shows, in percent. */
export async function zoomPercent(page) {
  return Number((await page.locator('.zoombar span:not(.zsep)').first().innerText()).replace('%', ''));
}

// ---- colours (a computed colour is `rgb(r, g, b)`, `rgba(...)` or `color(srgb r g b / a)`) ----

/** [r, g, b] in 0..255 of a computed colour, or null. */
export function rgbOf(css) {
  const rgb = /^rgba?\(([^)]+)\)$/.exec(css.trim());
  if (rgb) return rgb[1].split(',').slice(0, 3).map(Number);
  const srgb = /^color\(srgb ([^)/]+)/.exec(css.trim());
  if (srgb) return srgb[1].trim().split(/\s+/).map((v) => Math.round(Number(v) * 255));
  return null;
}

const lum = ([r, g, b]) => {
  const f = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
};

/** The WCAG contrast ratio of two computed colours (null when one is not a plain colour). */
export function contrastOf(a, b) {
  const x = rgbOf(a);
  const y = rgbOf(b);
  if (!x || !y) return null;
  const [hi, lo] = [lum(x), lum(y)].sort((p, q) => q - p);
  return (hi + 0.05) / (lo + 0.05);
}

// ---- notes ----

/** The notes of the drawing on show (a doc comment or a relation, drawn as `g.note`): where each is on the screen, where it is written, and the
 * colours its frame and words have now, with the theme's own tokens and the sheet behind it, resolved. */
export function noteFacts(page) {
  return page.evaluate(() => {
    const token = (name) => {
      const probe = document.createElement('i');
      probe.style.color = `var(${name})`;
      document.body.appendChild(probe);
      const colour = getComputedStyle(probe).color;
      probe.remove();
      return colour;
    };
    const tokens = { fill: token('--sch-note-fill'), stroke: token('--sch-note-stroke'), text: token('--sch-note-text'), heading: token('--sch-note-heading'), sheet: token('--sch-sheet'), sel: token('--sel') };
    const notes = [...document.querySelectorAll('svg.schematic g.note')]
      .filter((n) => n.getClientRects().length > 0)
      .map((n) => {
        const frame = n.querySelector('.note-frame');
        const text = n.querySelector('.note-text');
        const heading = n.querySelector('.note-heading');
        const b = n.getBoundingClientRect();
        return {
          cls: n.getAttribute('class'),
          at: n.getAttribute('data-at'),
          ref: n.getAttribute('data-ref'),
          index: n.getAttribute('data-n'),
          box: { x: b.x, y: b.y, w: b.width, h: b.height },
          frameFill: frame && getComputedStyle(frame).fill,
          frameStroke: frame && getComputedStyle(frame).stroke,
          textFill: text && getComputedStyle(text).fill,
          headingFill: heading && getComputedStyle(heading).fill,
          opacity: getComputedStyle(n).opacity,
          lit: n.classList.contains('on'),
        };
      });
    return { tokens, notes };
  });
}

/** The parts on show, as boxes on the screen. */
export const partBoxes = (page) =>
  page.evaluate(() =>
    [...document.querySelectorAll('svg.schematic g.part[data-ref]')]
      .filter((p) => p.getClientRects().length > 0)
      .map((p) => {
        const b = p.getBoundingClientRect();
        return { ref: p.getAttribute('data-ref'), x: b.x, y: b.y, w: b.width, h: b.height };
      }),
  );

/** Whether two boxes (`x y w h`) overlap by more than `slack` pixels in both directions. */
export const overlaps = (a, b, slack = 0.5) => a.x + a.w - slack > b.x && b.x + b.w - slack > a.x && a.y + a.h - slack > b.y && b.y + b.h - slack > a.y;

/** The halos of the parts on show that have a fill or a stroke now, by designator of the part they stand behind. */
export const visibleHalos = async (page) => {
  // A halo's fill and stroke are CSS transitions: read after they have run, not on the frame of the pick (CI saw `none`).
  await page.waitForFunction(() => [...document.querySelectorAll('svg.schematic .part-halo')].every((h) => h.getAnimations().length === 0));
  return page.evaluate(() =>
    [...document.querySelectorAll('svg.schematic g.part[data-ref]')]
      .filter((p) => p.getClientRects().length > 0)
      .map((p) => {
        const halo = p.previousElementSibling?.classList.contains('part-halo') ? p.previousElementSibling : null;
        const c = halo && getComputedStyle(halo);
        const seen = (v) => !!v && v !== 'none' && !/\/ 0\)$|, 0\)$/.test(v);
        return { ref: p.getAttribute('data-ref'), halo: !!halo, fill: c?.fill, stroke: c?.stroke, visible: !!c && (seen(c.fill) || seen(c.stroke)) };
      })
      .filter((h) => h.visible),
  );
};

export const THEMES = ['light', 'dark', 'hc-light', 'hc-dark'];

/** Put the page in a theme (the attribute the page's own theme code sets; the page's rules key on it) and return what it was. */
export const setTheme = (page, id) =>
  page.evaluate((t) => {
    const was = document.documentElement.getAttribute('data-theme');
    document.documentElement.setAttribute('data-theme', t);
    return was;
  }, id);
