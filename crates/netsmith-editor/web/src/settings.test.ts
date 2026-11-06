import { describe, expect, it } from 'vitest';

import indexHtml from '../index.html?raw';
import { DEFAULTS, SCALES, SETTINGS_KEY, parseSettings, readSettings, reducedMotion, rootState, settingMessage, writeSettings } from './settings';

describe('parseSettings', () => {
  it('keeps what is a choice and defaults the rest, field by field', () => {
    expect(parseSettings({ density: 'compact', scale: 125, motion: 'reduce' })).toEqual({ density: 'compact', scale: 125, motion: 'reduce' });
    expect(parseSettings({ density: 'cosy', scale: 130, motion: 'off' })).toEqual(DEFAULTS);
    expect(parseSettings({ density: 'compact', scale: '110' })).toEqual({ ...DEFAULTS, density: 'compact' });
    expect(parseSettings({ scale: 90 })).toEqual({ ...DEFAULTS, scale: 90 });
  });
  it('is the defaults for anything that is not an object', () => {
    for (const v of [null, undefined, 3, 'compact', [], true]) expect(parseSettings(v)).toEqual(DEFAULTS);
  });
  it('offers 90, 100, 110 and 125 percent', () => expect([...SCALES]).toEqual([90, 100, 110, 125]));
});

describe('the stored settings', () => {
  const store = () => {
    const data = new Map<string, string>();
    return { data, getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
  };
  it('reads what was written', () => {
    const s = store();
    const mine = { density: 'compact' as const, scale: 110 as const, motion: 'reduce' as const };
    expect(writeSettings(mine, s)).toBe(true);
    expect(readSettings(s)).toEqual(mine);
    expect(JSON.parse(s.data.get(SETTINGS_KEY)!)).toEqual(mine);
  });
  it('is the defaults when nothing, junk or a half-written object is stored', () => {
    expect(readSettings({ getItem: () => null })).toEqual(DEFAULTS);
    expect(readSettings({ getItem: () => 'not json' })).toEqual(DEFAULTS);
    expect(readSettings({ getItem: () => '{"scale":125}' })).toEqual({ ...DEFAULTS, scale: 125 });
  });
  it('is the defaults when the storage is missing or throws, and says so when it cannot keep a choice', () => {
    expect(readSettings(null)).toEqual(DEFAULTS);
    expect(readSettings({ getItem: () => { throw new Error('blocked'); } })).toEqual(DEFAULTS);
    expect(writeSettings(DEFAULTS, null)).toBe(false);
    expect(writeSettings(DEFAULTS, { setItem: () => { throw new Error('full'); } })).toBe(false);
    expect(settingMessage('Density', 'Compact', false).bad).toBe(true);
    expect(settingMessage('Density', 'Compact', true)).toEqual({ text: 'Density: Compact', bad: false });
  });
});

describe('reduced motion', () => {
  it('is on when the person chose Reduce, whatever the system says', () => {
    expect(reducedMotion({ ...DEFAULTS, motion: 'reduce' }, false)).toBe(true);
  });
  it('follows the system under System', () => {
    expect(reducedMotion(DEFAULTS, true)).toBe(true);
    expect(reducedMotion(DEFAULTS, false)).toBe(false);
  });
});

describe('index.html applies the settings before the first paint, the same way', () => {
  // The inline script in index.html is a second copy of rootState, so it must be run here.
  const script = /<script id="settings-boot">([\s\S]*?)<\/script>/.exec(indexHtml)?.[1];
  const boot = (stored: string | null | 'throws') => {
    const root = { dataset: {} as Record<string, string>, style: { props: {} as Record<string, string>, setProperty(k: string, v: string) { this.props[k] = v; } } };
    const win = {
      localStorage: {
        getItem: () => {
          if (stored === 'throws') throw new Error('blocked');
          return stored;
        },
      },
    };
    new Function('window', 'document', script!)(win, { documentElement: root });
    return root;
  };
  it('has the script', () => expect(script).toBeTruthy());
  it('agrees with rootState for every choice, and for junk', () => {
    const stored: (string | null | 'throws')[] = [null, 'throws', 'junk', '{"density":"cosy"}', '"compact"'];
    for (const density of ['comfortable', 'compact']) for (const scale of SCALES) for (const motion of ['system', 'reduce']) stored.push(JSON.stringify({ density, scale, motion }));
    for (const s of stored) {
      let want = DEFAULTS;
      try {
        want = s === null || s === 'throws' ? DEFAULTS : parseSettings(JSON.parse(s));
      } catch {
        want = DEFAULTS;
      }
      const root = boot(s);
      const r = rootState(want);
      expect(root.dataset.density, String(s)).toBe(r.density);
      expect(root.dataset.motion, String(s)).toBe(r.motion);
      expect(root.style.props['--ui-scale'], String(s)).toBe(r.uiScale);
    }
  });
});
