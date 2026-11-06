import { describe, expect, it } from 'vitest';

import { lineSeverity } from './LogPanel';
import { ago } from './ui';

describe('ago', () => {
  const t = 1_000_000;
  it('says just now for a few seconds', () => {
    expect(ago(t, t + 3_000)).toBe('just now');
  });
  it('counts seconds, minutes, hours and days', () => {
    expect(ago(t, t + 42_000)).toBe('42 s ago');
    expect(ago(t, t + 5 * 60_000)).toBe('5 min ago');
    expect(ago(t, t + 3 * 3_600_000)).toBe('3 h ago');
    expect(ago(t, t + 2 * 86_400_000)).toBe('2 d ago');
  });
  it('never says a time in the future', () => {
    expect(ago(t + 10_000, t)).toBe('just now');
  });
});

describe('the severity of a line of output', () => {
  it('reads an error, a warning, a note and a success', () => {
    expect(lineSeverity('error: no such board')).toBe('bad');
    expect(lineSeverity('design rules check: FAILED')).toBe('bad');
    expect(lineSeverity('warning: the pad is close to the edge')).toBe('warn');
    expect(lineSeverity('[constraint] J1 is 3 mm from the edge')).toBe('warn');
    expect(lineSeverity('note: 4 relations the solver left alone')).toBe('info');
    expect(lineSeverity('design rules check: passed')).toBe('ok');
    expect(lineSeverity('wrote out/TwoBoards/led')).toBe('ok');
  });
  it('leaves a line that says nothing in particular plain, and does not mind "0 errors"', () => {
    expect(lineSeverity('  J1   Header1x2   Conn_01x02')).toBe('');
    expect(lineSeverity('checked: 0 errors, 2 notes')).toBe('');
  });
});
