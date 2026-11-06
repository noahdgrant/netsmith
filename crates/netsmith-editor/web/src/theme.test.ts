import { describe, expect, it } from 'vitest';

import indexHtml from '../index.html?raw';
import compareCss from './compare.css?raw';
import { contrast, flatten, over, paint, parseHex } from './contrast';
import styleCss from './style.css?raw';
import css from './themes.css?raw';
import viewsCss from './views.css?raw';
import {
  type Environment,
  THEMES,
  THEME_KEY,
  type Theme,
  type ThemePref,
  isDark,
  parsePref,
  readPref,
  resolveTheme,
  writePref,
} from './theme';

const env = (dark: boolean, contrast = false): Environment => ({ dark, contrast });

describe('resolveTheme', () => {
  it('a chosen theme is that theme, whatever the system asks', () => {
    for (const t of ['light', 'dark', 'hc-dark', 'hc-light'] as const) {
      expect(resolveTheme(t, env(true))).toBe(t);
      expect(resolveTheme(t, env(false, true))).toBe(t);
    }
  });
  it('system follows the colour scheme', () => {
    expect(resolveTheme('system', env(true))).toBe('dark');
    expect(resolveTheme('system', env(false))).toBe('light');
  });
  it('system follows a request for more contrast', () => {
    expect(resolveTheme('system', env(true, true))).toBe('hc-dark');
    expect(resolveTheme('system', env(false, true))).toBe('hc-light');
  });
  it('knows which themes are dark', () => {
    expect(['light', 'dark', 'hc-dark', 'hc-light'].map((t) => isDark(t as Theme))).toEqual([false, true, true, false]);
  });
});

describe('the stored choice', () => {
  it('reads what was written', () => {
    const data = new Map<string, string>();
    const store = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
    for (const t of THEMES) {
      expect(writePref(t.id, store)).toBe(true);
      expect(readPref(store)).toBe(t.id);
    }
    expect(data.get(THEME_KEY)).toBe('"hc-dark"');
  });
  it('is system when nothing, junk or something unknown is stored', () => {
    const of = (v: string | null) => ({ getItem: () => v });
    expect(readPref(of(null))).toBe('system');
    expect(readPref(of('not json'))).toBe('system');
    expect(readPref(of('"purple"'))).toBe('system');
    expect(readPref(of('3'))).toBe('system');
    expect(parsePref(undefined)).toBe('system');
  });
  it('is system when the storage is missing or throws', () => {
    expect(readPref(null)).toBe('system');
    expect(
      readPref({
        getItem: () => {
          throw new Error('blocked');
        },
      }),
    ).toBe('system');
  });
  it('says so when it could not keep the choice', () => {
    expect(writePref('dark', null)).toBe(false);
    expect(
      writePref('dark', {
        setItem: () => {
          throw new Error('full');
        },
      }),
    ).toBe(false);
  });
});

describe('index.html decides before the first paint, the same way', () => {
  // The inline script in index.html is a second copy of resolveTheme, so it must be run here.
  const script = /<script id="theme-boot">([\s\S]*?)<\/script>/.exec(indexHtml)?.[1];
  const boot = (stored: string | null | 'throws', dark: boolean, more: boolean) => {
    const root = { dataset: {} as Record<string, string>, style: {} as Record<string, string> };
    const win = {
      localStorage: {
        getItem: () => {
          if (stored === 'throws') throw new Error('blocked');
          return stored;
        },
      },
      matchMedia: (q: string) => ({ matches: q.includes('color-scheme') ? dark : more }),
    };
    new Function('window', 'document', script!)(win, { documentElement: root });
    return root;
  };
  it('has the script', () => expect(script).toBeTruthy());
  it('agrees with resolveTheme for every choice and every environment', () => {
    const stored: (string | null | 'throws')[] = [null, 'throws', 'junk', ...THEMES.map((t) => JSON.stringify(t.id))];
    for (const s of stored) {
      for (const dark of [true, false]) {
        for (const more of [true, false]) {
          const pref: ThemePref = s === null || s === 'throws' || s === 'junk' ? 'system' : parsePref(JSON.parse(s));
          const root = boot(s, dark, more);
          expect(root.dataset.theme, `${s} dark=${dark} more=${more}`).toBe(resolveTheme(pref, env(dark, more)));
          expect(root.style.colorScheme).toBe(isDark(root.dataset.theme as Theme) ? 'dark' : 'light');
        }
      }
    }
  });
});

