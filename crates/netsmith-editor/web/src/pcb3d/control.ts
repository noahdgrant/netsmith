// What an agent asks of the 3D view over `/api/view` (P8a of issue 430, docs/BOARD3D.md), as plain
// words and numbers: a change read and checked against the scene the view shows, before anything
// is done, so a change the view cannot make changes nothing; and what the view shows, named as
// `netsmith render` names it. The names are render's: the views, `--section x=13`, the ids of
// `--highlight` and its error. Nothing here touches three.js or the page.

import type { ViewChange } from '../api';
import type { ColourBy } from './inspectMode';
import { PREVIEW_MAX_MM, PREVIEW_MIN_MM } from './inspectMode';
import { ViewRefusal } from './mode';
import { type Axis, type P3, RENDER_VIEWS, type RenderView } from './tools';

/** What the view has, to check a change's ids against: its parts, the nets with copper in it,
 * its height limits by name and its case models (ADR 0109) by name. In a system's scene (ADR
 * 0110) `system` is its name, a part's path and designator and a net are its member's
 * (`led/power_in`, `led/J1`, `led/GND`), and there is no case model: a case is one board's. */
export interface Ids {
  parts: { path: string; refdes: string }[];
  nets: string[];
  limits: string[];
  cases: string[];
  system?: string;
}

/** What `--highlight` (or `select`) named, sorted out: the parts by path, the nets, the limits,
 * the case models. */
export interface Named {
  parts: string[];
  nets: string[];
  limits: string[];
  cases: string[];
}

/** `x=13`, `y=8`, `z=-0.8` as `render --section` reads it; `none` is no cut. */
export function parseSection(text: string): { axis: Axis; at: number } | null {
  const t = text.trim();
  if (t === 'none') return null;
  const m = /^([xyz])\s*=\s*(.+)$/.exec(t);
  const at = m ? Number(m[2]) : NaN;
  if (!m || !Number.isFinite(at)) {
    throw new ViewRefusal(`section needs one axis and a position in mm of the scene's frame, such as x=12.5, y=8 or z=-0.8 (or none for no cut), not \`${text}\``);
  }
  return { axis: m[1] as Axis, at };
}

/** A cut as `render --section` writes it. */
export const sectionText = (cut: { axis: Axis; at: number } | null): string | null => (cut ? `${cut.axis}=${Number(cut.at.toFixed(3))}` : null);

/** The ids as `render --highlight` reads them: a part by designator or path, `net:<name>`,
 * `limit:<name>`, `case:<name>` (a case model); an item may hold several, comma-separated. An id the board does not have is an
 * error that names it and lists what the board has, as `render` words it. */
export function resolveIds(requested: string[], have: Ids, what = 'highlight'): Named {
  const out: Named = { parts: [], nets: [], limits: [], cases: [] };
  const unknown: string[] = [];
  const add = (list: string[], v: string) => list.includes(v) || list.push(v);
  for (const asked of requested.flatMap((r) => r.split(',')).map((r) => r.trim()).filter(Boolean)) {
    // A member's net as `render --board System` also writes it, `led/net:GND`, is `net:led/GND`.
    const memberNet = have.system ? /^([^/:]+)\/net:(.+)$/.exec(asked) : null;
    const want = memberNet ? `net:${memberNet[1]}/${memberNet[2]}` : asked;
    if (want.startsWith('net:')) {
      const net = want.slice(4);
      if (have.nets.includes(net)) add(out.nets, net);
      else unknown.push(asked);
    } else if (want.startsWith('limit:')) {
      const name = want.slice(6);
      if (have.limits.includes(name)) add(out.limits, name);
      else unknown.push(asked);
    } else if (want.startsWith('case:')) {
      // A case model is one board's (ADR 0109): a system's scene draws none, so there is nothing to light.
      if (have.system) {
        throw new ViewRefusal(
          `${what} names \`${asked}\`, a case model, and the system \`${have.system}\` draws none: a case model is one board's, so show that board (\`--board <System>/<member>\`) to light its case`,
        );
      }
      const name = want.slice(5);
      if (have.cases.includes(name)) add(out.cases, name);
      else unknown.push(asked);
    } else {
      const part = have.parts.find((p) => p.path === want) ?? have.parts.find((p) => p.refdes === want);
      if (part) add(out.parts, part.path);
      else unknown.push(asked);
    }
  }
  if (!unknown.length) return out;
  const list = (v: string[]) => (v.length ? v.join(', ') : '(none)');
  const owner = have.system ? `the system \`${have.system}\`` : 'the board';
  const hint = have.system ? `\n(in the system, a part and a net are a member's: such as ${have.parts[0]?.refdes ?? 'member/J1'} and net:${have.nets[0] ?? 'member/GND'})` : '';
  throw new ViewRefusal(
    `${what} names ${unknown.length === 1 ? 'an id' : 'ids'} that ${owner} does not have: ${unknown.map((u) => `\`${u}\``).join(', ')}\n` +
      `the parts are: ${list(have.parts.map((p) => `${p.refdes} (${p.path})`))}\n` +
      `the nets: ${list([...new Set(have.nets)].sort().map((n) => `net:${n}`))}\n` +
      `the limits: ${list([...new Set(have.limits)].map((l) => `limit:${l}`))}` +
      (have.system ? '' : `\nthe case models: ${list([...new Set(have.cases)].map((c) => `case:${c}`))}`) +
      hint,
  );
}

/** A selection of a system's scene (`led/power_in`, `led/GND`) as the page's selection holds it:
 * the member, and the parts and the net by its own names. The page's selection is one board's,
 * so ids of two members are refused. Null for none named. */
export function memberSelection(sel: { parts: string[]; net: string | null }, what: string): { member: string | null; parts: string[]; net: string | null } {
  const ids = [...sel.parts, ...(sel.net === null ? [] : [sel.net])];
  const members = [...new Set(ids.map((id) => id.slice(0, id.indexOf('/'))))];
  if (members.length > 1) {
    throw new ViewRefusal(`${what} names ids of ${members.length} members (${members.join(', ')}): the editor's selection is one board's (\`netsmith render --board <System>\` lights several)`);
  }
  const own = (id: string) => id.slice(id.indexOf('/') + 1);
  return { member: members[0] ?? null, parts: sel.parts.map(own), net: sel.net === null ? null : own(sel.net) };
}

