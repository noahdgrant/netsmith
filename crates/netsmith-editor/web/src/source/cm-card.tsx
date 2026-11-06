// The hover card in the editor: a CodeMirror tooltip that shows after a quarter of a second at rest
// on a declaration's name, a type, or a string that names a net, and goes when the pointer moves off
// it. The same card opens at the cursor on a key (`source.card.show`), and closes on Escape or when
// the cursor moves. The card itself is `components/HoverCard.tsx`.

import { type EditorState, type Extension, Prec, StateEffect, StateField } from '@codemirror/state';
import { type EditorView, type Tooltip, ViewPlugin, closeHoverTooltips, hasHoverTooltips, hoverTooltip, keymap, showTooltip } from '@codemirror/view';
import { render } from 'preact';

import { HoverCard, contextFor } from '../components/HoverCard';
import { detail } from '../store';
import { hoverCards } from './prefs';
import { type Hit, type Target, cardFor, targetAt } from './card';
import { symbolsOf } from './symcache';

export const HOVER_MS = 250;

interface Found {
  from: number;
  to: number;
  target: Target;
  /** The document position of the token that names the definition the card is about (the type), where the server is asked for its signature. */
  typeAt?: number;
}

/** The thing under a position of the document, when there is a card for it. */
export function findAt(state: EditorState, path: string, pos: number): Found | null {
  const line = state.doc.lineAt(pos);
  const nets = new Set((detail.peek()?.nets ?? []).map((n) => n.name));
  const hit: Hit | null = targetAt(symbolsOf(state.doc), line.text, line.number, pos - line.from, nets);
  if (!hit) return null;
  if (!cardFor(contextFor(path), hit.target)) return null;
  return { from: line.from + hit.from, to: line.from + hit.to, target: hit.target, typeAt: typeAtOf(hit, line.text, line.from) };
}

/** Where the type of a declaration, or the type hovered, stands in the document. */
function typeAtOf(hit: Hit, text: string, lineFrom: number): number | undefined {
  if (hit.target.type === 'type') return lineFrom + hit.from;
  if (hit.target.type !== 'decl' || !hit.target.sym.typeName) return undefined;
  const at = text.indexOf(hit.target.sym.typeName, hit.to);
  return at < 0 ? undefined : lineFrom + at;
}

function make(found: Found, path: string, above = true): Tooltip {
  return {
    pos: found.from,
    end: found.to,
    above,
    create(view) {
      const dom = document.createElement('div');
      dom.className = 'hc-root cm-card-tip';
      // The window's own Escape (Back, Clear the selection) steps aside for a card that is open; the card is closed by `escapes` below.
      dom.setAttribute('data-esc-closes', '');
      // Shift+Tab from the card's first control goes back to the text; Tab goes on from the last one to what follows. Escape is the editor's (below).
      dom.addEventListener('keydown', (e) => {
        if (e.key !== 'Tab' || !e.shiftKey) return;
        const first = cardControls(dom)[0];
        if (first && e.target === first) {
          e.preventDefault();
          view.focus();
        }
      });
      const close = () => {
        // A card that has the focus gives it back to the text (a button that is gone would leave it on the page's body).
        const had = dom.contains(document.activeElement);
        view.dispatch({ effects: [closeHoverTooltips, openCard.of(null)] });
        if (had) view.focus();
      };
      let gone = false;
      // CodeMirror makes the tooltip while it updates, which can be while a component of the page renders:
      // a render inside that one breaks Preact's hooks, so the card is drawn a moment later. The tooltip is
      // measured again when it has its content, and whenever the content changes size (the rail arrives).
      queueMicrotask(() => {
        if (gone) return;
        render(<HoverCard target={found.target} path={path} close={close} typeAt={found.typeAt} />, dom);
        view.requestMeasure();
      });
      const resize = typeof ResizeObserver === 'function' ? new ResizeObserver(() => view.requestMeasure()) : null;
      resize?.observe(dom);
      return {
        dom,
        destroy() {
          gone = true;
          resize?.disconnect();
          queueMicrotask(() => render(null, dom));
        },
      };
    },
  };
}

