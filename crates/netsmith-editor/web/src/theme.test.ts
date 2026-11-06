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
  for (const k of ['danger', 'warning', 'info']) list.push({ fg: `--${k}-text`, bg: [`--${k}-bg`], what: `${k} strip text` });
  list.push({ fg: '--on-danger', bg: ['--danger-solid'], what: 'label on a danger fill' });
  list.push({ fg: '--on-warning', bg: ['--warning-solid'], what: 'label on a warning fill' });
  list.push({ fg: '--on-kind', bg: KINDS.filter((k) => /part|module|board|system|panel|interface/.test(k)), what: 'badge letter on a kind fill' });
  for (const k of KINDS) list.push({ fg: k, bg: ['--surface', '--canvas'], what: 'kind icon or word on a surface' });
  for (const s of SYN) list.push({ fg: s, bg: ['--canvas'], what: 'syntax colour on the editor' });
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
  list.push({ fg: '--brand', bg: ['--surface'], min: 3, what: 'the logo' });
  list.push({ fg: '--pcb-key', bg: ['--pcb-bg'], what: 'board key text' });
  list.push({ fg: '--pcb-silk', bg: ['--pcb-bg'], what: 'board reference text' });
  for (const n of NETS) list.push({ fg: n, bg: ['--pcb-bg'], min: 3, what: 'net colour on the board (a graphic)' });
  for (const n of NETS) list.push({ fg: '--pcb-pad-text', bg: [n], min: 3, what: 'pad number on its net colour' });
  list.push({ fg: '--pcb-outline', bg: ['--pcb-bg'], min: 3, what: 'board outline' });
  list.push({ fg: '--pcb-select', bg: ['--pcb-bg'], min: 3, what: 'selection on the board' });
  list.push({ fg: '--pcb-keepout', bg: ['--pcb-bg'], min: 3, what: 'keepout on the board' });
  // Marks the page draws without words: the focus ring and the brand line.
  list.push({ fg: '--focus', bg: ['--surface', '--canvas', '--surface-raised', '--surface-3', '--bg'], min: 3, what: 'focus indicator' });
  list.push({ fg: '--accent', bg: ['--surface', '--canvas', '--bg'], min: 3, what: 'accent marks (the active tab line, a primary action)' });
  list.push({ fg: '--accent-text', bg: ['--bg'], what: 'accent text on the frame' });
  return list.filter((p) => p.bg.length);
}

const MIN: Record<Theme, number> = { dark: 4.5, light: 4.5, 'hc-dark': 7, 'hc-light': 7 };

describe.each(Object.keys(THEME_TOKENS) as Theme[])('contrast of %s', (name) => {
  const t = THEME_TOKENS[name];
  const ink = (token: string) => t[token] ?? THEME_TOKENS.dark[token];
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
