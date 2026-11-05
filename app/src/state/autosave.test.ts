import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '../bridge/api';
import { setKernelForTests } from '../bridge/client';
import { fakeKernel } from '../testing/fakeKernel';
import { autosaveState, backupNow, resetAutosave, startAutosave } from './autosave';
import { useDocumentStore } from './documentStore';
import { usePrefsStore } from './prefsStore';

describe('autosave', () => {
  beforeEach(() => {
    resetAutosave();
    useDocumentStore.setState({ kernelStatus: 'ready', dirty: true, revision: 5 });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('backs up a changed document once per revision', async () => {
    const saveCopy = vi.spyOn(api.doc, 'saveCopy').mockResolvedValue({ path: 'backup.carve' });
    expect(await backupNow()).toBe(true);
    expect(autosaveState().revision).toBe(5);
    expect(autosaveState().path).toBeTruthy();
    expect(await backupNow()).toBe(false);
    useDocumentStore.setState({ revision: 6 });
    expect(await backupNow()).toBe(true);
    expect(saveCopy).toHaveBeenCalledTimes(2);
  });

  it('skips a document without unsaved changes', async () => {
    useDocumentStore.setState({ dirty: false });
    expect(await backupNow()).toBe(false);
  });

  it('keeps the error when the backup fails', async () => {
    vi.spyOn(api.doc, 'saveCopy').mockRejectedValue(new Error('disk full'));
    expect(await backupNow()).toBe(false);
    expect(autosaveState().error).toBe('disk full');
  });

  it('follows the interval preference', async () => {
    vi.useFakeTimers();
    const saveCopy = vi.spyOn(api.doc, 'saveCopy').mockResolvedValue({ path: 'b' });
    const prefs = usePrefsStore.getState().prefs;
    usePrefsStore.setState({
      prefs: { ...prefs, general: { ...prefs.general, autosaveMinutes: 0 } },
    });
    const stop = startAutosave();
    await vi.advanceTimersByTimeAsync(10 * 60_000);
    expect(saveCopy).not.toHaveBeenCalled();
    usePrefsStore.setState({
      prefs: { ...prefs, general: { ...prefs.general, autosaveMinutes: 2 } },
    });
    await vi.advanceTimersByTimeAsync(2 * 60_000);
    expect(saveCopy).toHaveBeenCalledTimes(1);
    stop();
    useDocumentStore.setState({ revision: 9 });
    await vi.advanceTimersByTimeAsync(4 * 60_000);
    expect(saveCopy).toHaveBeenCalledTimes(1);
  });

  it('tells the shell which backup holds the unsaved changes, and when none does', async () => {
    const recorded: (string | null)[] = [];
    setKernelForTests({
      ...fakeKernel(() => ({})),
      recordBackup: (path) => {
        recorded.push(path);
        return Promise.resolve();
      },
    });
    try {
      vi.spyOn(api.doc, 'saveCopy').mockResolvedValue({ path: '/j/sign.backup.carve' });
      const stop = startAutosave();
      expect(await backupNow()).toBe(true);
      expect(recorded).toEqual(['/j/sign.backup.carve']);
      // Still unsaved: nothing changes.
      useDocumentStore.setState({ revision: 6 });
      expect(recorded).toEqual(['/j/sign.backup.carve']);
      // Saved: the backup holds nothing worth restoring.
      useDocumentStore.setState({ dirty: false });
      expect(recorded).toEqual(['/j/sign.backup.carve', null]);
      useDocumentStore.setState({ dirty: true });
      useDocumentStore.setState({ dirty: false });
      expect(recorded).toHaveLength(2);
      stop();
    } finally {
      setKernelForTests(null);
    }
  });
});
