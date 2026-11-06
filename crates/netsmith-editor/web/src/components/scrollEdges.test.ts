import { describe, expect, it } from 'vitest';

import { edgesOf } from './scrollEdges';

const box = (o: Partial<Record<'scrollLeft' | 'scrollTop' | 'clientWidth' | 'clientHeight' | 'scrollWidth' | 'scrollHeight', number>>) => ({
  scrollLeft: 0,
  scrollTop: 0,
  clientWidth: 400,
  clientHeight: 300,
  scrollWidth: 400,
  scrollHeight: 300,
  ...o,
});

describe('edgesOf', () => {
  it('has no edge to fade when everything fits', () => {
    expect(edgesOf(box({}))).toEqual({ l: false, r: false, t: false, b: false });
  });
  it('fades the right edge of a table wider than its box, and the left once it has moved', () => {
    expect(edgesOf(box({ scrollWidth: 900 }))).toMatchObject({ l: false, r: true });
    expect(edgesOf(box({ scrollWidth: 900, scrollLeft: 200 }))).toMatchObject({ l: true, r: true });
    expect(edgesOf(box({ scrollWidth: 900, scrollLeft: 500 }))).toMatchObject({ l: true, r: false });
  });
  it('allows a pixel or two of rounding at the end', () => {
    expect(edgesOf(box({ scrollWidth: 401 }))).toMatchObject({ r: false });
    expect(edgesOf(box({ scrollHeight: 301 }))).toMatchObject({ b: false });
  });
  it('knows the vertical edges as well', () => {
    expect(edgesOf(box({ scrollHeight: 800, scrollTop: 100 }))).toMatchObject({ t: true, b: true });
  });
});
