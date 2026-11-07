// The errors and warnings of each file, for the mark in the file's row in the tree and on its tab. A note is not counted: it gets no mark.

import { computed } from '@preact/signals';

import { problems } from './store';

export const fileProblems = computed(() => {
  const by = new Map<string, { error: number; warning: number }>();
  for (const p of problems.value) {
    if (!p.at || p.severity === 'note') continue;
    const e = by.get(p.at.file) ?? { error: 0, warning: 0 };
    e[p.severity]++;
    by.set(p.at.file, e);
  }
  return by;
});