/** What in a card takes Tab: its chips and buttons, in the order they are drawn. */
export const cardControls = (root: HTMLElement): HTMLElement[] => [...root.querySelectorAll<HTMLElement>('button:not([disabled]), a[href], [tabindex="0"]')];

/** The card that is open over this editor, whichever way it was opened. */
export const openCardOf = (view: EditorView): HTMLElement | null => view.dom.querySelector<HTMLElement>('.cm-card-tip');

/** Tab from the text moves the focus into the card that is open (and does nothing, so the key indents, when none is). */
export function focusCard(view: EditorView): boolean {
  const card = openCardOf(view);
  if (!card) return false;
  const first = cardControls(card)[0] ?? card.querySelector<HTMLElement>('[role="dialog"]');
  if (!first) return false;
  first.focus();
  return true;
}

/** Open the card at a position (or close it with null): the keyboard's way. */
export const openCard = StateEffect.define<number | null>();

/** Whether the cursor is at a thing a card can describe; for the command's `when`. */
export function cardAtCursor(state: EditorState, path: string): Found | null {
  const head = state.selection.main.head;
  // A cursor at the end of a word is on the word.
  return findAt(state, path, head) ?? (head > 0 ? findAt(state, path, head - 1) : null);
}

export function cardExtension(path: string): Extension {
  const pointer = hoverTooltip(
    (view, pos) => {
      if (!hoverCards.peek()) return null;
      const found = findAt(view.state, path, pos);
      return found ? make(found, path) : null;
    },
    { hoverTime: HOVER_MS, hideOnChange: true },
  );
  const keyboard = StateField.define<Tooltip | null>({
    create: () => null,
    update(value, tr) {
      for (const e of tr.effects) {
        if (!e.is(openCard)) continue;
        if (e.value == null) return null;
        const found = findAt(tr.state, path, e.value) ?? (e.value > 0 ? findAt(tr.state, path, e.value - 1) : null);
        return found ? make(found, path, false) : null;
      }
      return tr.selection || tr.docChanged ? null : value;
    },
    provide: (f) => showTooltip.from(f),
  });
  // Escape closes a card whichever way it was opened. The editor's keymap only hears the key while the editor has the focus, and a card
  // opened by the pointer is open while the focus is anywhere: the plugin listens on the document, before the page's own handlers.
  const escapes = ViewPlugin.define((view) => {
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      if (!hasHoverTooltips(view.state) && !view.state.field(keyboard, false)) return;
      // Escape in the card closes it and the focus goes back to the text.
      const inside = !!openCardOf(view)?.contains(document.activeElement);
      view.dispatch({ effects: [closeHoverTooltips, openCard.of(null)] });
      if (inside) view.focus();
      e.stopPropagation();
    };
    // CodeMirror closes a card the pointer opened when the pointer moves off the word and the card. While the keyboard's focus is in the card (Tab went there)
    // it stays: the pointer's moves are not told to the hover (its listener is on this element, in the bubble phase; this one, in the capture phase, comes first).
    // Only a focus the keyboard gave it counts (`:focus-visible`): a click on a chip also focuses it, and the card then goes when the pointer does.
    const moves = (e: Event) => {
      const a = document.activeElement;
      if (a && openCardOf(view)?.contains(a) && a.matches(':focus-visible')) e.stopPropagation();
    };
    document.addEventListener('keydown', key, true);
    view.dom.addEventListener('mousemove', moves, true);
    return {
      destroy: () => {
        document.removeEventListener('keydown', key, true);
        view.dom.removeEventListener('mousemove', moves, true);
      },
    };
  });
  return [
    pointer,
    keyboard,
    escapes,
    // Tab moves into an open card before it indents.
    Prec.highest(keymap.of([{ key: 'Tab', run: focusCard }])),
    keymap.of([
      {
        key: 'Escape',
        run: (view) => {
          if (!view.state.field(keyboard, false)) return false;
          view.dispatch({ effects: openCard.of(null) });
          return true;
        },
      },
    ]),
  ];
}