/** What the page's selection can hold: parts, or one net (as a click picks them). */
function selectable(n: Named, what: string): { parts: string[]; net: string | null } {
  if (n.nets.length > 1) throw new ViewRefusal(`${what} names ${n.nets.length} nets: the editor lights one net at a time (\`netsmith render\` lights several)`);
  if (n.nets.length && n.parts.length) throw new ViewRefusal(`${what} names parts and a net: the editor's selection holds parts or one net, not both`);
  return { parts: n.parts, net: n.nets[0] ?? null };
}

/** A change, checked: what to do, in the view's terms. Absent means leave it as it is. */
export interface Plan {
  view?: RenderView;
  /** Where to look from, in the scene's frame: the view's direction. */
  towardEye?: P3;
  camera?: { position: P3; target: P3 };
  section?: { axis: Axis; at: number } | null;
  selection?: { parts: string[]; net: string | null };
  limits?: string[];
  /** The case models lit, by name. */
  cases?: string[];
  xray?: boolean;
  ortho?: boolean;
  colourBy?: ColourBy;
  preview?: { on: boolean; mm?: number };
  exploded?: boolean;
}

export { ViewRefusal };

const finite3 = (v: unknown): v is P3 => Array.isArray(v) && v.length === 3 && v.every((x) => typeof x === 'number' && Number.isFinite(x));

/** Check a change against what the view has, before any of it is done. */
export function planChange(change: ViewChange, have: Ids, canExplode: boolean): Plan {
  const plan: Plan = {};
  if (change.view !== undefined) {
    const dir = RENDER_VIEWS.find(([v]) => v === change.view)?.[1];
    if (!dir) throw new ViewRefusal(`view must be one of ${RENDER_VIEWS.map(([v]) => v).join(', ')}, not \`${change.view}\``);
    plan.view = change.view;
    plan.towardEye = dir;
  }
  if (change.camera !== undefined) {
    const { position, target } = change.camera;
    if (!finite3(position) || !finite3(target)) throw new ViewRefusal('camera needs a position and a target, each three numbers in mm of the scene\'s frame');
    if (Math.hypot(position[0] - target[0], position[1] - target[1], position[2] - target[2]) < 1e-6) throw new ViewRefusal('camera: the position and the target are the same point');
    plan.camera = { position, target };
  }
  if (change.section !== undefined) plan.section = parseSection(change.section);
  if (change.highlight !== undefined) {
    const n = resolveIds(change.highlight, have, 'highlight');
    plan.selection = selectable(n, 'highlight');
    plan.limits = n.limits;
    plan.cases = n.cases;
  }
  if (change.select !== undefined) {
    const n = resolveIds(change.select, have, 'select');
    if (n.limits.length) throw new ViewRefusal('select names a limit: a limit is not selected, it is lit (use highlight)');
    if (n.cases.length) throw new ViewRefusal('select names a case model: a case model is not selected, it is lit (use highlight)');
    plan.selection = selectable(n, 'select');
  }
  if (change.xray !== undefined) plan.xray = change.xray;
  if (change.ortho !== undefined) plan.ortho = change.ortho;
  if (change.perspective !== undefined) plan.ortho = !change.perspective;
  if (change.colourBy !== undefined) plan.colourBy = change.colourBy;
  const pv = change.heightPreview;
  if (typeof pv === 'boolean') plan.preview = { on: pv };
  else if (typeof pv === 'number') {
    if (!(pv >= PREVIEW_MIN_MM && pv <= PREVIEW_MAX_MM)) throw new ViewRefusal(`heightPreview is a height from ${PREVIEW_MIN_MM} to ${PREVIEW_MAX_MM} mm, or true or false, not ${pv}`);
    plan.preview = { on: true, mm: Math.round(pv * 100) / 100 };
  }
  if (change.exploded !== undefined) {
    if (change.exploded && !canExplode) throw new ViewRefusal('exploded: the board\'s scene has no stackup to pull apart', 409);
    plan.exploded = change.exploded;
  }
  return plan;
}

/** What is lit and picked, as `render --highlight` names it: the parts by designator, the net as
 * `net:<name>`, the limits as `limit:<name>`, the case models as `case:<name>`. */
export function litIds(
  selected: string[],
  net: string | null,
  limits: string[],
  parts: { path: string; refdes: string }[],
  cases: string[] = [],
): { highlight: string[]; selection: string[] } {
  const refs = selected.map((path) => parts.find((p) => p.path === path)?.refdes ?? path);
  const selection = net ? [`net:${net}`] : refs;
  return { selection, highlight: [...selection, ...[...new Set(limits)].map((l) => `limit:${l}`), ...[...new Set(cases)].map((c) => `case:${c}`)] };
}
