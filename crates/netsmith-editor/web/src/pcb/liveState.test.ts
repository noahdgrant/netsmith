import { beforeEach, describe, expect, it } from 'vitest';
import { dropOutside, livePlace, liveRoute, liveSeen, noteOutside, notePcbLive, outsideRuns } from './liveState';

const live = (task: string) => ({ board: 'B', task, parts: [], tracks: [], vias: [] });

describe('what the page is shown while the placer and the router run', () => {
  it('keeps the last glimpse of each task and counts them', () => {
    const seen = liveSeen.peek();
    notePcbLive(live('place'));
    notePcbLive(live('route'));
    notePcbLive(live('route'));
    notePcbLive(live('other'));
    expect(livePlace.peek()?.task).toBe('place');
    expect(liveRoute.peek()?.task).toBe('route');
    expect(liveSeen.peek()).toEqual({ place: seen.place + 1, route: seen.route + 2 });
  });
});

describe('runs the editor did not start', () => {
  beforeEach(() => (outsideRuns.value = {}));
  const line = (run: string, message: string, more = {}) => ({ run, command: 'fab', board: '', message, ended: false, ...more });

  it('gathers the lines of a run until it ends, and drops an ended run when another begins', () => {
    noteOutside(line('fab 1', 'Led: placing 12 parts', { board: 'Led' }), 1000);
    noteOutside(line('fab 1', 'Led: routing: try 1 of 1 [2 s]'), 2000);
    let r = outsideRuns.peek()['fab 1'];
    expect(r.lines).toEqual(['Led: placing 12 parts', 'Led: routing: try 1 of 1 [2 s]']);
    expect([r.board, r.started, r.ended]).toEqual(['Led', 1000, undefined]);
    noteOutside(line('fab 1', 'fab: done', { ended: true, ok: true }), 3000);
    r = outsideRuns.peek()['fab 1'];
    expect(r.ended).toEqual({ ok: true, at: 3000 });
    noteOutside(line('board3d 2', 'Led: placing: run 1 [1 s]'), 4000);
    expect(Object.keys(outsideRuns.peek())).toEqual(['board3d 2']);
    dropOutside('board3d 2');
    expect(outsideRuns.peek()).toEqual({});
  });
});
