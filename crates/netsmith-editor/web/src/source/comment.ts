// Comment or uncomment the lines of the selection with the language's line comment (`//` in
// `.nsm`, `#` in TOML). CodeMirror's own toggle takes the `//` of a doc comment (`///`, `//!`) as
// a comment to remove and leaves `! text`; here a doc comment is code that is not commented: to
// comment it out the line gets a `// ` in front of the three slashes, and that is what is removed.

import { type ChangeSpec, type EditorState, type TransactionSpec } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';

/** The line comment token at `pos`. */
export function lineToken(state: EditorState, pos: number): string {
  for (const d of state.languageDataAt<{ line?: string }>('commentTokens', pos)) if (d?.line) return d.line;
  return '//';
}

/** Whether `text` is commented out with `token`: starts with it, and (for `//`) is no doc comment. */
function isCommented(text: string, token: string): boolean {
  const t = text.trimStart();
  if (!t.startsWith(token)) return false;
  if (token === '//') return !/^\/\/[/!]/.test(t) || /^\/\/ /.test(t);
  return true;
}

/** The lines the selection covers (a range that ends at the very start of a line does not take it), first to last, once each. */
function coveredLines(state: EditorState): number[] {
  const out = new Set<number>();
  for (const r of state.selection.ranges) {
    const first = state.doc.lineAt(r.from).number;
    let last = state.doc.lineAt(r.to).number;
    if (!r.empty && r.to === state.doc.line(last).from && last > first) last--;
    for (let n = first; n <= last; n++) out.add(n);
  }
  return [...out].sort((a, b) => a - b);
}

/** The change that comments the covered lines, or uncomments them when every non-blank one is commented; null for nothing to do. */
export function toggleLineCommentChange(state: EditorState): ChangeSpec[] | null {
  const lines = coveredLines(state).map((n) => state.doc.line(n));
  const body = lines.filter((l) => l.text.trim() !== '');
  if (body.length === 0) return null;
  const token = lineToken(state, body[0].from);
  const changes: ChangeSpec[] = [];
  if (body.every((l) => isCommented(l.text, token))) {
    for (const l of body) {
      const at = l.from + (l.text.length - l.text.trimStart().length);
      const after = l.text.slice(at - l.from + token.length);
      changes.push({ from: at, to: at + token.length + (after.startsWith(' ') ? 1 : 0) });
    }
  } else {
    // One column for all of them: the shallowest indent, so the block stays a block.
    const indent = Math.min(...body.map((l) => l.text.length - l.text.trimStart().length));
    for (const l of body) changes.push({ from: l.from + indent, insert: `${token} ` });
  }
  return changes;
}

/** The transaction for the editor: null when it is read-only or has nothing to do. */
export function toggleLineComment(view: EditorView): boolean {
  if (view.state.readOnly) return false;
  const changes = toggleLineCommentChange(view.state);
  if (!changes) return false;
  const spec: TransactionSpec = { changes, scrollIntoView: true, userEvent: 'input.comment' };
  view.dispatch(spec);
  return true;
}
