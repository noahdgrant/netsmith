import { describe, expect, it } from 'vitest';

import rust from '../../../../netsmith-pcb/src/svg.rs?raw';
import css from '../style.css?raw';
import { FIXED, NET_PALETTE, themed } from './colours';

describe("the page knows every colour of the server's board drawing", () => {
  it('has the palette of the drawing, in its order', () => {
    const list = /const PALETTE: [^=]*= &?\[([\s\S]*?)\];/.exec(rust)?.[1] ?? '';
    expect([...list.matchAll(/"(#[0-9a-f]{6})"/g)].map((m) => m[1])).toEqual(NET_PALETTE);
  });
  it('has every other colour the drawing names', () => {
    const named = [...rust.matchAll(/^const [A-Z_]+: &str = "(#[0-9a-f]{6})";/gm)].map((m) => m[1]);
    expect(named.length).toBeGreaterThan(8);
    for (const c of named) expect(Object.keys(FIXED), c).toContain(c);
  });
  it('maps each of them in style.css', () => {
    for (const c of [...NET_PALETTE, ...Object.keys(FIXED)]) {
      expect(css, c).toContain(`[fill="${c}"]`);
      expect(css, c).toContain(`[stroke="${c}"]`);
    }
  });
});

describe('themed', () => {
  it('turns a net colour into its token', () => {
    expect(themed('#4DD4AC')).toBe('var(--net-1)');
    expect(themed('#8b949e')).toBe('var(--pcb-unconnected)');
  });
  it('leaves a colour it does not know', () => {
    expect(themed('#123456')).toBe('#123456');
  });
});
