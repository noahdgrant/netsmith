// The rule areas of the board (issue 1543), over the 2D view's copper: the server's list (`GET /api/pcb/rule-areas`, the
// `rule_areas` of `netsmith board rules`, what `netsmith view pcb` hatches): the footprints' (a library's keep-out, an RF module's
// antenna area) and the board's that are not keepouts of the physical file, which the scene draws as keepouts already. Each is
// hatched in `--pcb-keepout` with a dashed outline, as a keepout is, and its tip names what it forbids there, so a track drawn by
// hand learns of it before the design rules check says so. Their switch is the Layers menu's Rule areas. An area of the far side is
// dimmed as that side's copper is. The places are in the board file's frame, which is the sheet's, so they go into the drawing as
// they are.

import type { PcbRuleAreas, PcbScene, RuleArea } from '../api';
import { sideOf } from './pours';

/** Whether `areas` are of the placement `scene` shows, as `fillOf` says of the fill. */
export function ruleAreasOf(areas: PcbRuleAreas | null, scene: PcbScene | null): areas is PcbRuleAreas {
  return (
    !!areas &&
    !!scene &&
    areas.board === scene.board &&
    areas.layoutStamp === scene.layoutStamp &&
    areas.routesStamp === scene.routesStamp &&
    areas.routed === scene.routed
  );
}

const num = (v: number): string => String(Math.round(v * 1000) / 1000);

const escaped = (text: string): string =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** What an area is and what it forbids, as its tip says it, as `netsmith view pcb` prints it
 * (`antenna of U1 forbids tracks, vias on F.Cu, B.Cu`). */
export function ruleAreaSaid(area: RuleArea): string {
  const name = area.name || 'rule area';
  const of = area.part ? ` of ${area.part}` : ' of the board';
  const forbids = area.forbids.length ? area.forbids.join(', ') : 'nothing';
  return `${name}${of} forbids ${forbids} on ${area.layers.join(', ')}`;
}

/** The side an area is drawn with: `front` or `back` when it is on that outer layer alone, else `both`. */
function sideOfArea(area: RuleArea): 'front' | 'back' | 'both' {
  const sides = new Set(area.layers.map(sideOf));
  return sides.size === 1 && !sides.has('inner') ? (sides.has('front') ? 'front' : 'back') : 'both';
}

/** The areas as one group of the drawing (`l-rule-areas`), or an empty string when there are none. `current` says whether they are
 * of the placement drawn: those of an older one stay, marked `data-current="false"`, until the new ones come. */
export function ruleAreasMarkup(areas: PcbRuleAreas | null, current: boolean): string {
  if (!areas) return '';
  const drawn = areas.areas.flatMap((a) => {
    const d = [...a.outlines, ...a.holes]
      .filter((r) => r.length >= 3)
      .map((r) => r.map((p, k) => `${k === 0 ? 'M' : 'L'}${num(p.x)} ${num(p.y)}`).join('') + 'Z')
      .join('');
    if (d === '') return [];
    const part = a.part ? ` data-part="${escaped(a.part)}"` : '';
    return [
      `<path class="rule-area area-${sideOfArea(a)}" data-rule-area="${escaped(a.name)}"${part} data-layers="${escaped(a.layers.join(' '))}" data-forbids="${escaped(a.forbids.join(' '))}" d="${d}"><title>${escaped(ruleAreaSaid(a))}</title></path>`,
    ];
  });
  if (drawn.length === 0) return '';
  return `<g class="l-rule-areas" data-testid="rule-areas" data-count="${drawn.length}" data-current="${current}">${drawn.join('')}</g>`;
}
