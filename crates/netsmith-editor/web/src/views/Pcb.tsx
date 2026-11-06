import { useEffect, useMemo, useState } from 'preact/hooks';

import type { PcbBox } from '../api';
import { api } from '../client';
import { NoBoard, ViewLoading } from '../components/ViewStatus';
import { goToFirstError, pcbFocus } from '../feedback/derive';
import { filter, setDockOpen } from '../feedback/state';
import { announce } from '../feedback/announce';
import { routeResult } from '../feedback/results';
import { JobProgress } from '../components/JobProgress';
import { ChangesSwitch } from '../components/ChangesSwitch';
import { marks as cmpMarks } from '../compare/store';
import { Icon } from '../components/Icons';
import { EmptyState, Segmented, Spinner } from '../components/ui';
import { Canvas, type Tip } from '../pcb/Canvas';
import { FindingsList } from '../pcb/Findings';
import { Panel } from '../pcb/Panel';
import { PanelPreview } from '../pcb/PanelPreview';
import { prepare } from '../pcb/drawing';
import { type Pt, unite } from '../pcb/geometry';
import {
  GRIDS,
  findings,
  history,
  keepoutDefaults,
  loadScene,
  pickedFinding,
  recheck,
  redo,
  scene,
  selection as pcbSelection,
  wrote,
  sceneError,
  setGrid,
  tool,
  undo,
  grid,
} from '../pcbstore';
import { analysis, board, detail, fabReports, job, panelShown, pcbReports, pcbStamp, pickNet, routeBoard } from '../store';
import { partPathsOf, revealRequest, sameSelection, selection } from '../selection';
import { seconds } from '../util';

const LAYERS: { id: string; label: string }[] = [
  { id: 'front-copper', label: 'Front copper' },
  { id: 'back-copper', label: 'Back copper' },
  { id: 'vias', label: 'Vias' },
  { id: 'pads', label: 'Pads' },
  { id: 'planes', label: 'Planes' },
  { id: 'ratsnest', label: 'Ratsnest' },
  { id: 'silk', label: 'Silkscreen' },
  { id: 'fab', label: 'Fab' },
  { id: 'courtyard', label: 'Courtyard' },
  { id: 'refs', label: 'References' },
  { id: 'key', label: 'Key' },
];

/** The box with room round it, so a zoom to a small part still shows what is near it. */
/** Route the board, and say how it went when it is done, wherever the person is by then. */
async function route(name: string) {
  const report = await routeBoard(name);
  if (report) announce(routeResult(report));
}

function roomAround(b: PcbBox): PcbBox {
  const [w, h] = [Math.max(b.width, 16), Math.max(b.height, 12)];
  return { x: b.x + b.width / 2 - w / 2, y: b.y + b.height / 2 - h / 2, width: w, height: h };
}

const mm = (v: number) => (v === 0 ? 'no grid' : `${v} mm`);

