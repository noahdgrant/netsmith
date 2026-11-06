import { describe, expect, it } from 'vitest';

import { planeLabel, planeTone } from './planes';

describe('planeTone', () => {
  it('is the layer the drawing names, not the place among the planes', () => {
    // two planes, the first drawn is on In2: the guess by place would say the first tone
    expect(planeTone('In2.Cu', 0)).toBe(2);
    expect(planeTone('In1.Cu', 1)).toBe(1);
    expect(planeTone('In3.Cu', 0)).toBe(3);
  });
  it('gives every layer from the fourth inward the last tone', () => {
    expect(planeTone('In4.Cu', 0)).toBe(4);
    expect(planeTone('In12.Cu', 0)).toBe(4);
  });
  it('falls back to the place for a drawing with no layer, and for an outer layer', () => {
    expect(planeTone(undefined, 0)).toBe(1);
    expect(planeTone(null, 2)).toBe(3);
    expect(planeTone('', 1)).toBe(2);
    expect(planeTone('B.Cu', 1)).toBe(2);
    expect(planeTone(undefined, 9)).toBe(4);
  });
});

describe('planeLabel', () => {
  it('names the net and the layer', () => {
    expect(planeLabel('GND', 'In1.Cu')).toBe('GND · In1.Cu');
  });
  it('is the net alone when the drawing has no layer', () => {
    expect(planeLabel('GND', undefined)).toBe('GND');
    expect(planeLabel('3V3', '')).toBe('3V3');
  });
});