// ---- the themes hold the contrast they promise ----

function tokens(selector: string): Record<string, string> {
  const at = css.indexOf(selector);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  const body = css.slice(css.indexOf('{', at) + 1, css.indexOf('\n}', at));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}
export const THEME_TOKENS: Record<Theme, Record<string, string>> = {
  dark: tokens(':root, :root[data-theme="dark"]'),
  light: tokens(':root[data-theme="light"]'),
  'hc-dark': tokens(':root[data-theme="hc-dark"]'),
  'hc-light': tokens(':root[data-theme="hc-light"]'),
};

type Pair = { fg: string; bg: string[]; min?: number; what: string };
const SURFACES = ['--bg', '--surface', '--surface-raised', '--surface-3', '--canvas'];
const KINDS = ['part', 'net', 'module', 'definition', 'file', 'command', 'board', 'system', 'panel', 'interface'].map((k) => `--kind-${k}`);
const SYN = ['comment', 'doc', 'string', 'number', 'keyword', 'type', 'property', 'namespace', 'variable', 'operator', 'punct', 'invalid'].map((k) => `--syn-${k}`);
const NETS = Array.from({ length: 10 }, (_, i) => `--net-${i}`);

function pairs(): Pair[] {
  const list: Pair[] = [];
  for (const fg of ['--text', '--text-muted', '--text-faint']) list.push({ fg, bg: SURFACES, what: 'text on every surface' });
  list.push({ fg: '--accent-text', bg: ['--surface', '--surface-raised', '--canvas'], what: 'accent text' });
  list.push({ fg: '--accent-fg', bg: ['--accent-fill'], what: 'label of the primary button' });
  for (const k of ['danger', 'warning', 'success', 'info']) list.push({ fg: `--${k}`, bg: ['--surface', '--surface-raised', '--canvas'], what: `${k} as text or icon` });
  for (const k of ['danger', 'warning', 'info', 'success']) list.push({ fg: `--${k}-text`, bg: [`--${k}-bg`], what: `${k} strip text` });
  // A status pill, a log line and a sourcing chip: the four tones' words on the layers a card and a log sit on.
  for (const k of ['danger', 'warning', 'info', 'success']) list.push({ fg: `--${k}-text`, bg: ['--surface', '--surface-raised', '--canvas', '--surface-3'], what: `${k} words on a card, a log and a pill` });
  list.push({ fg: '--on-danger', bg: ['--danger-solid'], what: 'label on a danger fill' });
  list.push({ fg: '--on-warning', bg: ['--warning-solid'], what: 'label on a warning fill' });
  list.push({ fg: '--on-kind', bg: KINDS.filter((k) => /part|module|board|system|panel|interface/.test(k)), what: 'badge letter on a kind fill' });
  for (const k of KINDS) list.push({ fg: k, bg: ['--surface', '--canvas'], what: 'kind icon or word on a surface' });
  for (const s of SYN) list.push({ fg: s, bg: ['--canvas'], what: 'syntax colour on the editor' });
  // The source (round 4): the hint at the end of a line is text; the indent guide is a hairline, not text.
  list.push({ fg: '--cm-hint', bg: ['--canvas'], what: 'the inline hint at the end of a line of the source' });
  list.push({ fg: '--text', bg: ['--pop'], what: 'text in a menu' });
  list.push({ fg: '--text-faint', bg: ['--pop'], what: 'faint text in a menu' });
  list.push({ fg: '--text-muted', bg: ['--pop'], what: 'muted text in a menu' });
  // What is picked in a data view: the blue's words and its marks.
  list.push({ fg: '--sel-text', bg: [...SURFACES, '--pop'], what: 'selection text' });
  list.push({ fg: '--sel', bg: ['--surface', '--canvas', '--surface-raised', '--surface-3'], min: 3, what: 'selection marks (the bar at a picked row)' });
  for (const k of ['--sch-text', '--sch-net', '--sch-body', '--sch-pin', '--sch-bus']) list.push({ fg: k, bg: ['--sch-sheet'], what: 'schematic text' });
  list.push({ fg: '--sch-wire', bg: ['--sch-sheet'], min: 3, what: 'schematic wire (a graphic)' });
  list.push({ fg: '--sch-muted', bg: ['--sch-sheet'], min: 3, what: 'schematic open pin mark (a graphic)' });
  list.push({ fg: '--sch-bad', bg: ['--sch-sheet'], min: 3, what: 'schematic error mark (a graphic)' });
  list.push({ fg: '--sch-hit', bg: ['--sch-sheet'], min: 3, what: 'schematic highlight (a graphic)' });
  list.push({ fg: '--sch-mark', bg: ['--sch-sheet'], min: 3, what: 'schematic selection mark (a graphic)' });
  // A schematic note (doc comments and relations): its words and heading on the card, its frame on the sheet.
  for (const k of ['--sch-note-text', '--sch-note-heading']) list.push({ fg: k, bg: ['--sch-note-fill'], what: 'the words of a schematic note on its card' });
  list.push({ fg: '--sch-note-stroke', bg: ['--sch-sheet'], min: 3, what: 'the frame of a schematic note (a graphic)' });
  // A segmented control: its words on the track and on the segment that is on.
  list.push({ fg: '--text-muted', bg: ['--seg-track'], what: 'a segment that is off' });
  list.push({ fg: '--text', bg: ['--seg-on'], what: 'the segment that is on' });
  list.push({ fg: '--brand', bg: ['--surface'], min: 3, what: 'the logo' });
  // The board (round 3): text 4.5:1 on the canvas and the substrate, every mark that is a graphic 3:1 on both.
  const BOARD = ['--pcb-bg', '--pcb-board'];
  list.push({ fg: '--pcb-key', bg: BOARD, what: 'board key text' });
  list.push({ fg: '--pcb-refdes', bg: BOARD, what: 'board reference text' });
  list.push({ fg: '--pcb-silk', bg: BOARD, min: 3, what: 'board silkscreen (a graphic)' });
  for (const n of NETS) list.push({ fg: n, bg: ['--pcb-bg'], min: 3, what: 'net colour on the board (a plane border, a graphic)' });
  for (const k of ['--pcb-front', '--pcb-back', '--pcb-thru', '--pcb-via', '--pcb-outline', '--pcb-ratsnest', '--pcb-courtyard', '--pcb-fab', '--pcb-keepout', '--pcb-height-limit', '--pcb-select']) {
    list.push({ fg: k, bg: BOARD, min: 3, what: `${k.slice(6)} on the board (a graphic)` });
  }
  // The number of a pad with a hole is not drawn over the hole, so only the surface pads carry one.
  for (const k of ['--pcb-front', '--pcb-back']) list.push({ fg: '--pcb-pad-text', bg: [k], what: `a pad's number on ${k.slice(6)} copper` });
  // An inner plane (round 5): its border and fill tone is a graphic 3:1 on the canvas and the substrate, like every other mark.
  for (const k of ['--pcb-plane-1', '--pcb-plane-2', '--pcb-plane-3', '--pcb-plane-4']) list.push({ fg: k, bg: BOARD, min: 3, what: `${k.slice(6)}, an inner layer's tone (a graphic)` });
  // The soft light of a shared hover is a mark on every surface the views draw on, and is not a pick's blue.
  list.push({ fg: '--hover-lit', bg: [...SURFACES, '--pop', '--pcb-bg', '--pcb-board', '--sch-sheet'], min: 3, what: 'the shared hover light (a graphic)' });
  // What changed since a git ref: the outline of a changed part, a bar in the margin, a badge: marks on every surface a view draws on.
  for (const k of ['--cmp-added', '--cmp-changed', '--cmp-removed', '--cmp-renumbered']) {
    list.push({ fg: k, bg: [...SURFACES, '--sch-sheet', '--pcb-bg', '--pcb-board'], min: 3, what: `${k} mark of the comparison (a graphic)` });
  }
  // Marks the page draws without words: the focus ring and the brand line.
  list.push({ fg: '--focus', bg: ['--surface', '--canvas', '--surface-raised', '--surface-3', '--bg'], min: 3, what: 'focus indicator' });
  // The ring of a focused note of the schematic (round 8 fix) is a 2 px line on the sheet, clear of the note's own card (and on the next note's card where two meet).
  list.push({ fg: '--focus', bg: ['--sch-sheet', '--sch-note-fill'], min: 3, what: "a schematic note's focus ring, on the sheet and on a note's card" });
  // The overview ruler's glyphs (an error's disc, a warning's triangle) stand on the strip over the editor's surface.
  list.push({ fg: '--danger', bg: ['--surface', '--canvas'], min: 3, what: "the overview ruler's error disc (a graphic)" });
  list.push({ fg: '--warning', bg: ['--surface', '--canvas'], min: 3, what: "the overview ruler's warning triangle (a graphic)" });
  list.push({ fg: '--accent', bg: ['--surface', '--canvas', '--bg'], min: 3, what: 'accent marks (the active tab line, a primary action)' });
  list.push({ fg: '--accent-text', bg: ['--bg'], what: 'accent text on the frame' });
  // The status bar is on the frame: its stale, error and success words are the tones' own text colours, not the icon colours.
  for (const k of ['danger', 'warning', 'info', 'success']) list.push({ fg: `--${k}-text`, bg: ['--bg'], what: `${k} words on the frame (the status bar)` });
  return list.filter((p) => p.bg.length);
}

