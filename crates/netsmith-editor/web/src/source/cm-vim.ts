// Vim keys for the source editor, from @replit/codemirror-vim: off until chosen (a setting), a
// mode and command line in a bar under the text, and `:w` saves the file like Ctrl+S.

import type { Extension } from '@codemirror/state';
import { Vim, vim } from '@replit/codemirror-vim';

import { save } from '../store';
import { sourceEditor } from './handle';

let defined = false;

function defineExCommands() {
  if (defined) return;
  defined = true;
  const write = () => {
    if (sourceEditor.path) void save(sourceEditor.path);
  };
  Vim.defineEx('write', 'w', write);
}

/** The extension for a state of the setting: nothing when it is off. */
export function vimExtension(on: boolean): Extension {
  if (!on) return [];
  defineExCommands();
  return vim({ status: true });
}
