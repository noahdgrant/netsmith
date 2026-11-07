import { type MockInstance, afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from './client';
import { autosave } from './source/prefs';
import { AUTOSAVE_MS, type OpenFile, edit, openFiles } from './store';

const file = (over: Partial<OpenFile> = {}): OpenFile => ({
  path: 'src/main.nsm',
  text: 'a',
  saved: 'a',
  stamp: 's1',
  readonly: false,
  conflict: false,
  rev: 0,
  ...over,
});

describe('autosave', () => {
  let write: MockInstance<typeof api.save>;
  beforeEach(() => {
    vi.useFakeTimers();
    write = vi.spyOn(api, 'save').mockResolvedValue({ stamp: 's2' } as never);
    vi.spyOn(api, 'analysis').mockRejectedValue(new Error('no server'));
    autosave.value = true;
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    autosave.value = false;
    openFiles.value = {};
  });

  it('writes a file once it has been left alone for the pause', async () => {
    openFiles.value = { 'src/main.nsm': file() };
    edit('src/main.nsm', 'ab');
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS - 1);
    expect(write).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(2);
    expect(write).toHaveBeenCalledWith('src/main.nsm', 'ab', 's1');
    expect(openFiles.value['src/main.nsm'].saved).toBe('ab');
  });

  it('waits again at every edit, and writes the last text once', async () => {
    openFiles.value = { 'src/main.nsm': file() };
    edit('src/main.nsm', 'ab');
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS - 100);
    edit('src/main.nsm', 'abc');
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS - 100);
    expect(write).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(200);
    expect(write).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith('src/main.nsm', 'abc', 's1');
  });

  it('does nothing when it is off', async () => {
    openFiles.value = { 'src/main.nsm': file() };
    autosave.value = false;
    edit('src/main.nsm', 'ab');
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS * 2);
    expect(write).not.toHaveBeenCalled();
  });

  it('writes the files that were waiting when it is turned on', async () => {
    openFiles.value = { 'src/main.nsm': file() };
    autosave.value = false;
    edit('src/main.nsm', 'ab');
    autosave.value = true;
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS + 1);
    expect(write).toHaveBeenCalledTimes(1);
  });

  it('does nothing for a read-only file or a file in conflict', async () => {
    openFiles.value = { 'std/x.nsm': file({ path: 'std/x.nsm', readonly: true }), 'src/c.nsm': file({ path: 'src/c.nsm', conflict: true }) };
    edit('std/x.nsm', 'ab');
    edit('src/c.nsm', 'ab');
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS * 2);
    expect(write).not.toHaveBeenCalled();
  });

  it('does not write a file whose edit was undone', async () => {
    openFiles.value = { 'src/main.nsm': file() };
    edit('src/main.nsm', 'ab');
    edit('src/main.nsm', 'a');
    await vi.advanceTimersByTimeAsync(AUTOSAVE_MS * 2);
    expect(write).not.toHaveBeenCalled();
  });
});
