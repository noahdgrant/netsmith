// Folding: any `{` that ends a line folds to its closing `}` (a definition, a declaration with a body,
// a `pins` table). The gutter marks are the page's chevron; a folded block shows how many lines it hides.

import { codeFolding, foldGutter, foldService } from '@codemirror/language';
import type { EditorState, Extension, Text } from '@codemirror/state';

import { tokenize } from '../nsm';
import { iconDom } from './dom';

/** For each line that ends in `{`, the line of the `}` that closes it. */
export function bracePairs(lines: readonly string[]): Map<number, number> {
  const out = new Map<number, number>();
  const open: number[] = [];
  lines.forEach((text, i) => {
    const toks = tokenize(text);
    const braces = toks.filter((t) => t.kind === 'punct' && (text[t.from] === '{' || text[t.from] === '}'));
    for (const t of braces) {
      if (text[t.from] === '{') open.push(i + 1);
      else {
        const start = open.pop();
        // A block that opens and closes on one line has nothing to fold.
        if (start !== undefined && start !== i + 1) out.set(start, i + 1);
      }
    }
  });
  return out;
}

const cache = new WeakMap<Text, Map<number, number>>();
function pairsOf(doc: Text): Map<number, number> {
  let m = cache.get(doc);
  if (!m) {
    const lines: string[] = [];
    for (let n = 1; n <= doc.lines; n++) lines.push(doc.line(n).text);
    m = bracePairs(lines);
    cache.set(doc, m);
  }
  return m;
}

const service = foldService.of((state: EditorState, lineStart: number, lineEnd: number) => {
  const doc = state.doc;
  const n = doc.lineAt(lineStart).number;
  const end = pairsOf(doc).get(n);
  if (!end) return null;
  const last = doc.line(end);
  const close = last.text.indexOf('}');
  return { from: lineEnd, to: last.from + Math.max(0, close) };
});

export const foldExtension: Extension = [
  service,
  codeFolding({
    preparePlaceholder: (state, range) => state.doc.lineAt(range.to).number - state.doc.lineAt(range.from).number,
    placeholderDOM(_view, onclick, prepared) {
      const el = document.createElement('span');
      el.className = 'cm-foldPlaceholder';
      el.textContent = `${prepared} line${prepared === 1 ? '' : 's'}`;
      el.title = 'Unfold';
      el.setAttribute('role', 'button');
      el.onclick = onclick;
      return el;
    },
  }),
  foldGutter({
    markerDOM(open) {
      const el = iconDom('chevron-right', 12, `cm-fold-mark ${open ? '' : 'folded'}`);
      el.title = open ? 'Fold' : 'Unfold';
      return el;
    },
  }),
];
