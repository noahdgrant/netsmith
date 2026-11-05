import { useEffect, useMemo, useState } from 'preact/hooks';

import type { PcbBox } from '../api';
import { api } from '../client';
import { NoBoard, ViewLoading } from '../components/ViewStatus';
import { pcbFocus } from '../feedback/derive';
import { announce } from '../feedback/announce';
import { routeResult } from '../feedback/results';
import { JobProgress } from '../components/JobProgress';
import { Canvas, type Tip } from '../pcb/Canvas';
import { FindingsList } from '../pcb/Findings';
import { Panel } from '../pcb/Panel';
import { PanelPreview } from '../pcb/PanelPreview';
import { prepare } from '../pcb/drawing';
import type { Pt } from '../pcb/geometry';
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
  sceneError,
  setGrid,
  tool,
  undo,
  grid,
} from '../pcbstore';
import { analysis, board, fabReports, job, panelShown, pcbReports, pcbStamp, routeBoard } from '../store';
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
  const [picked, setPicked] = useState<string | null>(null);
  const [tip, setTip] = useState<Tip | null>(null);
  const [cursor, setCursor] = useState<Pt | null>(null);
  const [focus, setFocus] = useState<{ box: PcbBox; seq: number } | null>(null);

  useEffect(() => {
    if (name) void loadScene(name);
  }, [name, version, stamp]);

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
  const pick = (net: string | null) => setPicked(net === picked ? null : net);
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
      <div class="toolbar">
        {panels.map((p) => (
          <button key={p.name} class="tool" title={`Draw the panel ${p.name} of this board`} onClick={() => (panelShown.value = p.name)}>
            Panel {p.name}
          </button>
        ))}
        <button class="tool primary" disabled={busy} onClick={() => void route(name)}>
          {busy ? 'Routing…' : report?.routed ? 'Route again' : 'Place and route'}
        </button>
        {busy && (
          <button class="tool" title="Stop the router; what it has routed is kept" onClick={() => void api.cancelRoute()}>
            Cancel
          </button>
        )}
        {report && (
          <span class={`chip ${report.unrouted.length || report.placementProblems.length ? 'bad' : 'ok'}`}>
            {report.routed
              ? report.unrouted.length
                ? `${report.stopped ? 'stopped, ' : ''}${report.unrouted.length} unrouted`
                : `routed: ${report.tracks} tracks, ${report.vias} vias`
              : 'placed'}
          </span>
        )}
        {report?.routed && <span class="took">took {seconds(report.elapsedMs)}</span>}
        <span class={`chip ${fab ? (fab.drc === 'passed' ? 'ok' : fab.drc === 'failed' ? 'bad' : '') : ''}`} title="From the last Fab run in the Build tab">
          DRC: {fab ? (fab.drc === 'passed' ? 'passed' : fab.drc === 'failed' ? 'FAILED' : 'not run') : 'run Fab to check'}
        </span>
        {found && (
          <button
            class={`chip ${found.findings.some((f) => f.severity === 'error') ? 'bad' : found.findings.length ? '' : 'ok'}`}
            title="What is wrong with the board, with where: click for the list"
            data-testid="findings-chip"
            onClick={() => {
              setPanelOpen(true);
              setTab('findings');
            }}
          >
            {found.findings.length === 0 ? 'no findings' : `${found.findings.length} finding${found.findings.length === 1 ? '' : 's'}`}
          </button>
        )}
        <span class="sep" />
        <button class="tool" title="Undo (Ctrl+Z)" disabled={h.undo.length === 0} onClick={() => void undo()} data-testid="undo">
          Undo
        </button>
        <button class="tool" title="Redo (Ctrl+Shift+Z)" disabled={h.redo.length === 0} onClick={() => void redo()} data-testid="redo">
          Redo
        </button>
        <span class="sep" />
        <button class={`tool ${activeTool === 'select' ? 'on' : ''}`} title="Select, move and box select (Esc)" onClick={() => (tool.value = 'select')}>
          Select
        </button>
        <button class={`tool ${activeTool === 'keepout' ? 'on' : ''}`} title="Draw a keepout: drag a rectangle (K)" onClick={() => (tool.value = 'keepout')} data-testid="tool-keepout">
          Keepout
        </button>
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
        <span class="spacer" />
        <span class="cursor-at mono" data-testid="cursor">
          {cursor ? `${(Math.round(cursor.x * 100) / 100).toFixed(2)}, ${(Math.round(cursor.y * 100) / 100).toFixed(2)} mm` : ''}
        </span>
        <label class="grid-pick" title="Parts and keepouts snap to this grid while they move">
          Grid
          <select value={grid.value} onChange={(e) => setGrid(Number((e.target as HTMLSelectElement).value))} data-testid="grid">
            {GRIDS.map((g) => (
              <option key={g} value={g}>{mm(g)}</option>
            ))}
          </select>
        </label>
        <details class="layers no-pan">
          <summary>Layers</summary>
          <div class="layer-list">
            {LAYERS.map((l) => (
              <label key={l.id}>
                <input
                  type="checkbox"
                  checked={!hidden.includes(l.id)}
                  onChange={() => setHidden(hidden.includes(l.id) ? hidden.filter((x) => x !== l.id) : [...hidden, l.id])}
                />
                {l.label}
              </label>
            ))}
          </div>
        </details>
        <button class="tool" title="Show or hide the side panel" onClick={() => setPanelOpen(!panelOpen)}>
          Panel
        </button>
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
        <div class="empty bad">{sceneError.value}</div>
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
              onPickNet={(n) => (n === null ? setPicked(null) : pick(n))}
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
