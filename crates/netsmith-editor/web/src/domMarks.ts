// What the page's two DOM subscribers share (hoverDom.ts: `hover-lit`; compare/compareDom.ts: `cmp-*`): they put classes (and a
// data attribute) on elements that other code draws and re-draws, and a view that is rendered again sets the `class` of its own
// elements and so takes those classes off (a card of the power tree when it is picked, a row of a list when its tint changes).
// So a subscriber works out what it wants each element to have and changes only the difference: it can then look at the `class`
// attribute of the page without answering its own changes (nothing changed, so nothing is written, so nothing is seen), and
// two subscribers on one element never take each other's classes.

/** The part of an element this needs (a DOM element has it; a test makes one of its own). */
export interface Markable {
  classList: { add(...tokens: string[]): void; remove(...tokens: string[]): void; contains(token: string): boolean };
  getAttribute(name: string): string | null;
  setAttribute(name: string, value: string): void;
  removeAttribute(name: string): void;
}

/** What an element is to have: some classes, some attributes. */
export interface Wanted {
  classes: Set<string>;
  attrs: Map<string, string>;
}

export type WantedMap<E extends Markable> = Map<E, Wanted>;

/** Say that `el` is to have `classes` and, if given, the attribute. Several calls for one element add up. */
export function want<E extends Markable>(into: WantedMap<E>, el: E, classes: string[], attr?: [string, string]) {
  let w = into.get(el);
  if (!w) into.set(el, (w = { classes: new Set(), attrs: new Map() }));
  for (const c of classes) w.classes.add(c);
  if (attr) w.attrs.set(attr[0], attr[1]);
}

/** Make the elements as `next` says, given what this subscriber put on them last time: take off only what it no longer wants,
 * put on only what is missing. Returns whether it wrote anything (it does not when all is already right). */
export function reconcile<E extends Markable>(prev: WantedMap<E>, next: WantedMap<E>): boolean {
  let wrote = false;
  for (const [el, old] of prev) {
    const now = next.get(el);
    const gone = [...old.classes].filter((c) => !now?.classes.has(c) && el.classList.contains(c));
    if (gone.length) {
      el.classList.remove(...gone);
      wrote = true;
    }
    for (const name of old.attrs.keys())
      if (!now?.attrs.has(name) && el.getAttribute(name) !== null) {
        el.removeAttribute(name);
        wrote = true;
      }
  }
  for (const [el, now] of next) {
    const missing = [...now.classes].filter((c) => !el.classList.contains(c));
    if (missing.length) {
      el.classList.add(...missing);
      wrote = true;
    }
    for (const [name, value] of now.attrs)
      if (el.getAttribute(name) !== value) {
        el.setAttribute(name, value);
        wrote = true;
      }
  }
  return wrote;
}
