import { describe, expect, it } from 'vitest';

import type { PickLockInfo, PinRequestInfo } from './api';
import { lockView, pinRows, withoutPin } from './sidecars';

const req = (over: Partial<PinRequestInfo>): PinRequestInfo => ({ request: 'status', function: 'GPIO', locked: false, ...over });
const lock = (over: Partial<PickLockInfo>): PickLockInfo => ({ file: 'src/main.lock.toml', exists: true, current: true, diff: [], reported: '', ...over });

describe('the pin map rows', () => {
  it('says the pin, the pad, the function and where the entry came from', () => {
    const [row] = pinRows([req({ pin: 'p00', pad: '3', entry: 'hand', from: 'file', locked: true })]);
    expect(row).toMatchObject({ request: 'status', pin: 'p00', pad: 'pad 3', function: 'GPIO', locked: true, origin: 'file, hand', message: '' });
  });
  it('says a pin picked now has no entry in the file yet', () => {
    const [row] = pinRows([req({ pin: 'p02', pad: '5', entry: 'auto', from: 'run' })]);
    expect(row.origin).toBe('picked now');
    expect(row.originTitle).toContain('no entry for it yet');
    expect(row.locked).toBe(false);
  });
  it('leaves the pad out when the design has none', () => {
    expect(pinRows([req({ pin: 'p00', from: 'run' })])[0].pad).toBe('');
  });
  it('carries the check\'s message for a request that has no pin', () => {
    const rows = pinRows([req({ request: 'spare', function: 'GPIOO', message: '`spare` asks for GPIOO' }), req({ pin: 'p00', from: 'run' })]);
    expect(rows[0]).toMatchObject({ pin: '', origin: '', message: '`spare` asks for GPIOO' });
    expect(withoutPin(rows)).toBe(1);
  });
  it('is empty for a part no request asks', () => {
    expect(pinRows(undefined)).toEqual([]);
  });
});

describe('the pick lock view', () => {
  it('is current when the file holds the rows the solver picks', () => {
    expect(lockView(lock({}))).toMatchObject({ state: 'current', tone: 'ok', lines: [] });
  });
  it('lists the diff of a lock that is not current', () => {
    const v = lockView(lock({ current: false, diff: ['B: `u`: `A` -> `B`'] }));
    expect(v).toMatchObject({ state: 'not current', tone: 'warn', lines: ['B: `u`: `A` -> `B`'] });
    expect(v.says).toContain('lock --update');
  });
  it('says a lock that does not exist is written by build', () => {
    const v = lockView(lock({ exists: false, current: false }));
    expect(v).toMatchObject({ state: 'absent', tone: 'warn', lines: [] });
    expect(v.says).toContain('netsmith build');
  });
  it('needs no file for a source with nothing to lock', () => {
    expect(lockView(lock({ exists: false, current: true })).state).toBe('none needed');
  });
  it('shows the reason a lock cannot be read, and no diff', () => {
    expect(lockView(lock({ current: false, error: 'version 9 is not a version' }))).toMatchObject({ state: 'unreadable', tone: 'bad', says: 'version 9 is not a version', lines: [] });
  });
});
