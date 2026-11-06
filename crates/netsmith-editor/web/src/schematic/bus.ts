// Busses in the schematic: what a click on one selects, where a bundle flag leads, and what the
// inspector says of one. A bus is an interface instance drawn as one thick line (ADR 0049); the
// drawing says its path, its type and its members (`readDrawing`). The two ends of a bus that
// leaves a module are two instances with names of their own (`uart0` on one sheet, `serial` on
// the other), so the other end is found by the nets, not by the name. Pure functions.

import type { NetInfo, PinInfo } from '../api';
import type { Bus } from './drawing';
import { type Target, netSheets } from './nav';

/** The instance's own name: `mcu.uart0` is `uart0`. */
export const busName = (path: string): string => path.slice(path.lastIndexOf('.') + 1);

/** `4 nets`. */
export const busCount = (bus: Pick<Bus, 'members'>): string => `${bus.members.length} ${bus.members.length === 1 ? 'net' : 'nets'}`;

/** What the top bar's chip says: `bus uart0` and `(4 nets)`. */
export function busChip(bus: Pick<Bus, 'path' | 'members'>): { title: string; sub: string } {
  return { title: `bus ${busName(bus.path)}`, sub: `(${busCount(bus)})` };
}

/** A sheet that carries a bus's nets, other than the one the bus is drawn on. */
export interface BusTarget extends Target {
  /** How many of the bus's member nets have a pin there. */
  carried: number;
  /** The nets of the bus that have a pin there, in the order of the members. */
  nets: string[];
  /** The bus drawn there for the same nets, when the sheet draws one: the one that shares the
   * most of them. */
  bus?: Bus;
}

/** The sheets, other than `bus`'s own, that carry the same member nets, in the order of `order`
 * (the strip's). The names of the two ends may differ, so the nets decide: a sheet carries the bus
 * when it has pins of at least two of its member nets (of all of them for a bus of one or two).
 * One shared line, a pull-up on one signal, is no other end. */
export function busTargets(
  bus: Bus,
  nets: Pick<NetInfo, 'name' | 'pins'>[],
  partSheet: Map<string, string>,
  order: string[],
  buses: Bus[],
): BusTarget[] {
  const byNet = new Map(nets.map((n) => [n.name, n]));
  const members = bus.nets.filter((n) => byNet.has(n));
  const need = Math.min(2, bus.nets.length);
  const per = new Map<string, { pins: PinInfo[]; nets: string[] }>();
  for (const net of members) {
    for (const [sheet, pins] of netSheets(byNet.get(net)!.pins, partSheet)) {
      if (sheet === bus.sheet) continue;
      const at = per.get(sheet) ?? { pins: [], nets: [] };
      at.pins.push(...pins);
      at.nets.push(net);
      per.set(sheet, at);
    }
  }
  const out: BusTarget[] = [];
  for (const sheet of order) {
    const at = per.get(sheet);
    if (!at || at.nets.length < need) continue;
    let best: Bus | undefined;
    let most = 0;
    for (const b of buses) {
      if (b.sheet !== sheet) continue;
      const shared = b.nets.filter((n) => bus.nets.includes(n)).length;
      if (shared > most) [best, most] = [b, shared];
    }
    out.push({ sheet, pins: at.pins, carried: at.nets.length, nets: at.nets, bus: best });
  }
  return out;
}

/** One member of a bus for the inspector: its name, its net, and the net's pins by module. */
export interface MemberRow {
  name: string;
  net: string;
  /** The pins on the net, module by module in the order of the strip. */
  modules: { sheet: string; pins: PinInfo[] }[];
}

export function busMembers(
  bus: Pick<Bus, 'members'>,
  nets: Pick<NetInfo, 'name' | 'pins'>[],
  partSheet: Map<string, string>,
  order: string[],
): MemberRow[] {
  const byNet = new Map(nets.map((n) => [n.name, n]));
  const natural = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });
  return bus.members.map((m) => {
    const bySheet = netSheets(byNet.get(m.net)?.pins ?? [], partSheet);
    const sheets = [...bySheet.keys()].sort((a, b) => {
      const ra = order.indexOf(a);
      const rb = order.indexOf(b);
      return (ra < 0 ? order.length : ra) - (rb < 0 ? order.length : rb);
    });
    return {
      name: m.name,
      net: m.net,
      modules: sheets.map((sheet) => ({
        sheet,
        pins: [...bySheet.get(sheet)!].sort((a, b) => natural(a.refdes, b.refdes) || natural(a.pin, b.pin)),
      })),
    };
  });
}

/** The nets that are lit with `focus`: a bus lights all its members. */
export const litNets = (bus: Pick<Bus, 'nets'> | undefined): Set<string> => new Set(bus?.nets ?? []);

/** The buses a set of lit nets runs through: a net that is selected lights the line it is on. */
export function busesOnNets(buses: Bus[], lit: Set<string>): Bus[] {
  return buses.filter((b) => b.nets.some((n) => lit.has(n)));
}
