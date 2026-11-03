import { useEffect, useState } from 'preact/hooks';

import type { PanelView } from '../api';
import { api } from '../client';
import { analysis, panelShown } from '../store';

/** A panel drawn (ADR 0040): its boards outlined and named, the V-cut lines, the rails and
 * their marks, and what the panel checks say. Read only: the grid is in the source. */
export function PanelPreview({ name }: { name: string }) {
  const version = analysis.value?.version;
  const [view, setView] = useState<PanelView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .panel(name)
      .then((v) => live && (setView(v), setError(null)))
      .catch((e: Error) => live && (setView(null), setError(e.message)));
    return () => {
      live = false;
    };
  }, [name, version]);

  if (error) return <div class="empty bad">{error}</div>;
  if (!view) return <div class="empty">Reading…</div>;
  const errors = view.findings.filter((f) => f.error);
  return (
    <div class="panel-preview" data-testid="panel-preview">
      <div class="graph-title">
        Panel <b>{view.name}</b>
        <span class="hint">
          {view.columns} x {view.rows} {view.board}, {view.cellWidth} x {view.cellHeight} mm each; the panel is {view.width} x{' '}
          {view.height} mm, V-cut, {view.rail > 0 ? `${view.rail} mm rails` : 'no rails'}. Change the grid in the source.
        </span>
        <button class="tool" onClick={() => (panelShown.value = null)}>
          Show the board
        </button>
      </div>
      <div class="panel-drawing" dangerouslySetInnerHTML={{ __html: view.svg }} />
      <div class="panel-findings" data-testid="panel-findings">
        {view.findings.length === 0 && <div class="chip ok">the panel checks pass</div>}
        {view.findings.map((f) => (
          <div key={f.message} class={`problem ${f.error ? 'error' : 'warning'}`}>
            <span class="sev">{f.error ? '●' : '▲'}</span>
            <span class="msg">{f.message}</span>
          </div>
        ))}
        {errors.length > 0 && <div class="hint">fab refuses this panel until these are fixed.</div>}
      </div>
    </div>
  );
}
