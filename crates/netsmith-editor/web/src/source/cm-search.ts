// The find and replace panel, built here with the page's controls instead of CodeMirror's own: a card
// at the top right of the text with a field, the count (`3 of 12`), previous and next, the three
// switches (case, whole word, regular expression) and a second row for replacing. CodeMirror's search
// commands do the work; this is only the panel.

import {
  SearchQuery,
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  replaceAll,
  replaceNext,
  search,
  searchKeymap,
  setSearchQuery,
} from '@codemirror/search';
import type { EditorState, Extension } from '@codemirror/state';
import { EditorView, type KeyBinding, type Panel, type ViewUpdate, runScopeHandlers } from '@codemirror/view';

import { iconDom } from './dom';

/** How many matches the count looks for before it says `999+`. */
const CAP = 999;

/** The number of matches of a query in a document, and which one the selection is on (0 when none). */
export function countMatches(state: EditorState, query: SearchQuery): { total: number; current: number; capped: boolean; invalid: boolean } {
  if (!query.search || !query.valid) return { total: 0, current: 0, capped: false, invalid: !!query.search && !query.valid };
  const sel = state.selection.main;
  const it = query.getCursor(state);
  let total = 0;
  let current = 0;
  for (let r = it.next(); !r.done; r = it.next()) {
    total++;
    if (r.value.from === sel.from && r.value.to === sel.to) current = total;
    if (total >= CAP) return { total, current, capped: true, invalid: false };
  }
  return { total, current, capped: false, invalid: false };
}

/** What the count says. */
export function countText(c: { total: number; current: number; capped: boolean; invalid: boolean }, searching: boolean): string {
  if (!searching) return '';
  if (c.invalid) return 'Not a pattern';
  if (c.total === 0) return 'No results';
  const total = c.capped ? `${c.total}+` : String(c.total);
  return c.current ? `${c.current} of ${total}` : `${total} found`;
}

/** Open with the replace row showing (Ctrl+H). */
let wantReplace = false;
export function requestReplace() {
  wantReplace = true;
}

class FindPanel implements Panel {
  dom: HTMLElement;
  top = true;
  private field: HTMLInputElement;
  private replaceField: HTMLInputElement;
  private count: HTMLElement;
  private rows: { replace: HTMLElement };
  private toggles: Record<'caseSensitive' | 'wholeWord' | 'regexp', HTMLButtonElement>;
  private twisty: HTMLButtonElement;
  private replaceButtons: HTMLButtonElement[];
  private box: HTMLElement;

  constructor(private view: EditorView) {
    const query = getSearchQuery(view.state);
    const el = (tag: string, cls: string, text?: string) => {
      const n = document.createElement(tag);
      n.className = cls;
      if (text) n.textContent = text;
      return n;
    };
    const button = (cls: string, label: string, tip: string, onClick: () => void, content?: Node | string) => {
      const b = el('button', `sp-btn ${cls}`) as HTMLButtonElement;
      b.type = 'button';
      b.setAttribute('aria-label', label);
      b.setAttribute('data-tip', tip);
      if (content) b.append(content);
      b.addEventListener('click', onClick);
      return b;
    };

    this.dom = el('div', 'cm-search sp');
    this.dom.setAttribute('role', 'search');
    this.dom.setAttribute('aria-label', 'Find and replace');
    this.dom.setAttribute('data-testid', 'find-panel');
    this.dom.addEventListener('keydown', (e) => this.keydown(e));

    const row = el('div', 'sp-row');
    this.twisty = button('sp-twisty', 'Show replace', 'Show replace', () => this.showReplace(this.rows.replace.hidden), iconDom('chevron-right', 14));
    this.box = el('label', 'sp-field');
    this.box.append(iconDom('search', 13));
    this.field = el('input', '') as HTMLInputElement;
    this.field.name = 'search';
    this.field.placeholder = 'Find';
    this.field.setAttribute('aria-label', 'Find');
    this.field.setAttribute('main-field', 'true');
    this.field.value = query.search;
    this.field.addEventListener('input', () => this.commit());
    this.box.append(this.field);
    this.count = el('span', 'sp-count');
    this.count.setAttribute('aria-live', 'polite');
    const prev = button('', 'Previous match', 'Previous match (Shift+Enter)', () => findPrevious(view), iconDom('chevron-up', 14));
    const next = button('', 'Next match', 'Next match (Enter)', () => findNext(view), iconDom('chevron-down', 14));
    const toggle = (key: 'caseSensitive' | 'wholeWord' | 'regexp', label: string, tip: string, text: string) => {
      const b = button('mono', label, tip, () => {
        this.toggles[key].classList.toggle('on');
        this.commit();
      }, text);
      b.setAttribute('aria-pressed', 'false');
      return b;
    };
    this.toggles = {
      caseSensitive: toggle('caseSensitive', 'Match case', 'Match case', 'Aa'),
      wholeWord: toggle('wholeWord', 'Whole word', 'Whole word', 'ab'),
      regexp: toggle('regexp', 'Regular expression', 'Regular expression', '.*'),
    };
    const close = button('', 'Close', 'Close (Esc)', () => this.close(), iconDom('close', 14));
    row.append(this.twisty, this.box, this.count, prev, next, this.toggles.caseSensitive, this.toggles.wholeWord, this.toggles.regexp, close);

    const replace = el('div', 'sp-row sp-replace');
    replace.hidden = true;
    const rbox = el('label', 'sp-field');
    this.replaceField = el('input', '') as HTMLInputElement;
    this.replaceField.name = 'replace';
    this.replaceField.placeholder = 'Replace';
    this.replaceField.setAttribute('aria-label', 'Replace');
    this.replaceField.value = query.replace;
    this.replaceField.addEventListener('input', () => this.commit());
    rbox.append(this.replaceField);
    const one = button('text', 'Replace', 'Replace this match', () => replaceNext(view), 'Replace');
    const all = button('text', 'Replace all', 'Replace every match', () => replaceAll(view), 'All');
    this.replaceButtons = [one, all];
    replace.append(rbox, one, all);
    this.rows = { replace };
    this.dom.append(row, replace);
    this.sync(query, view.state);
    if (wantReplace) this.showReplace(true);
    wantReplace = false;
  }

