// The editor the commands act on: the view of the file that is on show, set by the component while
// it is mounted (the way `views/schematic-actions.ts` hands out the schematic's).

import type { EditorView } from '@codemirror/view';

export const sourceEditor: { view: EditorView | null; path: string | null } = { view: null, path: null };
