// The hover card in the editor: a CodeMirror tooltip that shows after a quarter of a second at rest
// on a declaration's name, a type, or a string that names a net, and goes when the pointer moves off
// it. The same card opens at the cursor on a key (`source.card.show`), and closes on Escape or when
// the cursor moves. The card itself is `components/HoverCard.tsx`.

import { type EditorState, type Extension, StateEffect, StateField } from '@codemirror/state';
import { type Tooltip, closeHoverTooltips, hoverTooltip, keymap, showTooltip } from '@codemirror/view';
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
}

/** The thing under a position of the document, when there is a card for it. */
export function findAt(state: EditorState, path: string, pos: number): Found | null {
  const line = state.doc.lineAt(pos);
  const nets = new Set((detail.peek()?.nets ?? []).map((n) => n.name));
  const hit: Hit | null = targetAt(symbolsOf(state.doc), line.text, line.number, pos - line.from, nets);
  if (!hit) return null;
  if (!cardFor(contextFor(path), hit.target)) return null;
  return { from: line.from + hit.from, to: line.from + hit.to, target: hit.target };
}

function make(found: Found, path: string, above = true): Tooltip {
  return {
    pos: found.from,
    end: found.to,
    above,
    create(view) {
      const dom = document.createElement('div');
      dom.className = 'hc-root cm-card-tip';
      const close = () => view.dispatch({ effects: [closeHoverTooltips, openCard.of(null)] });
      let gone = false;
      // CodeMirror makes the tooltip while it updates, which can be while a component of the page renders:
      // a render inside that one breaks Preact's hooks, so the card is drawn a moment later. The tooltip is
      // measured again when it has its content, and whenever the content changes size (the rail arrives).
      queueMicrotask(() => {
        if (gone) return;
        render(<HoverCard target={found.target} path={path} close={close} />, dom);
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
  return [
    pointer,
    keyboard,
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
