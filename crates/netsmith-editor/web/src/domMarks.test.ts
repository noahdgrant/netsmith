import { describe, expect, it } from 'vitest';

import { type Markable, reconcile, want, type WantedMap } from './domMarks';

/** An element as far as the subscribers use one. `render` sets the class as a view does when it is drawn again. */
class Fake implements Markable {
  classes = new Set<string>();
  attrs = new Map<string, string>();
  writes = 0;
  classList = {
    add: (...t: string[]) => (this.writes++, t.forEach((c) => this.classes.add(c))),
    remove: (...t: string[]) => (this.writes++, t.forEach((c) => this.classes.delete(c))),
    contains: (t: string) => this.classes.has(t),
  };
  getAttribute = (n: string) => this.attrs.get(n) ?? null;
  setAttribute = (n: string, v: string) => (this.writes++, void this.attrs.set(n, v));
  removeAttribute = (n: string) => (this.writes++, void this.attrs.delete(n));
  render(cls: string) {
    this.classes = new Set(cls.split(' ').filter(Boolean));
  }
}

const map = (...entries: [Fake, string[], [string, string]?][]): WantedMap<Fake> => {
  const m: WantedMap<Fake> = new Map();
  for (const [el, classes, attr] of entries) want(m, el, classes, attr);
  return m;
};

describe('reconciling the classes a subscriber puts on the page', () => {
  it('puts what is wanted on, and takes off what it put on and no longer wants', () => {
    const a = new Fake();
    const b = new Fake();
    const first = map([a, ['cmp-changed']], [b, ['cmp-added']]);
    expect(reconcile(new Map(), first)).toBe(true);
    expect([...a.classes, ...b.classes]).toEqual(['cmp-changed', 'cmp-added']);
    const second = map([a, ['cmp-changed']]);
    reconcile(first, second);
    expect(b.classes.size).toBe(0);
    expect(a.classes.has('cmp-changed')).toBe(true);
  });
  it('writes nothing when all is already right: a watcher of the `class` attribute does not hear itself', () => {
    const a = new Fake();
    const m = map([a, ['hover-lit']]);
    reconcile(new Map(), m);
    const writes = a.writes;
    expect(reconcile(m, map([a, ['hover-lit']]))).toBe(false);
    expect(a.writes).toBe(writes);
  });
  it('puts a class back that a view took off when it was drawn again, and leaves the view\'s own classes', () => {
    const card = new Fake();
    card.render('pt-node pt-rail');
    const m = map([card, ['cmp-changed']]);
    reconcile(new Map(), m);
    expect([...card.classes].sort()).toEqual(['cmp-changed', 'pt-node', 'pt-rail']);
    card.render('pt-node pt-rail selected');
    expect(reconcile(m, map([card, ['cmp-changed']]))).toBe(true);
    expect([...card.classes].sort()).toEqual(['cmp-changed', 'pt-node', 'pt-rail', 'selected']);
  });
  it('two subscribers on one element take only their own classes', () => {
    const el = new Fake();
    const hover = map([el, ['hover-lit']]);
    const marks = map([el, ['cmp-changed', 'cmp-shrank']]);
    reconcile(new Map(), hover);
    reconcile(new Map(), marks);
    expect([...el.classes].sort()).toEqual(['cmp-changed', 'cmp-shrank', 'hover-lit']);
    reconcile(hover, new Map());
    expect([...el.classes].sort()).toEqual(['cmp-changed', 'cmp-shrank']);
    reconcile(marks, new Map());
    expect(el.classes.size).toBe(0);
  });
  it('keeps an attribute (a rail\'s badge) in step, changes it when the value changes and removes it with the mark', () => {
    const rail = new Fake();
    const m1 = map([rail, ['cmp-changed'], ['data-cmp-delta', '70 mA → 50 mA']]);
    reconcile(new Map(), m1);
    expect(rail.getAttribute('data-cmp-delta')).toBe('70 mA → 50 mA');
    const m2 = map([rail, ['cmp-changed'], ['data-cmp-delta', '70 mA → 20 mA']]);
    reconcile(m1, m2);
    expect(rail.getAttribute('data-cmp-delta')).toBe('70 mA → 20 mA');
    reconcile(m2, new Map());
    expect(rail.getAttribute('data-cmp-delta')).toBeNull();
    expect(rail.classes.size).toBe(0);
  });
  it('does not take off a class it did not put on (the same name set by someone else after it)', () => {
    const el = new Fake();
    const m = map([el, ['hover-lit']]);
    reconcile(new Map(), m);
    el.render('row');
    reconcile(m, new Map());
    expect([...el.classes]).toEqual(['row']);
  });
});
