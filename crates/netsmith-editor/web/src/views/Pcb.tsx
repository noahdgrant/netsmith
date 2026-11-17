import { useEffect, useMemo, useRef, useState } from 'preact/hooks';

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
import { crossedText, reroutedText, shovedText } from '../pcb/walk';
import { viaKind } from '../pcb/routes';
import { FindingsList } from '../pcb/Findings';
import { SummaryTab } from '../pcb/SummaryTab';
import { Panel } from '../pcb/Panel';
import { PanelPreview } from '../pcb/PanelPreview';
import { prepare } from '../pcb/drawing';
import { SIDES, type Side } from '../pcb/side';
import { type Pt, unite } from '../pcb/geometry';
import { POUR_HIDDEN, fills, poursMarkup } from '../pcb/pours';
import { placingLine, placingSvg, routingMarkup } from '../pcb/live';
import { livePlace, liveRoute, liveSeen, placingSaid } from '../pcb/liveState';
import {
  GRIDS,
  findings,
  history,
  keepoutDefaults,
  loadScene,
  cancelPlacing,
  placingCancelled,
  pcbSide,
  pickedFinding,
  pours,
  recheck,
  redo,
  scene,
  selection as pcbSelection,
  wrote,
  sceneError,
  setGrid,
  type Tool,
  tool,
  trackLayer,
  trackShove,
  undo,
  viaSpan,
  crossedShown,
  shoveShown,
  rerouted,
  grid,
} from '../pcbstore';
import { analysis, board, detail, fabReports, job, panelShown, pcbReports, pcbStamp, pickNet, routeBoard } from '../store';
import { partPathsOf, revealRequest, sameSelection, selection } from '../selection';
import { hover as sharedHover } from '../hover';
import { groupsOf, litModules, litOn } from '../pcb/modules';
import { Board3d } from '../pcb3d/Lazy';
import { pcbMode, systemShown } from '../pcb3d/mode';
import { DrawingSwitch } from '../pcb/DrawingSwitch';
import { GerberView } from '../gerber/GerberView';
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
  { id: 'keepouts', label: 'Keepouts' },
  { id: 'limits', label: 'Height limits' },
  { id: 'requirements', label: 'Requirements' },
  { id: 'refs', label: 'References' },
  { id: 'key', label: 'Key' },
];

// Keepouts and height limits are rectangles among the drawing's marks, not groups of their own: a keepout forbids something, a height
// limit (`data-limit-only`, or the label a keepout that forbids and limits has) only limits how tall a part may stand.
const ZONE_HIDDEN: Record<string, string> = {
  keepouts: '.pcb-host svg [data-keepout]:not([data-limit-only])',
  limits: '.pcb-host svg [data-keepout][data-limit-only], .pcb-host svg [data-height-limit]',
  // The windows of the placement requirements and the ghosts, drawn over the board (ADR 0045 C4).
  requirements: '.pcb-host .req-overlay',
};

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

/** The board while the server places it (issue 764): the parts where the placer has them now, the last line it told, and
 * Cancel. Before the first glimpse, the skeleton of a loading view. */
