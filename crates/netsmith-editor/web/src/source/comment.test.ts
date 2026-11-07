import { EditorSelection, EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';

import { tomlLanguage } from '../languages';
import { nsmLanguage } from '../nsm';
import { toggleLineCommentChange } from './comment';

function toggle(doc: string, from = 0, to = from, extensions: import('@codemirror/state').Extension[] = [nsmLanguage.extension]): string {
  let state = EditorState.create({ doc, selection: EditorSelection.single(from, to), extensions });
  const changes = toggleLineCommentChange(state);
  if (!changes) return doc;
  state = state.update({ changes }).state;
  return state.doc.toString();
}

describe('toggleLineCommentChange', () => {
  it('comments a line and uncomments it again', () => {
    const once = toggle('.r1: Resistor');
    expect(once).toBe('// .r1: Resistor');
    expect(toggle(once)).toBe('.r1: Resistor');
  });

  it('comments every line of a selection at the shallowest indent, skipping blank lines', () => {
    const doc = 'module M {\n    .a: A\n\n      .b: B\n}';
    const from = doc.indexOf('.a');
    const to = doc.indexOf('.b') + 2;
    expect(toggle(doc, from, to)).toBe('module M {\n    // .a: A\n\n    //   .b: B\n}');
  });

  it('uncomments the lines when all are commented, and comments all when only some are', () => {
    expect(toggle('// a\n// b', 0, 9)).toBe('a\nb');
    expect(toggle('// a\nb', 0, 6)).toBe('// // a\n// b');
  });

  it('does not take the last line of a selection that stops at its start', () => {
    expect(toggle('a\nb', 0, 2)).toBe('// a\nb');
  });

  it('treats a doc comment as code: it is commented out with a // in front, and that is what goes', () => {
    expect(toggle('/// docs')).toBe('// /// docs');
    expect(toggle('// /// docs')).toBe('/// docs');
    expect(toggle('//! module docs')).toBe('// //! module docs');
  });

  it('keeps a plain comment that has no space after the slashes', () => {
    expect(toggle('//note')).toBe('note');
  });

  it('uses the # of a TOML file', () => {
    expect(toggle('a = 1', 0, 0, [tomlLanguage.extension])).toBe('# a = 1');
    expect(toggle('# a = 1', 0, 0, [tomlLanguage.extension])).toBe('a = 1');
  });

  it('does nothing on blank lines', () => {
    expect(toggle('\n\n')).toBe('\n\n');
  });
});