export function Pcb() {
  const name = board.value;
  const version = analysis.value?.version;
  const stamp = pcbStamp.value;
  const [hidden, setHidden] = useState<string[]>([]);
  const [panelOpen, setPanelOpen] = useState(true);
  const [tab, setTab] = useState('parts');
  const [hover, setHover] = useState<string | null>(null);
  const [tip, setTip] = useState<Tip | null>(null);
  const [cursor, setCursor] = useState<Pt | null>(null);
  const [focus, setFocus] = useState<{ box: PcbBox; seq: number } | null>(null);

  useEffect(() => {
    if (name) void loadScene(name);
  }, [name, version, stamp]);

  // The net picked is the page's, in every view. The parts picked on the board (the canvas moves
  // and edits them) follow the shared selection, and a click on a part there writes it back.
  const shared = selection.value;
  const picked = shared?.kind === 'net' ? shared.key : null;
  const loaded = scene.value;
  useEffect(() => {
    // A selection the board itself made is already on the board.
    if (wrote.sel !== undefined && sameSelection(wrote.sel, shared)) {
      wrote.sel = undefined;
      return;
    }
    wrote.sel = undefined;
    const want = partPathsOf(detail.value, shared);
    const have = pcbSelection.peek();
    if (want.length !== have.length || want.some((p, i) => p !== have[i])) pcbSelection.value = want;
  }, [shared, detail.value, loaded?.board]);
  // Asked to show the selection: zoom to its parts.
  const ask = revealRequest.value;
  useEffect(() => {
    const sc = scene.peek();
    if (!ask || !sc || sc.board !== name) return;
    const paths = new Set(partPathsOf(detail.peek(), ask.sel));
    const box = unite(sc.parts.filter((p) => paths.has(p.path)).map((p) => p.courtyard));
    if (box) setFocus({ box: roomAround(box), seq: (focus?.seq ?? 0) + 1 });
  }, [ask?.nonce]);
  // A problem in the dock asks the board to zoom to where it is.
  const asked = pcbFocus.value;
  useEffect(() => {
    if (!asked) return;
    pcbFocus.value = null;
    setFocus((was) => ({ box: roomAround(asked.box), seq: (was?.seq ?? 0) + 1 }));
  }, [asked?.nonce]);

  const sc = scene.value && scene.value.board === name ? scene.value : null;
  const found = findings.value && findings.value.board === name ? findings.value : null;
  const drawing = useMemo(() => (sc ? prepare(sc.svg) : null), [sc?.svg]);
  const css = useMemo(() => hidden.map((l) => `.pcb-host .l-${l}{display:none}`).join(''), [hidden]);

  if (!name) return <NoBoard what="show" />;
  const fab = fabReports.value[name];
  const report = pcbReports.value[name] ?? null;
  const busy = job.value?.task === 'route';
  const pick = (net: string | null) => pickNet(net === picked ? null : net);
  const h = history.value;
  const activeTool = tool.value;
  const defaults = keepoutDefaults.value;
  // The panels that repeat this board (ADR 0040), and the one being shown.
  const definition = analysis.value?.boards.find((b) => b.name === name)?.definition;
  const panels = (analysis.value?.panels ?? []).filter((p) => p.board === definition);
  const shownPanel = panels.find((p) => p.name === panelShown.value);
  if (shownPanel) {
    return (
      <div class="pcb-view">
        <PanelPreview name={shownPanel.name} />
      </div>
    );
  }
  return (
    <div class="pcb-view">
      <div class="toolbar pcb-bar" role="toolbar" aria-label="Board">
        <div class="tb-group">
          {panels.map((p) => (
            <button key={p.name} class="tool" title={`Draw the panel ${p.name} of this board`} onClick={() => (panelShown.value = p.name)}>
              <Icon name="panel" size={14} />
              Panel {p.name}
            </button>
          ))}
          <button class="tool primary" disabled={busy} onClick={() => void route(name)}>
            {busy ? <Spinner size={13} /> : <Icon name="route" size={14} />}
            {busy ? 'Routing…' : report?.routed ? 'Route again' : 'Place and route'}
          </button>
          {busy && (
            <button class="tool" data-tip="Stop the router; what it has routed is kept" onClick={() => void api.cancelRoute()}>
              Cancel
            </button>
          )}
        </div>
        <div class="tb-group status">
          {report && (
            <span class={`chip ${report.unrouted.length || report.placementProblems.length ? 'bad' : 'ok'}`}>
              {report.routed
                ? report.unrouted.length
                  ? `${report.stopped ? 'stopped, ' : ''}${report.unrouted.length} unrouted`
                  : `routed: ${report.tracks} tracks, ${report.vias} vias`
                : 'placed'}
            </span>
          )}
          {report?.routed && <span class="took tb-took">took {seconds(report.elapsedMs)}</span>}
          <span class={`chip tb-drc ${fab ? (fab.drc === 'passed' ? 'ok' : fab.drc === 'failed' ? 'bad' : '') : ''}`} title="From the last Fab run in the Build tab">
            DRC: {fab ? (fab.drc === 'passed' ? 'passed' : fab.drc === 'failed' ? 'FAILED' : 'not run') : 'run Fab to check'}
          </span>
          {found && (
            <button
              class={`chip findings-pill ${found.findings.some((f) => f.severity === 'error') ? 'bad' : found.findings.length ? 'warn' : 'ok'}`}
              title="What is wrong with this board. Click: the Problems dock opens, showing this view's problems (Ctrl+J). The Findings tab of the side panel lists the same findings."
              aria-label={`${found.findings.length === 0 ? 'No findings' : `${found.findings.length} finding${found.findings.length === 1 ? '' : 's'}`} on this board. Show them in the Problems dock.`}
              data-testid="findings-chip"
              onClick={() => {
                filter.value = { ...filter.value, thisView: true };
                setDockOpen(true);
              }}
            >
              <Icon name={found.findings.length === 0 ? 'check-circle' : found.findings.some((f) => f.severity === 'error') ? 'error' : 'warning'} size={12} />
              {found.findings.length === 0 ? 'no findings' : `${found.findings.length} finding${found.findings.length === 1 ? '' : 's'}`}
            </button>
          )}
        </div>
        <span class="spacer" />
        <div class="tb-group">
          <Segmented<'select' | 'keepout'>
            label="Tool"
            value={activeTool === 'keepout' ? 'keepout' : 'select'}
            onChange={(v) => (tool.value = v)}
            options={[
              { value: 'select', label: 'Select', icon: 'cursor', tip: 'Select, move and box select', keys: 'Esc' },
              { value: 'keepout', label: 'Keepout', icon: 'keepout', tip: 'Draw a keepout: drag a rectangle', keys: 'K', testid: 'tool-keepout' },
            ]}
          />
          {activeTool === 'keepout' && (
            <span class="keepout-options" data-testid="keepout-options">
              <select
                value={defaults.layers}
                onChange={(e) => (keepoutDefaults.value = { ...defaults, layers: (e.target as HTMLSelectElement).value })}
                title="The copper layers it applies to"
              >
                <option value="all">all layers</option>
                {(sc?.layers ?? []).map((l) => (
                  <option key={l} value={l}>{l}</option>
                ))}
              </select>
              {(['tracks', 'vias', 'copper', 'parts'] as const).map((f) => (
                <label key={f} title={`Forbid ${f}`}>
                  <input
                    type="checkbox"
                    checked={defaults[f]}
                    onChange={() => {
                      const next = { ...defaults, [f]: !defaults[f] };
                      if (next.tracks || next.vias || next.copper || next.parts) keepoutDefaults.value = next;
                    }}
                  />
                  {f}
                </label>
              ))}
            </span>
          )}
          <span class="tb-sep" aria-hidden="true" />
          <button class="icon-btn" data-tip="Undo" data-tip-keys="Ctrl+Z" aria-label="Undo" disabled={h.undo.length === 0} onClick={() => void undo()} data-testid="undo">
            <Icon name="undo" size={15} />
          </button>
          <button class="icon-btn" data-tip="Redo" data-tip-keys="Ctrl+Shift+Z" aria-label="Redo" disabled={h.redo.length === 0} onClick={() => void redo()} data-testid="redo">
            <Icon name="redo" size={15} />
          </button>
        </div>
        <div class="tb-group">
          <span class="cursor-at mono" data-testid="cursor">
            {cursor ? `${(Math.round(cursor.x * 100) / 100).toFixed(2)}, ${(Math.round(cursor.y * 100) / 100).toFixed(2)} mm` : ''}
          </span>
          <label class="grid-pick" title="Parts and keepouts snap to this grid while they move">
            <Icon name="grid" size={14} />
            <select value={grid.value} onChange={(e) => setGrid(Number((e.target as HTMLSelectElement).value))} data-testid="grid" aria-label="Grid">
              {GRIDS.map((g) => (
                <option key={g} value={g}>{mm(g)}</option>
              ))}
            </select>
          </label>
          <ChangesSwitch />
          <details class="layers no-pan">
            <summary>
              <Icon name="layers" size={14} />
              <span class="tb-label">Layers</span>
              {hidden.length > 0 && <span class="pl pl-neutral">{LAYERS.length - hidden.length}/{LAYERS.length}</span>}
            </summary>
            <div class="layer-list">
              <div class="layer-head">
                <span>Layers</span>
                <button type="button" class="link" onClick={() => setHidden([])}>
                  Show all
                </button>
              </div>
              {LAYERS.map((l) => (
                <label key={l.id} class={hidden.includes(l.id) ? 'off' : ''}>
                  <input
                    type="checkbox"
                    checked={!hidden.includes(l.id)}
                    onChange={() => setHidden(hidden.includes(l.id) ? hidden.filter((x) => x !== l.id) : [...hidden, l.id])}
                  />
                  <i class={`lsw lsw-${l.id}`} aria-hidden="true" />
                  {l.label}
                </label>
              ))}
            </div>
          </details>
          <button class={`icon-btn ${panelOpen ? 'on' : ''}`} data-tip="Show or hide the side panel" aria-label="Panel" aria-pressed={panelOpen} onClick={() => setPanelOpen(!panelOpen)}>
            <Icon name="sidepanel" size={15} />
          </button>
        </div>
      </div>
      <JobProgress task="route" />
      <JobProgress task="drc" />
      {report && (report.unrouted.length > 0 || report.placementProblems.length > 0) && (
        <div class="banner bad">
          {report.placementProblems.map((p) => (
            <div key={p}>{p}</div>
          ))}
          {report.unrouted.slice(0, 8).map((p) => (
            <div key={p}>unrouted: {p}</div>
          ))}
          {report.unrouted.length > 8 && <div>and {report.unrouted.length - 8} more</div>}
        </div>
      )}
      {sceneError.value && !sc ? (
        <EmptyState
          icon="error"
          tone="bad"
          title="The board could not be placed"
          help="Nothing was drawn. The problems list says why, and the source is where to fix it."
          actions={
            <>
              <button class="tool primary" onClick={goToFirstError}>
                Go to error
              </button>
              <button class="tool" onClick={() => setDockOpen(true)}>
                Show problems
              </button>
            </>
          }
        >
          <div class="empty-error">
            <span class="text">{sceneError.value}</span>
          </div>
        </EmptyState>
      ) : !sc || !drawing ? (
        <ViewLoading what="Placing the board" />
      ) : (
        <div class="pcb-main">
          <Canvas
            drawing={drawing}
            scene={sc}
            css={css}
            litNet={hover ?? picked}
            onHoverNet={setHover}
            onPickNet={pick}
            onTip={setTip}
            onCursor={setCursor}
            focus={focus}
            cmp={cmpMarks.value?.parts ?? null}
            marks={(found?.findings ?? []).map((f) => ({
              id: f.id,
              box: f.area,
              level: f.severity,
              on: pickedFinding.value === f.id,
            }))}
          />
          {tip && (
            <div class="pcb-tip" style={{ left: tip.x, top: tip.y }} data-testid="tip">
              <b>{tip.title}</b>
              {tip.lines.map((l) => (
                <div key={l}>{l}</div>
              ))}
            </div>
          )}
          {panelOpen && (
            <Panel
              scene={sc}
              drawing={drawing}
              picked={picked}
              onPickNet={(n) => (n === null ? pickNet(null) : pick(n))}
              onHoverNet={setHover}
              onFocus={(box) => setFocus({ box: roomAround(box), seq: (focus?.seq ?? 0) + 1 })}
              tab={tab}
              onTab={setTab}
              extra={[
                {
                  id: 'findings',
                  label: `Findings${found ? ` (${found.findings.length})` : ''}`,
                  body: (
                    <FindingsList
                      key="findings"
                      scene={sc}
                      onFocus={(box) => setFocus({ box: roomAround(box), seq: (focus?.seq ?? 0) + 1 })}
                      onRecheck={() => void recheck(name)}
                    />
                  ),
                },
              ]}
            />
          )}
        </div>
      )}
    </div>
  );
}
