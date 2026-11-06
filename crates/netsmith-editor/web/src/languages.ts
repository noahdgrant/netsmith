// Which language a file of the project is coloured as, chosen by its name. Only `.nsm` is the
// netsmith language; the manifest is TOML and anything else (a catalog's CSV, a note) is text.
// Colouring never draws an error mark on a file that is not `.nsm`.

import { HighlightStyle, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { type EditorView, hoverTooltip } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';

import type { Problem } from './api';
import { nsmHighlight, nsmLanguage, tokenize } from './nsm';

export type FileLanguage = 'nsm' | 'toml' | 'plain';

/** The language of the file at `path`, from its extension (`.physical.nsm` is `.nsm`). */
export function languageOfPath(path: string): FileLanguage {
  const name = (path.split('/').pop() ?? path).toLowerCase();
  if (name.endsWith('.nsm')) return 'nsm';
  if (name.endsWith('.toml')) return 'toml';
  return 'plain';
}

interface TomlState {
  /** The quote that ends a string running over lines (`"""` or `'''`), if inside one. */
  multi: string | null;
}

/** A small TOML colouring: comments, `[tables]`, keys, strings, numbers, dates and the words
 * `true` and `false`. It colours; it does not check, and nothing is marked as wrong. */
export const tomlLanguage = StreamLanguage.define<TomlState>({
  name: 'toml',
  startState: () => ({ multi: null }),
  token(stream, state) {
    if (state.multi) {
      while (!stream.eol()) {
        if (stream.match(state.multi)) {
          state.multi = null;
          break;
        }
        stream.next();
      }
      return 'string';
    }
    if (stream.eatSpace()) return null;
    if (stream.sol() && stream.peek() === '[') {
      stream.skipTo('#') || stream.skipToEnd();
      return 'heading';
    }
    const c = stream.peek();
    if (c === '#') {
      stream.skipToEnd();
      return 'comment';
    }
    if (c === '"' || c === "'") {
      const triple = c.repeat(3);
      if (stream.match(triple)) {
        state.multi = triple;
        while (!stream.eol()) {
          if (stream.match(triple)) {
            state.multi = null;
            break;
          }
          stream.next();
        }
        return 'string';
      }
      stream.next();
      while (!stream.eol()) {
        const d = stream.next();
        if (d === '\\' && c === '"') stream.next();
        else if (d === c) break;
      }
      return 'string';
    }
    if (stream.match(/^[+-]?(\d{4}-\d\d-\d\d([T ]\d\d:\d\d(:\d\d(\.\d+)?)?(Z|[+-]\d\d:\d\d)?)?|\d[\d_]*(\.\d[\d_]*)?([eE][+-]?\d+)?|0x[0-9a-fA-F_]+|inf|nan)(?![\w-])/)) return 'number';
    if (stream.match(/^(true|false)(?![\w-])/)) return 'bool';
    if (stream.match(/^[A-Za-z0-9_.-]+(?=\s*=)/)) return 'propertyName';
    if (stream.match(/^[=,{}\[\]]/)) return 'punctuation';
    stream.next();
    return null;
  },
  languageData: { commentTokens: { line: '#' }, closeBrackets: { brackets: ['[', '{', '"', "'"] } },
});

const tomlHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: t.comment, color: 'var(--syn-comment)', fontStyle: 'italic' },
    { tag: t.heading, color: 'var(--syn-keyword)', fontWeight: '600' },
    { tag: t.propertyName, color: 'var(--syn-property)' },
    { tag: t.string, color: 'var(--syn-string)' },
    { tag: t.number, color: 'var(--syn-number)' },
    { tag: t.bool, color: 'var(--syn-type)' },
    { tag: t.punctuation, color: 'var(--syn-punct)' },
  ]),
);

/** What a hover on a token that `.nsm` colouring marks as not valid says: the checker's own
 * message when it reports a problem there, else that the compiler reports nothing and the mark
 * is the editor's colouring. */
export function explainInvalid(problems: Problem[], path: string, line: number, from: number, to: number, text: string): string {
  const here = problems.find(
    (p) =>
      p.at &&
      p.at.file === path &&
      p.severity !== 'note' &&
      p.at.line <= line &&
      line <= p.at.endLine &&
      (p.at.line < line || p.at.column - 1 <= to) &&
      (p.at.endLine > line || p.at.endColumn - 1 >= from),
  );
  if (here) return here.message.split('\n')[0];
  const shown = text.length > 12 ? `${text.slice(0, 12)}…` : text;
  return `unexpected \`${shown}\`: the compiler reports no error here; this is the editor's syntax colouring, which does not know this spelling`;
}

function invalidTooltip(path: string, problems: () => Problem[]): Extension {
  return hoverTooltip((view: EditorView, pos: number) => {
    const line = view.state.doc.lineAt(pos);
    const at = pos - line.from;
    const token = tokenize(line.text).find((k) => k.kind === 'error' && k.from <= at && at < k.to);
    if (!token) return null;
    const message = explainInvalid(problems(), path, line.number, token.from, token.to, line.text.slice(token.from, token.to));
    return {
      pos: line.from + token.from,
      end: line.from + token.to,
      above: true,
      create() {
        const dom = document.createElement('div');
        dom.className = 'cm-invalid-tip';
        dom.textContent = message;
        return { dom };
      },
    };
  });
}

/** The colouring of the file at `path` and nothing else: a view that only shows text (the comparison with a ref) wants no
 * tooltips on the marks. */
export function colouring(path: string): Extension[] {
  switch (languageOfPath(path)) {
    case 'nsm':
      return [nsmLanguage, nsmHighlight];
    case 'toml':
      return [tomlLanguage, tomlHighlight];
    default:
      return [];
  }
}

/** The extensions that colour the file at `path`. `problems` gives the checker's current ones. */
export function languageExtensions(path: string, problems: () => Problem[]): Extension[] {
  return languageOfPath(path) === 'nsm' ? [...colouring(path), invalidTooltip(path, problems)] : colouring(path);
}
