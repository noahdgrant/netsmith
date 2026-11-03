// The findings list: what is wrong with the board, with where. A click zooms to the finding and
// marks it on the board.

import type { PcbBox, PcbFinding, PcbScene } from '../api';
import { checking, findings, pickedFinding, select } from '../pcbstore';

const SOURCE = { placement: 'netsmith', unrouted: 'netsmith', drc: 'netsmith', kicad: 'KiCad' } as const;

function fmt(v: number): string {
  return (Math.round(v * 100) / 100).toFixed(2);
}

export function FindingsList(props: { scene: PcbScene; onFocus: (box: PcbBox) => void; onRecheck: () => void }) {
  const f = findings.value;
  const sc = props.scene;
  const open = (item: PcbFinding) => {
    pickedFinding.value = pickedFinding.value === item.id ? null : item.id;
    if (pickedFinding.value === null) return;
    select(sc.parts.filter((p) => item.parts.includes(p.refdes)).map((p) => p.path));
    props.onFocus(item.area);
  };
  return (
    <div class="findings" data-testid="findings">
      <div class="findings-head">
        <button class="tool" disabled={checking.value} onClick={props.onRecheck} title="Ask KiCad's design rules check as well (netsmith's own runs by itself; KiCad is optional)" data-testid="recheck">
          {checking.value ? 'Checking…' : 'Also ask KiCad'}
        </button>
        {f && <span class="faint">{f.kicad === 'ran' ? 'with KiCad' : f.kicad === 'unavailable' ? 'KiCad unavailable' : 'KiCad not run'}</span>}
      </div>
      {f && f.kicad === 'unavailable' && <div class="note-line bad">{f.kicadNote}</div>}
      {f && !f.routed && <div class="note-line faint">Not routed: the connections still to make are not findings yet.</div>}
      <div class="rows">
        {(f?.findings ?? []).map((item) => (
          <div
            key={item.id}
            class={`finding ${item.severity} ${pickedFinding.value === item.id ? 'sel' : ''}`}
            data-testid="finding"
            onClick={() => open(item)}
          >
            <div class="finding-top">
              <span class={`dot ${item.severity}`} />
              <b class="rule">{item.rule}</b>
              <span class="tag">{SOURCE[item.source]}</span>
              <span class="where mono">
                {fmt(item.x - sc.originX)}, {fmt(item.y - sc.originY)}
              </span>
            </div>
            <div class="finding-msg" title={item.message}>{item.message}</div>
          </div>
        ))}
        {f && f.findings.length === 0 && <div class="none">Nothing found{f.kicad === 'ran' ? ' by netsmith or KiCad' : ''}.</div>}
        {!f && <div class="none">Checking…</div>}
      </div>
    </div>
  );
}
