import { describe, expect, it } from 'vitest';

import indexHtml from '../index.html?raw';
import { contrast, over, parseHex } from './contrast';
import css from './themes.css?raw';
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
  for (const k of ['--pcb-front', '--pcb-back', '--pcb-thru']) list.push({ fg: '--pcb-pad-text', bg: [k], what: `a pad's number on ${k.slice(6)} copper` });
  // The soft light of a shared hover is a mark on every surface the views draw on, and is not a pick's blue.
  list.push({ fg: '--hover-lit', bg: [...SURFACES, '--pop', '--pcb-bg', '--pcb-board', '--sch-sheet'], min: 3, what: 'the shared hover light (a graphic)' });
  // What changed since a git ref: the outline of a changed part, a bar in the margin, a badge: marks on every surface a view draws on.
  for (const k of ['--cmp-added', '--cmp-changed', '--cmp-removed', '--cmp-renumbered']) {
    list.push({ fg: k, bg: [...SURFACES, '--sch-sheet', '--pcb-bg', '--pcb-board'], min: 3, what: `${k} mark of the comparison (a graphic)` });
  }
  // Marks the page draws without words: the focus ring and the brand line.
  list.push({ fg: '--focus', bg: ['--surface', '--canvas', '--surface-raised', '--surface-3', '--bg'], min: 3, what: 'focus indicator' });
  list.push({ fg: '--accent', bg: ['--surface', '--canvas', '--bg'], min: 3, what: 'accent marks (the active tab line, a primary action)' });
  list.push({ fg: '--accent-text', bg: ['--bg'], what: 'accent text on the frame' });
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

// ---- nothing but themes.css names a colour ----

const sources = import.meta.glob(['./**/*.ts', './**/*.tsx', './**/*.css'], { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
// The server's drawings are written with colours of their own (pcb/colours.ts lists them), and the tests name colours to test them.
const ALLOWED = [/themes\.css$/, /pcb\/colours\.ts$/, /\.test\.ts$/, /contrast\.ts$/];
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
