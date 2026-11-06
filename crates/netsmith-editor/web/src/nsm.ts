// The `.nsm` language for the editor: a tokenizer written from docs/LANGUAGE.md and the
// lexer in crates/netsmith-lang/src/lexer.rs, and the CodeMirror language that uses it.

import { StreamLanguage, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

export type Kind =
  | 'comment'
  | 'doc'
  | 'string'
  | 'number'
  | 'keyword'
  | 'type'
  | 'member'
  | 'namespace'
  | 'name'
  | 'quoted'
  | 'operator'
  | 'punct'
  | 'error';

export interface Token {
  from: number;
  to: number;
  kind: Kind;
}

/** The words that start a definition or a statement, and the markers after a pin row (`required`, `optional`, ADR 0016). */
export const KEYWORDS = new Set(['board', 'system', 'panel', 'part', 'interface', 'module', 'enum', 'generic', 'provides', 'import', 'catalog', 'bridge', 'required', 'optional']);

// Longest spellings first, as the lexer reads them.
const OPERATORS = ['~>', '&=', '?=', '==', '>=', '<=', '+/-', '..', '::', '~', '+', '-', '*', '/', '=', '>', '<'];

const isDigit = (c: string) => c >= '0' && c <= '9';
const isWordStart = (c: string) => /[\p{L}_]/u.test(c);
const isWord = (c: string) => /[\p{L}\p{N}_]/u.test(c);

/** The tokens of one line, as ranges of the line. Nothing in the language spans lines. */
export function tokenize(line: string): Token[] {
  const out: Token[] = [];
  let i = 0;
  // Whether the next word is a member, because a dot came just before it.
  let afterDot = false;
  const push = (from: number, to: number, kind: Kind) => out.push({ from, to, kind });
  while (i < line.length) {
    const c = line[i];
    if (c === ' ' || c === '\t' || c === '\r') {
      i++;
      continue;
    }
    if (c === '/' && line[i + 1] === '/') {
      const doc = (line[i + 2] === '/' && line[i + 3] !== '/') || line[i + 2] === '!';
      push(i, line.length, doc ? 'doc' : 'comment');
      return out;
    }
    if (c === '/' && line[i + 1] === '*') {
      // Block comments do not exist; the compiler says so.
      push(i, i + 2, 'error');
      i += 2;
      continue;
    }
    if (c === '"') {
      let j = i + 1;
      while (j < line.length && line[j] !== '"') j += line[j] === '\\' ? 2 : 1;
      push(i, Math.min(j + 1, line.length), j < line.length ? 'string' : 'error');
      i = j + 1;
      afterDot = false;
      continue;
    }
    if (c === "'") {
      const j = line.indexOf("'", i + 1);
      if (j < 0) {
        push(i, i + 1, 'error');
        i++;
      } else {
        push(i, j + 1, 'quoted');
        i = j + 1;
      }
      afterDot = false;
      continue;
    }
    if (isDigit(c)) {
      let j = i;
      if (c === '0' && /[xX]/.test(line[i + 1] ?? '') && /[0-9a-fA-F]/.test(line[i + 2] ?? '')) {
        j = i + 2;
        while (/[0-9a-fA-F]/.test(line[j] ?? '')) j++;
      } else {
        while (isDigit(line[j] ?? '')) j++;
        // A fraction needs a digit after the dot, so `1..5` stays a range.
        if (line[j] === '.' && isDigit(line[j + 1] ?? '')) {
          j++;
          while (isDigit(line[j] ?? '')) j++;
        }
        // An exponent needs digits, so `5eV` is five electron-volts.
        if (/[eE]/.test(line[j] ?? '')) {
          let k = j + 1;
          if (line[k] === '+' || line[k] === '-') k++;
          if (isDigit(line[k] ?? '')) {
            while (isDigit(line[k] ?? '')) k++;
            j = k;
          }
        }
        if (line[j] === '%') j++;
        else while (/\p{L}/u.test(line[j] ?? '')) j++;
      }
      push(i, j, 'number');
      i = j;
      afterDot = false;
      continue;
    }
    if (isWordStart(c)) {
      let j = i;
      while (j < line.length && isWord(line[j])) j++;
      const word = line.slice(i, j);
      let kind: Kind;
      if (afterDot) kind = 'member';
      else if (KEYWORDS.has(word)) kind = 'keyword';
      // A name before `::` is a module in a path (`std::parts`) unless it is capitalised, which
      // makes it a type: the enum of a variant, `AssemblyMode::Hand`.
      else if (line.slice(j, j + 2) === '::' && !/^\p{Lu}/u.test(word)) kind = 'namespace';
      else if (/^\p{Lu}/u.test(word)) kind = 'type';
      else kind = 'name';
      push(i, j, kind);
      i = j;
      afterDot = false;
      continue;
    }
    if (c === '.' && line[i + 1] !== '.') {
      // `.a` starts a member name and `a.b` continues a path: both are followed by one.
      push(i, i + 1, 'punct');
      i++;
      afterDot = true;
      continue;
    }
    const op = OPERATORS.find((o) => line.startsWith(o, i));
    if (op) {
      push(i, i + op.length, op === '::' ? 'punct' : 'operator');
      i += op.length;
      afterDot = false;
      continue;
    }
    if ('{}(),;:'.includes(c)) {
      push(i, i + 1, 'punct');
      i++;
      afterDot = false;
      continue;
    }
    push(i, i + 1, 'error');
    i++;
  }
  return out;
}

interface LineState {
  tokens: Token[];
  next: number;
}

const TAG: Record<Kind, string | null> = {
  comment: 'comment',
  doc: 'docComment',
  string: 'string',
  number: 'number',
  keyword: 'keyword',
  type: 'typeName',
  member: 'propertyName',
  namespace: 'namespace',
  name: 'variableName',
  quoted: 'string',
  operator: 'operator',
  punct: 'punctuation',
  error: 'invalid',
};

export const nsmLanguage = StreamLanguage.define<LineState>({
  name: 'nsm',
  startState: () => ({ tokens: [], next: 0 }),
  token(stream, state) {
    if (stream.sol()) {
      state.tokens = tokenize(stream.string);
      state.next = 0;
    }
    const token = state.tokens[state.next];
    if (!token) {
      stream.skipToEnd();
      return null;
    }
    if (token.from > stream.pos) {
      stream.pos = token.from;
      return null;
    }
    state.next++;
    stream.pos = token.to;
    return TAG[token.kind];
  },
  languageData: {
    commentTokens: { line: '//' },
    closeBrackets: { brackets: ['(', '{', '"'] },
    indentOnInput: /^\s*\}$/,
  },
});

export const nsmHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: t.comment, color: '#6c7a89', fontStyle: 'italic' },
    { tag: t.docComment, color: '#7fa37a', fontStyle: 'italic' },
    { tag: t.string, color: '#a6d189' },
    { tag: t.number, color: '#ef9f76' },
    { tag: t.keyword, color: '#e0a458', fontWeight: '600' },
    { tag: t.typeName, color: '#8fb8de' },
    { tag: t.propertyName, color: '#c3a6e0' },
    { tag: t.namespace, color: '#8c97a6' },
    { tag: t.variableName, color: '#d7dae0' },
    { tag: t.operator, color: '#e78284' },
    { tag: t.punctuation, color: '#8c97a6' },
    { tag: t.invalid, color: '#ff6b6b', textDecoration: 'underline wavy' },
  ]),
);