function Placing({ board: name }: { board: string }) {
  const live = livePlace.value?.board === name ? livePlace.value : null;
  const said = placingSaid.value?.board === name ? placingSaid.value.line : null;
  const svg = useMemo(() => placingSvg(live), [live]);
  return (
    <div class="pcb-placing" data-testid="pcb-placing">
      <div class="job">
        <div class="job-head">
          <span class="spinner" />
          <span class="job-line" data-testid="placing-line">
            {placingLine(said, live)}
          </span>
          <button class="tool small" data-testid="cancel-placing" data-tip="Stop placing the board. Nothing it placed is kept" onClick={() => void cancelPlacing()}>
            Cancel
          </button>
        </div>
      </div>
      {svg ? (
        <div class="pcb-live" data-testid="pcb-live" data-parts={live?.parts.length} dangerouslySetInnerHTML={{ __html: svg }} />
      ) : (
        <ViewLoading what="Placing the board" />
      )}
    </div>
  );
}

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
  const followed = useRef<typeof shared | undefined>(undefined);
  useEffect(() => {
    const changed = followed.current !== shared;
    followed.current = shared;
    // A selection the board itself made is already on the board.
    if (wrote.sel !== undefined && sameSelection(wrote.sel, shared)) {
      wrote.sel = undefined;
      return;
    }
    wrote.sel = undefined;
    // Several parts picked on the board leave the page with no selection: a detail read again (the layout
    // file the move wrote was rebuilt) is no reason to drop them. Only a change of the page's selection is.
    if (!changed && shared === null) return;
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
  // The modules lit (and one a row or a block the pointer is on lights), drawn over the board.
  const pointed = sharedHover.value;
  const litNow = litOn(name, litModules.value);
  const moduleGroups = useMemo(
    () => (sc ? groupsOf(sc.parts.map((p) => p.path), litNow, pointed?.kind === 'module' ? pointed.key : null) : []),
    [sc?.parts, litNow, pointed],
  );
  const found = findings.value && findings.value.board === name ? findings.value : null;
  // The pours and zones go into the drawing under the copper. They come after the board (the server fills after it has
  // placed), and while the board changes the last fill stays, marked as not current, rather than the board going bare.
  const poured = pours.value && pours.value.board === name ? pours.value : null;
  const pouredNow = fills(poured, sc);
  // While the board is routed, the copper the router has laid so far goes over it (issue 764).
  const routing = job.value?.task === 'route' && job.value.board === name;
  const laid = routing && liveRoute.value?.board === name ? routingMarkup(liveRoute.value) : '';
  const drawing = useMemo(() => (sc ? prepare(sc.svg, poursMarkup(poured, pouredNow), laid) : null), [sc?.svg, poured, pouredNow, laid]);
  // The planes layer takes the planes' labels with it: a name on nothing is noise.
  // The marks of the routes file's copper go with the layer of their copper.
  const fixedOf: Record<string, string> = { 'front-copper': '[data-layer=top]', 'back-copper': '[data-layer=bottom]', vias: '.fixed-via' };
  const css = useMemo(
    () =>
      hidden
        .map((l) =>
          ZONE_HIDDEN[l]
            ? `${ZONE_HIDDEN[l]}{display:none}`
            : `.pcb-host .l-${l}${l === 'planes' ? ', .pcb-host .l-plane-labels' : ''}${fixedOf[l] ? `, .pcb-host .fixed-shape${fixedOf[l]}` : ''}${POUR_HIDDEN[l] ? `, .pcb-host .l-pours ${POUR_HIDDEN[l]}` : ''}{display:none}`,
        )
        .join(''),
    [hidden],
  );

  if (!name) return <NoBoard what="show" />;
  const fab = fabReports.value[name];
  const report = pcbReports.value[name] ?? null;
  const busy = job.value?.task === 'route';
  const pick = (net: string | null) => pickNet(net === picked ? null : net);
  const h = history.value;
  const activeTool = tool.value;
  // The router's copper the track being drawn crosses, as words, while there is one.
  const shown = crossedShown.value;
  const crossedNow = shown ? crossedText(shown) : null;
  // The router's crossed nets routed again round it, as a preview (issue 1009), once the server has said.
  const again = crossedNow ? (rerouted.value?.answer ?? null) : null;
  // The copper of the routes file it shoves aside, or why it goes round instead (issue 726).
  const shoving = shoveShown.value;
  const shovedNow = shoving ? shovedText(shoving.shoved) : null;
  const stuckNow = shoving?.stuck ?? null;
  const in3d = pcbMode.value === '3d';
  // The 3D view on a system's scene stays while the page's board changes to another member (a part
  // of it picked there): it draws the system, not this board's placement, so the camera stays.
  const sysIn3d = in3d && (analysis.value?.systems ?? []).some((x) => x.name === systemShown.value && x.boards.includes(name));
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
  if (pcbMode.value === 'gerber') {
    return (
      <div class="pcb-view">
        <GerberView board={name} />
      </div>
    );
  }
  return (
    <div class="pcb-view" data-live-place={liveSeen.value.place} data-live-route={liveSeen.value.route}>
      <div class="toolbar pcb-bar" role="toolbar" aria-label="Board">
        {/* First in the bar, so it is where it was whichever view is shown and whatever the groups after it hold (issue 784). */}
        <DrawingSwitch />
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
          <a
            class="tool"
            href={api.pcbKicadUrl(name)}
            download
            data-testid="pcb-kicad"
            data-tip="Download the board as it is placed, not routed, as KiCad files (a zip with the .kicad_pcb, the .kicad_pro and the placement's findings): also while the placement has findings. Not for fabrication"
          >
            <Icon name="download" size={14} />
            <span class="tb-label">Download KiCad board</span>
          </a>
          {busy && (
            <button
              class="tool"
              data-tip="Stop the router; what it has routed is kept. While the board is still placed, the placing stops and nothing is kept"
              onClick={() => {
                void api.cancelRoute();
                void cancelPlacing();
              }}
            >
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
          {!in3d && (
            <Segmented<Side>
              label="Side"
              value={pcbSide.value}
              onChange={(v) => (pcbSide.value = v)}
              options={SIDES.map((v) => ({
                value: v,
                label: v === 'front' ? 'Front' : v === 'back' ? 'Back' : 'Both',
                tip:
                  v === 'front'
                    ? 'The board seen from the front, the back dimmed'
                    : v === 'back'
                      ? 'The board seen from the back, mirrored left to right as KiCad shows it, the front dimmed'
                      : 'Both faces as they are in the file',
                keys: 'B',
                testid: `pcb-side-${v}`,
              }))}
            />
          )}
          {!in3d && (
            <Segmented<Tool>
              label="Tool"
              value={activeTool}
              onChange={(v) => (tool.value = v)}
              options={[
                { value: 'select', label: 'Select', icon: 'cursor', tip: 'Select, move and box select; drag a fixed track\'s end or a via (unlocked copper of the routes file in its way is shoved aside, as the track mode says)', keys: 'Esc' },
                { value: 'keepout', label: 'Keepout', icon: 'keepout', tip: 'Draw a keepout: drag a rectangle', keys: 'K', testid: 'tool-keepout' },
                // Drawn copper goes in the routes file beside the layout file, which a board without a layout file has none of.
                ...(sc?.routesFile
                  ? [
                      {
                        value: 'track' as const,
                        label: 'Track',
                        icon: 'track' as const,
                        tip: 'Draw a track: click a pad, then each corner (unlocked copper of the routes file in the way is shoved aside, and locked copper is gone round, as the mode in the toolbar says; router copper is not an obstacle: it is laid again, and a preview shows it laid round the track); another layer picked while drawing puts a via at the last corner; double-click or Enter ends it, Backspace takes the last corner or layer change off, Esc drops it',
                        keys: 'X',
                        testid: 'tool-track',
                      },
                      { value: 'via' as const, label: 'Via', icon: 'via' as const, tip: 'Place a via: click a pad, a track or a via of the net (unlocked copper of the routes file in its way is shoved aside, as the track mode says; where there is no room the tip says what is in the way)', keys: 'V', testid: 'tool-via' },
                    ]
                  : []),
              ]}
            />
          )}
          {!in3d && activeTool === 'track' && (
            <label class="grid-pick" data-testid="track-options" title="The layer the track is drawn on; its width is the net's by the board's rules">
              <Icon name="layers" size={14} />
              <select
                value={sc && sc.trackLayers.includes(trackLayer.value) ? trackLayer.value : 'top'}
                onChange={(e) => {
                  const el = e.target as HTMLSelectElement;
                  trackLayer.value = el.value;
                  // The keys of the track being drawn (Enter, Esc, Backspace) are the canvas's again.
                  el.blur();
                }}
                data-testid="track-layer"
                aria-label="Track layer"
              >
                {(sc?.trackLayers ?? ['top', 'bottom']).map((l) => (
                  <option key={l} value={l}>
                    {l} layer
                  </option>
                ))}
              </select>
            </label>
          )}
          {!in3d && activeTool === 'via' && sc && sc.viaOffers.length > 1 && (
            <label class="grid-pick" data-testid="via-options" title="The via placed: the through via, or a blind or buried via the board's HDI process makes, by the two layers it ends on; its size is the process's for it">
              <Icon name="via" size={14} />
              <select
                value={sc.viaOffers.some((o) => o.layers.join(',') === viaSpan.value) ? viaSpan.value : ''}
                onChange={(e) => {
                  const el = e.target as HTMLSelectElement;
                  viaSpan.value = el.value;
                  el.blur();
                }}
                data-testid="via-span"
                aria-label="Via span"
              >
                {[...sc.viaOffers.slice(-1), ...sc.viaOffers.slice(0, -1)].map((o) => (
                  <option key={o.layers.join(',')} value={o.layers.join(',')}>
                    {viaKind(o.layers)}, {o.diameter} mm
                  </option>
                ))}
              </select>
            </label>
          )}
          {!in3d && (activeTool === 'track' || activeTool === 'via') && (
            <label
              class="grid-pick"
              data-testid="track-mode"
              title="What a track, a via or a drag of a track's end or a via does about unlocked copper of the routes file in its way: shove it aside (locked copper and pads stay, a track goes round them), or only go round it (a via or a drag is then refused where it has no room)"
            >
              <Icon name="track" size={14} />
              <select
                value={trackShove.value ? 'shove' : 'walk'}
                onChange={(e) => {
                  const el = e.target as HTMLSelectElement;
                  trackShove.value = el.value === 'shove';
                  el.blur();
                }}
                data-testid="track-shove"
                aria-label="Track mode"
              >
                <option value="shove">shove copper aside</option>
                <option value="walk">go round only</option>
              </select>
            </label>
          )}
          {!in3d && activeTool === 'keepout' && (
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
          {!in3d && (
            <>
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
            </>
          )}
          <button class={`icon-btn ${panelOpen ? 'on' : ''}`} data-tip="Show or hide the side panel" aria-label="Panel" aria-pressed={panelOpen} onClick={() => setPanelOpen(!panelOpen)}>
            <Icon name="sidepanel" size={15} />
          </button>
        </div>
      </div>
      <JobProgress task="route" />
      <JobProgress task="drc" />
      <JobProgress task="regenerate" />
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
      {sceneError.value && !sc && !sysIn3d ? (
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
      ) : placingCancelled.value === name && !sc && !sysIn3d ? (
        <EmptyState
          icon="stop"
          title="The placing was cancelled"
          help="Nothing it placed was kept. Place the board again when you want to see it."
          actions={
            <button class="tool primary" data-testid="place-again" onClick={() => void loadScene(name)}>
              Place again
            </button>
          }
        />
      ) : (!sc || !drawing) && !sysIn3d ? (
        sc ? <ViewLoading what="Drawing the board" /> : <Placing board={name} />
      ) : (
        <div class="pcb-main">
          {in3d ? (
            <Board3d board={name} />
          ) : (
            sc &&
            drawing && (
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
                onZoomPlane={(box) => setFocus({ box, seq: (focus?.seq ?? 0) + 1 })}
                cmp={cmpMarks.value?.parts ?? null}
                modules={moduleGroups}
                marks={(found?.findings ?? []).map((f) => ({
                  id: f.id,
                  box: f.area,
                  level: f.severity,
                  on: pickedFinding.value === f.id,
                }))}
              />
            )
          )}
          {tip && !in3d && (
            <div class="pcb-tip" style={{ left: tip.x, top: tip.y }} data-testid="tip">
              <b>{tip.title}</b>
              {tip.lines.map((l) => (
                <div key={l}>{l}</div>
              ))}
            </div>
          )}
          {!in3d && (crossedNow || shovedNow || stuckNow) && (
            <div class="pcb-notes" data-testid="track-notes">
              {shovedNow && (
                <div class="pcb-note" data-testid="track-shoved">
                  <b>Shoves {shovedNow}</b>
                  <div>Unlocked copper of the routes file moves out of the way, in the same edit (one undo). A locked shape stays: a track goes round it.</div>
                </div>
              )}
              {stuckNow && (
                <div class="pcb-note warn" data-testid="track-stuck">
                  <b>{activeTool === 'track' ? 'Goes round, nothing shoved' : 'Nothing shoved'}</b>
                  <div>{stuckNow[0].toUpperCase() + stuckNow.slice(1)}.</div>
                </div>
              )}
              {crossedNow && (
                <div class="pcb-note warn" data-testid="track-crossed">
                  <b>Crosses {crossedNow}</b>
                  <div>An edit throws the router's copper away: route again after it to lay it round the new track.</div>
                  {again && (
                    <div data-testid="track-again" data-left={again.left}>
                      Dashed: {reroutedText(again)} (a preview).
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
          {panelOpen && sc && drawing && (
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
                  id: 'summary',
                  label: 'Summary',
                  body: <SummaryTab key="summary" scene={sc} onFocus={(box) => setFocus({ box: roomAround(box), seq: (focus?.seq ?? 0) + 1 })} />,
                },
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
