// The 3D view's ways of looking, as controls (P4b of issue 430): the x-ray switch, the "colour by" menu,
// the height preview's switch and its height, the legend of the colouring, and the label that says
// the preview is on. The state and its commands are in inspectMode.ts; the drawing in viewer.ts.

import { useEffect, useState } from 'preact/hooks';

import { Icon } from '../components/Icons';
import { KINDS, MARGIN_EASY_MM, kindCounts, marginBand } from './inspect';
import { COLOUR_BY, type ColourBy, PREVIEW_MAX_MM, PREVIEW_MIN_MM, colourBy, heightPreview, previewHeight, togglePreview, toggleXray, xray } from './inspectMode';
import { type LimitInfo, type PartInfo3d, mm } from './model';
import './inspect.css';

export function InspectBar() {
  const xr = xray.value;
  const cb = colourBy.value;
  const pv = heightPreview.value;
  const [text, setText] = useState(String(pv.mm));
  const [bad, setBad] = useState(false);
  useEffect(() => {
    setText(String(pv.mm));
    setBad(false);
  }, [pv.mm]);
  const commit = () => {
    const v = previewHeight(text);
    if (v === null) {
      setBad(true);
      return;
    }
    setBad(false);
    heightPreview.value = { ...heightPreview.peek(), mm: v };
  };
  return (
    <div class="v3-inspect no-pan" role="toolbar" aria-label="Ways of looking at the board" data-toast-clear>
      <button
        class={`tool small v3-tog ${xr ? 'on' : ''}`}
        aria-pressed={xr}
        data-tip="X-ray: see through the board, the mask and the bodies, to the inner copper, the vias and the pads under parts"
        data-tip-keys="V"
        data-testid="view-xray"
        onClick={toggleXray}
      >
        <Icon name="layers" size={14} />
        X-ray
      </button>
      <span class="tb-sep" aria-hidden="true" />
      <label class="v3-colour" data-tip={COLOUR_BY.find((c) => c.id === cb)?.tip} data-tip-keys="C">
        <span>Colour by</span>
        <select data-testid="colour-by" value={cb} onChange={(e) => (colourBy.value = (e.target as HTMLSelectElement).value as ColourBy)}>
          {COLOUR_BY.map((c) => (
            <option key={c.id} value={c.id}>
              {c.label}
            </option>
          ))}
        </select>
      </label>
      <span class="tb-sep" aria-hidden="true" />
      <button
        role="switch"
        aria-checked={pv.on}
        class={`v3-switch ${pv.on ? 'on' : ''}`}
        data-tip="Height preview: stand the parts of unknown height up at this height, in the picture only. No check, finding, number or file reads it, and it is off again after a reload."
        data-tip-keys="H"
        data-testid="height-preview"
        onClick={togglePreview}
      >
        <i class="v3-track" aria-hidden="true" />
        Preview at
      </button>
      <input
        class={`v3-mm mono ${bad ? 'bad' : ''}`}
        type="text"
        inputMode="decimal"
        size={4}
        value={text}
        aria-label={`The preview's height in mm, ${PREVIEW_MIN_MM} to ${PREVIEW_MAX_MM}`}
        aria-invalid={bad}
        title={bad ? `A height from ${PREVIEW_MIN_MM} to ${PREVIEW_MAX_MM} mm` : undefined}
        data-testid="preview-mm"
        onInput={(e) => setText((e.target as HTMLInputElement).value)}
        onChange={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            commit();
            (e.target as HTMLInputElement).blur();
          } else if (e.key === 'Escape') {
            setText(String(heightPreview.peek().mm));
            setBad(false);
            (e.target as HTMLInputElement).blur();
          }
        }}
      />
      <span class="v3-unit">mm</span>
    </div>
  );
}

/** Said over the picture while the preview is on, so the stood-up parts are never read as data. */
export function PreviewLabel({ mm: at, count }: { mm: number; count: number }) {
  return (
    <div class="v3-preview-tag" role="status" data-testid="preview-label">
      <b>Preview</b>
      <span>
        {count === 0 ? 'No part has an unknown height' : `${count} ${count === 1 ? 'part' : 'parts'} of unknown height drawn ${mm(at)} tall`} · not a height, not checked
      </span>
    </div>
  );
}

/** The legend of colour by kind or by height margin. */
export function InspectLegend({ by, parts, limits }: { by: ColourBy; parts: PartInfo3d[]; limits: LimitInfo[] }) {
  const known = parts.filter((p) => p.provenance !== 'unknown');
  const unknown = parts.length - known.length;
  const limitKeys = (
    <>
      {limits.some((l) => !l.violated) && (
        <span>
          <i class="v3-sw v3-sw-limit" aria-hidden="true" />
          Height limit
        </span>
      )}
      {limits.some((l) => l.violated) && (
        <span>
          <i class="v3-sw v3-sw-over" aria-hidden="true" />
          Limit exceeded
        </span>
      )}
    </>
  );
  const unknownKey = unknown > 0 && (
    <span>
      <i class="v3-sw v3-sw-unknown" aria-hidden="true" />
      Unknown height {unknown}
    </span>
  );
  if (by === 'kind') {
    return (
      <div class="v3-legend v3-legend-wrap" aria-label="The kind of each part" data-testid="kind-legend">
        {kindCounts(known).map((k) => (
          <span key={k.id} data-kind={k.id}>
            <i class="v3-sw" style={{ background: `var(${KINDS.find((x) => x.id === k.id)!.token})` }} aria-hidden="true" />
            {k.label} {k.count}
          </span>
        ))}
        {unknownKey}
        {limitKeys}
      </div>
    );
  }
  const bands = { over: 0, tight: 0, easy: 0, none: 0 };
  for (const p of known) bands[marginBand(p.marginMm)]++;
  return (
    <div class="v3-legend v3-legend-col" aria-label="The height map: each body by its margin to the height limit over it" data-testid="margin-legend">
      <div class="v3-ramp-row">
        <span class="v3-ramp-end">Too tall</span>
        <i class="v3-ramp" aria-hidden="true" />
        <span class="v3-ramp-end">{mm(MARGIN_EASY_MM)} or more to spare</span>
      </div>
      <div class="v3-legend-wrap">
        <span>
          <i class="v3-sw v3-sw-m-over" aria-hidden="true" />
          Over {bands.over}
        </span>
        <span>
          <i class="v3-sw v3-sw-m-tight" aria-hidden="true" />
          Under {mm(MARGIN_EASY_MM)} {bands.tight}
        </span>
        <span>
          <i class="v3-sw v3-sw-m-easy" aria-hidden="true" />
          {mm(MARGIN_EASY_MM)}+ {bands.easy}
        </span>
        <span>
          <i class="v3-sw v3-sw-m-none" aria-hidden="true" />
          No limit over it {bands.none}
        </span>
        {unknownKey}
        {limitKeys}
      </div>
      <div class="v3-legend-note" data-testid="margin-note">
        Green from {mm(MARGIN_EASY_MM)} to spare under the limit over a part: a threshold of the picture, not a check
      </div>
    </div>
  );
}
