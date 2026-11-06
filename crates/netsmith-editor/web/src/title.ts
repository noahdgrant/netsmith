// The browser tab's title: `netsmith - <board>`, with a dot in front while a file has changes that are not saved, so a tab in a
// row of tabs says which project it is and whether closing it would lose something.

import { effect } from '@preact/signals';

import { anyDirty, board, project } from './store';

/** What the tab says. The board when there is one, else the project's title, else just the name. */
export function titleFor(o: { board: string | null; project: string | null; dirty: boolean }): string {
  const what = o.board || o.project;
  return `${o.dirty ? '● ' : ''}netsmith${what ? ` - ${what}` : ''}`;
}

/** Keep `document.title` in step with the board, the project and the unsaved files. */
export function installTitle(): () => void {
  return effect(() => {
    document.title = titleFor({ board: board.value, project: project.value?.title ?? null, dirty: anyDirty.value });
  });
}