const MIN: Record<Theme, number> = { dark: 4.5, light: 4.5, 'hc-dark': 7, 'hc-light': 7 };

describe.each(Object.keys(THEME_TOKENS) as Theme[])('contrast of %s', (name) => {
  const t = THEME_TOKENS[name];
  // A token may name another (`var(--bg)`): follow it.
  const ink = (token: string): string => {
    const v = t[token] ?? THEME_TOKENS.dark[token];
    const m = /^var\((--[a-z0-9-]+)\)$/.exec(v);
    return m ? ink(m[1]) : v;
  };
  for (const p of pairs()) {
    for (const bg of p.bg) {
      // A graphic needs 3:1 in every theme; text 4.5 (7 in high contrast).
      const min = p.min ? (name.startsWith('hc') ? Math.max(p.min, 4.5) : p.min) : MIN[name];
      it(`${p.fg} on ${bg}: ${p.what} is at least ${min}:1`, () => {
        const ratio = contrast(ink(p.fg), ink(bg));
        expect(ratio, `${p.fg}=${ink(p.fg)} ${bg}=${ink(bg)}`).not.toBeNull();
        expect(ratio!).toBeGreaterThanOrEqual(min);
      });
    }
  }
  it('a part body that is a tint of its outline keeps the pin text and the outline readable', () => {
    const fill = ink('--sch-fill');
    const tint = /^color-mix\(in srgb, var\(--sch-body\) (\d+)%, transparent\)$/.exec(fill);
    if (!tint) return; // an opaque body: --sch-pin on it is held below
    const body = parseHex(ink('--sch-body'))!;
    const sheet = parseHex(ink('--sch-sheet'))!;
    const filled = over(body, sheet, Number(tint[1]) / 100);
    const hex = `#${filled.map((c) => c.toString(16).padStart(2, '0')).join('')}`;
    for (const k of ['--sch-pin', '--sch-body', '--sch-text']) expect(contrast(ink(k), hex)!, `${k} on the body ${hex}`).toBeGreaterThanOrEqual(MIN[name]);
    // the body is a visible step from the sheet, so a part reads as a box and not a frame of lines
    expect(contrast(hex, ink('--sch-sheet'))!).toBeGreaterThanOrEqual(1.08);
  });
  it('the board is a step from the canvas, and each kind of copper is told from the others', () => {
    expect(contrast(ink('--pcb-board'), ink('--pcb-bg'))!).toBeGreaterThanOrEqual(1.04);
    const tones = ['--pcb-front', '--pcb-back', '--pcb-thru', '--pcb-via'].map((k) => ink(k));
    expect(new Set(tones).size).toBe(4);
    // the front and back copper are a hue apart, not only a step: one is warm, one is cool
    const [fr, , fb] = parseHex(ink('--pcb-front'))!;
    const [br, , bb] = parseHex(ink('--pcb-back'))!;
    expect(fr - fb).toBeGreaterThan(0);
    expect(bb - br).toBeGreaterThan(0);
  });
  it('a through-hole pad is a ring (3:1) around a lighter body that is still a step from the substrate, with a hole that reads', () => {
    // The ring is what makes the pad a mark; the body inside it may be lighter, which is what makes the light board's through pads
    // calmer than a solid bronze disc (round 5). In the high contrast themes ring and body are one tone.
    const ring = ink('--pcb-thru');
    const fill = ink('--pcb-thru-fill');
    expect(contrast(ring, ink('--pcb-board'))!).toBeGreaterThanOrEqual(3);
    expect(contrast(fill, ink('--pcb-board'))!).toBeGreaterThanOrEqual(name.startsWith('hc') ? 3 : 1.3);
    expect(contrast(fill, ink('--pcb-bg'))!).toBeGreaterThanOrEqual(name.startsWith('hc') ? 3 : 1.3);
    if (!name.startsWith('hc')) expect(contrast(ring, fill)!).toBeGreaterThanOrEqual(1.5);
  });
  it('the four planes are four tones, none the colour of the front or back copper', () => {
    const planes = [1, 2, 3, 4].map((i) => ink(`--pcb-plane-${i}`).toLowerCase());
    expect(new Set(planes).size).toBe(4);
    for (const k of ['--pcb-front', '--pcb-back']) expect(planes).not.toContain(ink(k).toLowerCase());
  });
  it('the hover light is not the colour of a pick or of the brand', () => {
    for (const k of ['--sel', '--accent', '--sch-hit', '--sch-mark']) expect(ink('--hover-lit').toLowerCase()).not.toBe(ink(k).toLowerCase());
  });
  it('the indent guide is a step from the canvas and quieter than the hint, so a guide is seen and never read', () => {
    const g = contrast(ink('--cm-guide'), ink('--canvas'))!;
    expect(g).toBeGreaterThanOrEqual(1.08);
    expect(g).toBeLessThanOrEqual(3);
    expect(contrast(ink('--cm-hint'), ink('--canvas'))!).toBeGreaterThan(g);
  });
  it('the four marks of the comparison are told apart, and the words in a value chip stay readable on their tint', () => {
    const marks = ['--cmp-added', '--cmp-changed', '--cmp-removed', '--cmp-renumbered'];
    expect(new Set(marks.map((k) => ink(k).toLowerCase())).size).toBe(4);
    // views.css tints a chip with 16 % of its mark over the layer it sits on, and puts the page's ink on it
    for (const k of marks) {
      for (const layer of ['--surface', '--surface-raised', '--canvas', '--surface-3']) {
        const tint = over(parseHex(ink(k))!, parseHex(ink(layer))!, 0.16);
        const hex = `#${tint.map((c) => c.toString(16).padStart(2, '0')).join('')}`;
        expect(contrast(ink('--text'), hex)!, `--text on ${k} 16% over ${layer}`).toBeGreaterThanOrEqual(MIN[name]);
        expect(contrast(ink('--text-muted'), hex)!, `--text-muted on ${k} 16% over ${layer}`).toBeGreaterThanOrEqual(MIN[name] === 7 ? 7 : 4.5);
      }
    }
  });
  it('the segment that is on is a step from its track', () => {
    expect(contrast(ink('--seg-on'), ink('--seg-track'))!).toBeGreaterThanOrEqual(1.1);
  });
  it('a schematic note is a card: its fill is not the sheet, and its frame is not the colour of a wire or a part', () => {
    expect(ink('--sch-note-fill').toLowerCase()).not.toBe(ink('--sch-sheet').toLowerCase());
    for (const k of ['--sch-wire', '--sch-body', '--sch-bus', '--sch-frame']) expect(ink('--sch-note-stroke').toLowerCase(), k).not.toBe(ink(k).toLowerCase());
  });
  it('the schematic marks of a pick are told apart: the lit net, the picked part, the bus trunk and the wire', () => {
    const marks = ['--sch-hit', '--sch-mark', '--sch-bus', '--sch-wire'].map((k) => ink(k));
    expect(new Set(marks).size).toBe(4);
  });
  if (name.startsWith('hc')) {
    it('borders can be seen (3:1) on the surfaces', () => {
      for (const bg of ['--surface', '--canvas']) expect(contrast(t['--border'], t[bg])!).toBeGreaterThanOrEqual(3);
    });
  }
  it('every theme gives every token the dark theme has', () => {
    const missing = Object.keys(THEME_TOKENS.dark).filter((k) => !(k in t));
    expect(missing).toEqual([]);
  });
});

