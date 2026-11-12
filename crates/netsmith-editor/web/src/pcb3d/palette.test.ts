import { describe, expect, it } from 'vitest';

import { contrast, parseHex } from '../contrast';
import css from '../themes.css?raw';
import { FILE_MATERIALS, LOOKS, allTokens, lit, roleOf } from './palette';

const THEMES = {
  dark: ':root, :root[data-theme="dark"]',
  light: ':root[data-theme="light"]',
  'hc-dark': ':root[data-theme="hc-dark"]',
  'hc-light': ':root[data-theme="hc-light"]',
};

function tokens(selector: string): Record<string, string> {
  const at = css.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  const body = css.slice(css.indexOf('{', at) + 1, css.indexOf('\n}', at));
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}

/** A token's colour in a theme, following `var()`. */
function colour(t: Record<string, string>, token: string): string {
  let v = t[token];
  for (let i = 0; i < 5 && v?.startsWith('var('); i++) v = t[v.slice(4, -1)];
  return v;
}

describe('the 3D palette', () => {
  it('draws every material the file names, and a body by where its height came from', () => {
    for (const m of FILE_MATERIALS) expect(roleOf(m, null), m).not.toBeNull();
    expect(roleOf('body', 'stated')).toBe('body');
    expect(roleOf('body', 'catalog')).toBe('body-catalog');
    expect(roleOf('body', 'unknown')).toBe('body-unknown');
    expect(roleOf('body-unknown', 'unknown')).toBe('body-unknown');
    expect(roleOf('chrome', null)).toBeNull();
    expect(LOOKS['body-unknown'].hatch).toBe(true);
    expect(LOOKS.limit.opacity).toBeLessThan(1);
    expect(LOOKS['limit-violated'].opacity).toBeLessThan(1);
  });
  for (const [theme, selector] of Object.entries(THEMES)) {
    describe(theme, () => {
      const t = tokens(selector);
      it('has every token the view reads, each a colour', () => {
        for (const token of allTokens()) expect(parseHex(colour(t, token) ?? ''), `${token} in ${theme}`).not.toBeNull();
      });
      const c = (token: string) => colour(t, token);
      it('a body, an unknown outline and a picked part stand out from the board (3:1)', () => {
        for (const token of ['--pcb3d-body', '--pcb3d-body-catalog', '--pcb3d-unknown', '--sel']) {
          expect(contrast(c(token), c('--pcb3d-mask'))!, `${token} on the mask in ${theme}`).toBeGreaterThanOrEqual(3);
        }
      });
      it('the board stands out from the background, and its edge from the board', () => {
        expect(contrast(c('--pcb3d-mask'), c('--pcb3d-bg'))!, `the mask on the background in ${theme}`).toBeGreaterThanOrEqual(1.4);
        expect(contrast(c('--pcb3d-substrate'), c('--pcb3d-bg'))!, `the laminate on the background in ${theme}`).toBeGreaterThanOrEqual(1.8);
      });
      it('a limit and a broken limit are two colours, and a stated and a catalog height are two', () => {
        expect(c('--pcb3d-limit')).not.toBe(c('--pcb3d-limit-violated'));
        expect(c('--pcb3d-body')).not.toBe(c('--pcb3d-body-catalog'));
        expect(c('--pcb3d-body')).not.toBe(c('--sel'));
      });
      it('the hover lights a body visibly, and not in the colour of a pick', () => {
        for (const token of ['--pcb3d-body', '--pcb3d-body-catalog', '--pcb3d-unknown']) {
          const base = parseHex(c(token))!;
          const on = lit(base, parseHex(c('--hover-lit'))!);
          const hex = `#${on.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
          expect(contrast(hex, c(token))!, `${token} lit in ${theme}`).toBeGreaterThanOrEqual(1.3);
          expect(hex).not.toBe(c('--sel'));
        }
      });
    });
  }
});
