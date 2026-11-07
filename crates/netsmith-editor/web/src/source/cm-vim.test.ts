import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';

import { vimExtension } from './cm-vim';

describe('vimExtension', () => {
  it('is nothing while the setting is off', () => {
    expect(vimExtension(false)).toEqual([]);
  });

  it('makes a state that can be built, with the keys in it, when it is on', () => {
    const on = EditorState.create({ doc: 'a', extensions: [vimExtension(true)] });
    const off = EditorState.create({ doc: 'a', extensions: [vimExtension(false)] });
    expect(on.doc.toString()).toBe('a');
    expect(on.facet(EditorState.readOnly)).toBe(false);
    expect(vimExtension(true)).not.toEqual([]);
    expect(off.doc.toString()).toBe('a');
  });
});
