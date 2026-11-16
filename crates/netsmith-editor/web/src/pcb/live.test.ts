import { describe, expect, it } from 'vitest';
import type { PcbLive } from '../api';
import { placingLine, placingSvg, routingMarkup } from './live';

const placing: PcbLive = {
  board: 'B',
  task: 'place',
  region: { x: 0, y: 0, width: 20, height: 10 },
  parts: [
    { refdes: 'R1', box: { x: 1, y: 1, width: 3.4, height: 1.96 }, back: false },
    { refdes: 'U<1>', box: { x: 18, y: 8, width: 6, height: 6 }, back: true },
  ],
  tracks: [],
  vias: [],
};

const routing: PcbLive = {
  board: 'B',
  task: 'route',
  parts: [],
  tracks: [{ x1: 1, y1: 2, x2: 3.333, y2: 2, width: 0.25, layer: 'top' }],
  vias: [{ x: 3.333, y: 2, diameter: 0.6 }],
};

describe('the placer as it places', () => {
  it('draws the region and each part by face, in a box round them all', () => {
    const svg = placingSvg(placing);
    expect(svg).toContain('class="live-region" x="0" y="0" width="20" height="10"');
    expect(svg).toContain('class="live-part front" data-ref="R1"');
    expect(svg).toContain('class="live-part back" data-ref="U&lt;1&gt;"');
    // The part past the region's corner is in the view.
    const view = /viewBox="([^"]+)"/.exec(svg)![1].split(' ').map(Number);
    expect(view[0] + view[2]).toBeGreaterThanOrEqual(24);
    expect(view[1] + view[3]).toBeGreaterThanOrEqual(14);
  });

  it('draws nothing of a glimpse of the router, or of none', () => {
    expect(placingSvg(routing)).toBe('');
    expect(placingSvg(null)).toBe('');
  });

  it('says the last line the placer told, else what it places', () => {
    expect(placingLine('B: placing: run 1 [3 s]', placing)).toBe('B: placing: run 1 [3 s]');
    expect(placingLine(null, placing)).toBe('placing 2 parts');
    expect(placingLine(null, null)).toBe('placing the board');
  });
});

describe('the router as it routes', () => {
  it('draws its tracks and vias in the drawing\'s coordinates', () => {
    const g = routingMarkup(routing);
    expect(g).toContain('data-tracks="1"');
    expect(g).toContain('<line class="live-track" data-layer="top" x1="1" y1="2" x2="3.33" y2="2" stroke-width="0.25"/>');
    expect(g).toContain('<circle class="live-via" cx="3.33" cy="2" r="0.3"/>');
    expect(routingMarkup(placing)).toBe('');
  });
});
