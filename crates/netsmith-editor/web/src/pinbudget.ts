// The pin budget of a host processor (issue 1284) as the board summary lists it: a line for each module with the pins and
// serial blocks it uses of the host, and the serial blocks by bus with how many are left. The numbers are the server's
// (`PinBudget`, the same record `netsmith board explain <host>` prints); this only words them.

import type { PinBudget } from './api';

export interface BudgetModuleRow {
  /** The module's path, or `(board)` for the parts the board declares itself. */
  module: string;
  pins: string;
  /** The pins by the datasheet's names, for a title. */
  pinNames: string;
  blocks: string;
}

export interface BudgetBusRow {
  bus: string;
  text: string;
  /** Whether no block of the bus is left. */
  full: boolean;
}

export function budgetModules(b: PinBudget): BudgetModuleRow[] {
  return b.modules.map((m) => ({
    module: m.module === '' ? '(board)' : m.module,
    pins: m.pins.length === 1 ? '1 pin' : `${m.pins.length} pins`,
    pinNames: m.pins.join(', '),
    blocks: m.blocks.join(', '),
  }));
}

/** The serial blocks grouped by the bus they are an end of (`other` for a peripheral that names none), in name order. */
export function budgetBuses(b: PinBudget): BudgetBusRow[] {
  const by = new Map<string, { all: number; used: number; left: string[] }>();
  for (const s of b.blocks) {
    const bus = s.bus ?? 'other';
    const e = by.get(bus) ?? { all: 0, used: 0, left: [] };
    e.all += 1;
    if (s.used) e.used += 1;
    if (s.left) e.left.push(s.name);
    by.set(bus, e);
  }
  return [...by.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([bus, e]) => ({
      bus,
      text: `${e.used} of ${e.all} used, ${e.left.length} left${e.left.length > 0 ? ` (${e.left.join(', ')})` : ''}`,
      full: e.left.length === 0,
    }));
}