  private showReplace(on: boolean) {
    if (this.view.state.readOnly) on = false;
    this.rows.replace.hidden = !on;
    this.twisty.classList.toggle('on', false);
    this.twisty.classList.toggle('open', on);
    this.twisty.setAttribute('aria-expanded', String(on));
    this.twisty.setAttribute('aria-label', on ? 'Hide replace' : 'Show replace');
    if (on && document.activeElement !== this.field) this.replaceField.focus();
  }

  private commit() {
    const query = new SearchQuery({
      search: this.field.value,
      replace: this.replaceField.value,
      caseSensitive: this.toggles.caseSensitive.classList.contains('on'),
      wholeWord: this.toggles.wholeWord.classList.contains('on'),
      regexp: this.toggles.regexp.classList.contains('on'),
    });
    if (!query.eq(getSearchQuery(this.view.state))) this.view.dispatch({ effects: setSearchQuery.of(query) });
  }

  private sync(query: SearchQuery, state: EditorState) {
    if (this.field.value !== query.search) this.field.value = query.search;
    if (this.replaceField.value !== query.replace) this.replaceField.value = query.replace;
    for (const [key, b] of Object.entries(this.toggles) as ['caseSensitive' | 'wholeWord' | 'regexp', HTMLButtonElement][]) {
      b.classList.toggle('on', query[key]);
      b.setAttribute('aria-pressed', String(query[key]));
    }
    const c = countMatches(state, query);
    this.count.textContent = countText(c, !!query.search);
    this.count.classList.toggle('none', !!query.search && c.total === 0);
    this.box.classList.toggle('none', !!query.search && c.total === 0);
    for (const b of this.replaceButtons) b.disabled = state.readOnly || c.total === 0;
  }

  private close() {
    closeSearchPanel(this.view);
    this.view.focus();
  }

  private keydown(e: KeyboardEvent) {
    if (runScopeHandlers(this.view, e, 'search-panel')) {
      e.preventDefault();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      this.close();
    } else if (e.key === 'Enter' && e.target === this.field) {
      e.preventDefault();
      (e.shiftKey ? findPrevious : findNext)(this.view);
    } else if (e.key === 'Enter' && e.target === this.replaceField) {
      e.preventDefault();
      replaceNext(this.view);
    }
  }

  mount() {
    this.field.select();
  }

  update(u: ViewUpdate) {
    const query = getSearchQuery(u.state);
    if (u.docChanged || u.selectionSet || u.transactions.some((tr) => tr.effects.some((e) => e.is(setSearchQuery)))) this.sync(query, u.state);
  }
}

export const searchExtension: Extension = search({ top: true, createPanel: (view) => new FindPanel(view) });

/** The search keys of CodeMirror, without its go to line box (the page's `:` does that), and Ctrl+H. */
export const searchKeys: KeyBinding[] = [
  ...searchKeymap.filter((k) => k.key !== 'Alt-g'),
  {
    key: 'Mod-h',
    preventDefault: true,
    run: (view) => {
      requestReplace();
      const open = document.querySelector('.cm-search.sp');
      if (open) {
        // Already open: show the replace row.
        (open.querySelector('.sp-twisty:not(.open)') as HTMLElement | null)?.click();
        return true;
      }
      return searchKeymap.find((k) => k.key === 'Mod-f')?.run?.(view) ?? false;
    },
  },
];
