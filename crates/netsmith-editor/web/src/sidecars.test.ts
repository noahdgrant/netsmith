import { describe, expect, it } from 'vitest';

import type { PickLockInfo, PinRequestInfo } from './api';
import { cannotMove, lockView, padlockTitle, pinFileOf, pinRows, targetsOf, withoutPin } from './sidecars';

const req = (over: Partial<PinRequestInfo>): PinRequestInfo => ({ request: 'status', function: 'GPIO', locked: false, offers: [], ...over });
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

describe('the pins a request may be moved to', () => {
  const offers = [{ pin: 'p00', pad: '3' }, { pin: 'p01', pad: '4', heldBy: 'vbat' }, { pin: 'p02' }];
  const r = req({ pin: 'p00', pad: '3', offers });
  it('marks the pin it has, a pin another request holds and a free pin', () => {
    expect(targetsOf(r).map((t) => [t.pin, t.state])).toEqual([['p00', 'current'], ['p01', 'held'], ['p02', 'free']]);
  });
  it('names each pin for a screen reader and says why a held pin is refused', () => {
    const t = targetsOf(r);
    expect(t[0].label).toBe('p00, pad 3, where it is now');
    expect(t[1].label).toBe('p01, pad 4, held by vbat');
    expect(t[1].title).toContain('held by `vbat`');
    expect(t[2]).toMatchObject({ label: 'p02', pad: '', title: 'Move `status` to p02' });
  });
  it('is carried by the rows', () => {
    expect(pinRows([r])[0].targets).toHaveLength(3);
  });
  it('says what a padlock does in plain words, with the file it writes', () => {
    expect(padlockTitle({ request: 'status', locked: false }, 'src/main.pins.toml')).toBe('Unlocked: a run may move this pin to make the board route. Click to lock it (writes `lock = true` in src/main.pins.toml).');
    expect(padlockTitle({ request: 'status', locked: true }, 'src/main.pins.toml')).toContain('Click to unlock it (removes `lock = true` from src/main.pins.toml)');
  });
  it('names the pin file from the source file', () => {
    expect(pinFileOf('src/main.nsm')).toBe('src/main.pins.toml');
  });
  it('lets a request move unless it is locked, has no pin or has nowhere to go', () => {
    const [row] = pinRows([r]);
    expect(cannotMove(row)).toBeNull();
    expect(cannotMove({ ...row, locked: true })).toBe('Unlock `status` to move it.');
    expect(cannotMove({ ...row, message: 'no pin' })).toBe('This request has no pin yet.');
    expect(cannotMove({ ...row, targets: row.targets.slice(0, 1) })).toBe('No other pin offers this function.');
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