// ---- round 7: text on a tint (the diff view's changed lines, the row that is picked), and the stale views ----

// What the derived tokens at the end of themes.css (one definition for every theme) say.
const SHARED = (() => {
  const at = css.indexOf('/* ---- derived tokens');
  expect(at).toBeGreaterThan(0);
  const out: Record<string, string> = {};
  for (const m of css.slice(at).matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
})();
const percentOf = (source: string, selector: string, mark: string): number => {
  const m = new RegExp(`${selector.replace(/[.]/g, '\\.')}\\s*\\{[^}]*color-mix\\(in srgb, var\\(${mark}\\) ([\\d.]+)%`).exec(source);
  expect(m, `${selector} tints with ${mark}`).not.toBeNull();
  return Number(m![1]) / 100;
};

describe.each(Object.keys(THEME_TOKENS) as Theme[])('text on a tint in %s', (name) => {
  const t = THEME_TOKENS[name];
  const lookup = (k: string) => t[k] ?? SHARED[k] ?? THEME_TOKENS.dark[k];
  const rgbOf = (token: string) => parseHex(flatten(paint(`var(${token})`, lookup)!, [0, 0, 0]))!;
  const flat = (token: string, under: string) => flatten(paint(`var(${token})`, lookup)!, parseHex(under)!);
  const AA = 4.5;

  it('the diff view maps every syntax colour to the stronger one it has on a tinted line', () => {
    const rule = /\.diff-panel\s*\{([^}]*)\}/.exec(compareCss)?.[1] ?? '';
    for (const s of ['comment', 'doc', 'string', 'number', 'keyword', 'type', 'property', 'namespace', 'variable', 'operator', 'punct', 'invalid']) {
      expect(rule, `.diff-panel maps --syn-${s}`).toContain(`--syn-${s}: var(--diff-syn-${s});`);
      expect(SHARED[`--diff-syn-${s}`], `--diff-syn-${s} is defined once for every theme`).toMatch(/^color-mix\(in srgb, var\(--text\) [\d.]+%, var\(--syn-/);
    }
  });
  for (const [side, mark] of [['add', '--cmp-added'], ['del', '--cmp-removed']] as const) {
    it(`every syntax colour of the diff view is at least ${AA}:1 on a ${side === 'add' ? 'new' : 'old'} line and on its changed words`, () => {
      const canvas = flatten(paint('var(--canvas)', lookup)!, [0, 0, 0]);
      const linePct = percentOf(compareCss, `.cm-dl-${side}`, mark);
      const wordPct = percentOf(compareCss, `.cm-di-${side}`, mark);
      const line = flatten({ rgb: rgbOf(mark), a: linePct }, parseHex(canvas)!);
      const word = flatten({ rgb: rgbOf(mark), a: wordPct }, parseHex(line)!);
      for (const s of ['comment', 'doc', 'string', 'number', 'keyword', 'type', 'property', 'namespace', 'variable', 'operator', 'punct', 'invalid']) {
        const ink = flat(`--diff-syn-${s}`, canvas);
        expect(contrast(ink, line)!, `--syn-${s} (${ink}) on a ${side} line ${line}`).toBeGreaterThanOrEqual(AA);
        expect(contrast(ink, word)!, `--syn-${s} (${ink}) on a ${side} word ${word}`).toBeGreaterThanOrEqual(AA);
        // the diff's words, and the plain text, keep the strength they had on the page
        expect(contrast(flat(`--diff-syn-${s}`, canvas), canvas)!).toBeGreaterThanOrEqual(contrast(flat(`--syn-${s}`, canvas), canvas)!);
      }
      expect(contrast(rgbHex(rgbOf('--text')), word)!, '--text on a changed word').toBeGreaterThanOrEqual(AA);
    });
  }
  it('the source with the marks on keeps every syntax colour at least 4.5:1 on a changed or added line', () => {
    const canvas = flat('--canvas', '#000000');
    for (const [cls, mark] of [['added', '--cmp-added'], ['changed', '--cmp-changed']] as const) {
      const pct = percentOf(compareCss, `.cm-chg-line-${cls}`, mark);
      const line = flatten({ rgb: rgbOf(mark), a: pct }, parseHex(canvas)!);
      for (const s of ['comment', 'doc', 'string', 'number', 'keyword', 'type', 'property', 'namespace', 'variable', 'operator', 'punct', 'invalid']) {
        expect(contrast(flat(`--syn-${s}`, canvas), line)!, `--syn-${s} on a ${cls} line ${line}`).toBeGreaterThanOrEqual(AA);
      }
    }
  });
  it('the row that is picked (go to anything, the ref picker, the palette) keeps its secondary words at least 4.5:1', () => {
    for (const under of ['--pop', '--surface', '--surface-raised', '--canvas']) {
      const base = flat(under, '#000000');
      for (const row of ['--pick', '--active-bg', '--select-bg', '--hover-strong']) {
        const bg = flat(row, base);
        expect(contrast(flat('--text-muted', base), bg)!, `--text-muted on ${row} over ${under} (${bg})`).toBeGreaterThanOrEqual(AA);
        expect(contrast(flat('--text', base), bg)!, `--text on ${row} over ${under}`).toBeGreaterThanOrEqual(AA);
      }
    }
  });
  it('the status bar says "Last good version" in the tone made for words on the frame', () => {
    expect(contrast(flat('--warning-text', flat('--bg', '#000000')), flat('--bg', '#000000'))!).toBeGreaterThanOrEqual(AA);
  });
});

/** `#rrggbb` of a colour in channels. */
function rgbHex(c: [number, number, number]): string {
  return `#${c.map((n) => n.toString(16).padStart(2, '0')).join('')}`;
}

describe('the rows and views that are dimmed or picked', () => {
  const rule = (source: string, selector: string): string => {
    const at = source.indexOf(`${selector} {`);
    expect(at, selector).toBeGreaterThanOrEqual(0);
    return source.slice(at, source.indexOf('}', at));
  };
  it('a view that shows the last good version is desaturated, never made translucent (opacity lowers the contrast of every word on it)', () => {
    const r = rule(styleCss, '.section.is-stale > :not(.stale-strip)');
    expect(r).toMatch(/saturate\(/);
    expect(r).not.toMatch(/opacity\s*:/);
  });
  it('the status bar words for a stale analysis are the warning words, not the warning icon colour', () => {
    expect(rule(styleCss, '.statusbar .stale')).toContain('color: var(--warning-text)');
  });
  it('on the row that is picked the detail and the kind are the muted words, not the faint ones', () => {
    for (const k of ['.qp-detail', '.qp-type']) expect(viewsCss).toMatch(new RegExp(`\\.qp-row\\.on ${k.replace('.', '\\.')}[^{]*\\{[^}]*color: var\\(--text-muted\\)`));
  });
});

// ---- nothing but themes.css names a colour ----

const sources = import.meta.glob(['./**/*.ts', './**/*.tsx', './**/*.css'], { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
// The server's drawings are written with colours of their own (pcb/colours.ts lists them), and the tests name colours to test them.
const ALLOWED = [/themes\.css$/, /pcb\/colours\.ts$/, /\.test\.ts$/, /contrast\.ts$/];
describe('the schematic notes are painted by the page', () => {
  const rule = (selector: string): string => {
    const at = styleCss.indexOf(selector);
    expect(at, selector).toBeGreaterThanOrEqual(0);
    return styleCss.slice(at, styleCss.indexOf('}', at));
  };
  it('maps the server\u2019s default tokens and each class to the theme\u2019s tokens', () => {
    const host = rule('.schematic-host svg.schematic {');
    for (const [own, theme] of [['--note-fill', '--sch-note-fill'], ['--note-stroke', '--sch-note-stroke'], ['--note-text', '--sch-note-text'], ['--note-stroke-text', '--sch-note-heading']]) {
      expect(host, own).toContain(`${own}: var(${theme})`);
    }
    expect(rule('.schematic-host svg.schematic .note .note-frame')).toContain('var(--sch-note-fill)');
    expect(rule('.schematic-host svg.schematic .note .note-frame')).toContain('var(--sch-note-stroke)');
    expect(rule('.schematic-host svg.schematic .note .note-text')).toContain('var(--sch-note-text)');
    expect(rule('.schematic-host svg.schematic .note .note-heading')).toContain('var(--sch-note-heading)');
  });
  it('outranks the drawing\u2019s own embedded style and the rule that colours the text of a lit thing', () => {
    // `.schematic .note text` is (0,2,1) in the drawing and `.schematic-host svg.schematic .on text` is (0,3,2) in the page:
    // the note's rules have four classes, so a note that is lit keeps its own words.
    for (const selector of ['.note .note-frame', '.note .note-text', '.note .note-heading']) {
      expect(styleCss, selector).toContain(`.schematic-host svg.schematic ${selector}`);
    }
  });
});

describe('the page has no colours of its own', () => {
  it('finds the sources', () => expect(Object.keys(sources).length).toBeGreaterThan(40));
  it('has no hex or rgb colour outside themes.css', () => {
    const found: string[] = [];
    for (const [file, text] of Object.entries(sources)) {
      if (ALLOWED.some((re) => re.test(file))) continue;
      // An entity such as &#9654; and a CSS id selector are not colours; a hex colour is `#` and 3 to 8 hex digits not followed by more word.
      // The rules of style.css that map the server's attribute colours to tokens name the server's colours.
      const ownText = text.split('\n').map((l) => (/\[(fill|stroke)="#/.test(l) ? '' : l)).join('\n');
      for (const m of ownText.matchAll(/(?<![&\w])#[0-9a-fA-F]{3,8}\b(?!-)/g)) {
        const line = ownText.slice(0, m.index).split('\n').length;
        found.push(`${file}:${line} ${m[0]}`);
      }
      for (const m of text.matchAll(/\brgba?\(/g)) found.push(`${file}:${text.slice(0, m.index).split('\n').length} rgb(`);
    }
    expect(found).toEqual([]);
  });
});
