// The symbols of a document, scanned once for each text: the hover cards, the folds and the
// completions all ask, and a document is the same object until it is edited.

import type { Text } from '@codemirror/state';

import { type Sym, scanSymbols } from './symbols';

const cache = new WeakMap<Text, Sym[]>();

export function symbolsOf(doc: Text): Sym[] {
  let s = cache.get(doc);
  if (!s) {
    s = scanSymbols(doc.toString());
    cache.set(doc, s);
  }
  return s;
}
