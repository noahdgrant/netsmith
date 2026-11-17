import { describe, expect, it } from 'vitest';

import type { GerberFile } from '../api';
import { command, fileLabel, holesText, measure, measureText, svgPoint, toFrame, unreadWhere, viewLabel } from './gerber';

const file = (over: Partial<GerberFile>): GerberFile => ({
  file: 'x.gbr',
  format: 'gerber',
  function: 'copper',
  layer: 'top',
  identifiedBy: 'x2',
  fileFunction: null,
  read: true,
  extent: null,
  holes: null,
  slots: null,
  ...over,
});

describe('the Gerber view', () => {
  it('names the views by the reader’s words, and shows a word it does not know as it is', () => {
    expect(['top', 'inner2', 'bottom', 'all', 'top-paste'].map(viewLabel)).toEqual(['Top', 'Inner 2', 'Bottom', 'All', 'top-paste']);
  });

  it('names a file by its layer and function', () => {
    expect(fileLabel(file({}))).toBe('Top copper');
    expect(fileLabel(file({ layer: 'inner1' }))).toBe('Inner 1 copper');
    expect(fileLabel(file({ function: 'mask', layer: 'bottom' }))).toBe('Bottom mask openings');
    expect(fileLabel(file({ function: 'drill', layer: null }))).toBe('Drill');
    expect(fileLabel(file({ function: 'other', layer: null }))).toBe('Not known');
  });

  it('turns a point of the mirrored bottom back into the frame', () => {
    const region = [-1, -1, 22, 12];
    expect(toFrame({ x: 3, y: 4 }, region, false)).toEqual({ x: 3, y: 4 });
    // The region's left edge is its right edge turned over.
    expect(toFrame({ x: -1, y: 4 }, region, true)).toEqual({ x: 21, y: 4 });
    expect(toFrame({ x: 10, y: 4 }, region, true)).toEqual({ x: 10, y: 4 });
  });

  it('finds the point under the pointer in a box of any size', () => {
    expect(svgPoint(50, 25, 100, 50, [-1, -1, 22, 12])).toEqual({ x: 10, y: 5 });
  });

  it('measures a distance in mm', () => {
    const m = measure({ x: 0, y: 0 }, { x: 3, y: -4 });
    expect(m).toEqual({ dx: 3, dy: -4, d: 5 });
    expect(measureText(m)).toBe('5.000 mm (Δx 3.000, Δy -4.000)');
  });

  it('says the holes of a size and where a note is', () => {
    expect(holesText({ diameter: 0.3, plated: true, holes: 12, slots: 1 })).toBe('0.300 mm plated: 12 holes, 1 slot');
    expect(holesText({ diameter: 3.2, plated: false, holes: 1, slots: 0 })).toBe('3.200 mm not plated: 1 hole');
    expect(unreadWhere({ file: 'a.gbr', line: 12, message: 'x' })).toBe('a.gbr, line 12');
    expect(unreadWhere({ file: 'a.gbr', line: null, message: 'x' })).toBe('a.gbr');
  });

  it('gives the command that draws the same view', () => {
    expect(command('fab/led/led-gerbers.zip', 'bottom')).toBe('netsmith view gerber fab/led/led-gerbers.zip --layer bottom');
    expect(command('my fab/x.zip', 'top')).toBe("netsmith view gerber 'my fab/x.zip' --layer top");
  });
});
