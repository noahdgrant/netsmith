// `7 lines changed in this file` in the status bar while the source shows the marks of the comparison: how many places
// the file in the editor differs from the file at the ref. A click opens the comparison of the file.

import { compare, openDiff, refText, showChanges } from '../compare/store';
import { sourceChanges } from '../source/cm-changes';
import { plural } from '../util';
import { revealSection } from '../store';
import { Icon } from './Icons';

export function SourceChangesItem({ path }: { path: string }) {
  const s = sourceChanges.value;
  if (!showChanges.value || !compare.value || !s || s.file !== path) return null;
  return (
    <button
      class="item src-changes"
      data-testid="source-changes"
      data-tip={`${plural(s.lines, 'line')} of this file differ from ${refText.value}, in ${plural(s.hunks, 'place')}. Click to show the changes.`}
      aria-label={`${s.lines === 0 ? 'No lines' : plural(s.lines, 'line')} changed in this file since ${refText.value}. Show them.`}
      onClick={() => {
        openDiff({ path, where: 'source' });
        revealSection('source');
      }}
    >
      <Icon name="compare" size={13} />
      <span>{s.lines === 0 ? 'no lines changed in this file' : `${plural(s.lines, 'line')} changed in this file`}</span>
    </button>
  );
}
