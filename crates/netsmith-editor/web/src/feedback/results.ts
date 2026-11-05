// What a toast says when a long action ends: the result in a line, the cause when it failed, and
// what the person can do next. Pure; `Build.tsx` and the router give it the server's report and
// turn `next` into buttons.

import type { BuildReport, Download, FabReport, OrderReport, PcbReport } from '../api';
import { plural, seconds } from '../util';
import { causeOf } from './status';

export type Next =
  | { label: string; go: 'build' | 'pcb' }
  | { label: string; download: Download; board: string }
  | { label: string; orderDownload: Download; order: string };

export interface Result {
  kind: 'ok' | 'bad' | 'info';
  title: string;
  detail?: string;
  next: Next[];
}

const took = (ms: number) => `took ${seconds(ms)}`;

export function buildResult(r: BuildReport): Result {
  if (r.ok) {
    const n = r.written.length;
    return {
      kind: 'ok',
      title: `Wrote KiCad files${n ? ` for ${plural(n, 'board')}` : ''}`,
      detail: `${took(r.elapsedMs)}; in the project's out folder`,
      next: [{ label: 'Open Build', go: 'build' }],
    };
  }
  return {
    kind: 'bad',
    title: 'Writing the KiCad files did not finish',
    detail: causeOf(r.output) || undefined,
    next: [{ label: 'Show output', go: 'build' }],
  };
}

export function fabResult(r: FabReport): Result {
  if (r.ok) {
    const gerbers = r.downloads.find((d) => d.key === 'gerbers');
    const check = r.drc === 'passed' ? 'design rules check passed' : r.drc === 'failed' ? 'design rules check FAILED' : 'design rules check not run';
    return {
      kind: 'ok',
      title: `Fab files made for ${r.board}`,
      detail: `${took(r.elapsedMs)}; ${check}`,
      next: [
        ...(gerbers ? [{ label: 'Download Gerbers', download: gerbers, board: r.board } as Next] : []),
        { label: 'Open Build', go: 'build' },
      ],
    };
  }
  const why = r.drc === 'failed' ? 'The design rules check found errors' : causeOf(r.output);
  return {
    kind: 'bad',
    title: `Fab stopped for ${r.board}`,
    detail: why || undefined,
    next: [{ label: 'Show output', go: 'build' }],
  };
}

export function orderResult(r: OrderReport): Result {
  if (r.ok) {
    const summary = r.downloads.find((d) => d.key === 'summary');
    return {
      kind: 'ok',
      title: `Order ${r.name} made`,
      detail: `${took(r.elapsedMs)}; written to ${r.dir}`,
      next: [
        ...(summary ? [{ label: 'Download order.txt', orderDownload: summary, order: r.name } as Next] : []),
        { label: 'Open Build', go: 'build' },
      ],
    };
  }
  return {
    kind: 'bad',
    title: `Order ${r.name} stopped`,
    detail: causeOf(r.output) || undefined,
    next: [{ label: 'Show output', go: 'build' }],
  };
}

export function routeResult(r: PcbReport): Result {
  if (r.error) {
    return { kind: 'bad', title: `Could not place ${r.board}`, detail: causeOf(r.error) || undefined, next: [] };
  }
  const left = r.unrouted.length;
  const how = `${plural(r.tracks, 'track')}, ${plural(r.vias, 'via')}; ${took(r.elapsedMs)}`;
  if (r.stopped) {
    return {
      kind: 'info',
      title: `Routing of ${r.board} stopped`,
      detail: `${left ? `${plural(left, 'connection')} left. ` : ''}${how}`,
      next: [{ label: 'Open PCB', go: 'pcb' }],
    };
  }
  return {
    kind: left ? 'info' : 'ok',
    title: left ? `Routed ${r.board}: ${plural(left, 'connection')} left` : `Routed ${r.board}: every connection made`,
    detail: how,
    next: [{ label: 'Open PCB', go: 'pcb' }],
  };
}
