// What the outlines on the schematic mean, as a chip in its toolbar: added, changed, renumbered parts and
// the nets that changed, with how many of each the board has; shown while the marks are on.

import { marks, refText } from '../compare/store';
import { plural } from '../util';

export function CompareLegend() {
  const m = marks.value;
  if (!m) return null;
  const n = { added: 0, changed: 0, renumbered: 0 };
  for (const v of m.parts.values()) n[v]++;
  const removed = m.removedParts.length;
  return (
    <span class="cmp-legend" role="list" aria-label={`Changes since ${refText.value}`} data-testid="cmp-legend">
      {n.added > 0 && (
        <span class="lg-item lg-added" role="listitem" data-tip="Parts that are new since the ref">
          <i aria-hidden="true" />
          {n.added} added
        </span>
      )}
      {n.changed > 0 && (
        <span class="lg-item lg-changed" role="listitem" data-tip="Parts with another value, package or number">
          <i aria-hidden="true" />
          {n.changed} changed
        </span>
      )}
      {n.renumbered > 0 && (
        <span class="lg-item lg-renumbered" role="listitem" data-tip="Parts that only have another designator: adding a part renumbers the ones after it">
          <i aria-hidden="true" />
          {n.renumbered} renumbered
        </span>
      )}
      {m.nets.size > 0 && (
        <span class="lg-item lg-net" role="listitem" data-tip="Nets that gained or lost pins, or were renamed">
          <i aria-hidden="true" />
          {plural(m.nets.size, 'net')}
        </span>
      )}
      {removed > 0 && (
        <span class="lg-item lg-removed" role="listitem" data-tip="Parts that are gone: listed in the details, under Removed">
          {removed} removed
        </span>
      )}
      {n.added + n.changed + n.renumbered + m.nets.size + removed === 0 && <span class="lg-item">no changes on this board</span>}
    </span>
  );
}
