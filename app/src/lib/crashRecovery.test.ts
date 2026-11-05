import { afterEach, describe, expect, it, vi } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import type { KernelClient, SessionRecovery } from '../bridge/types';
import { useDocumentStore } from '../state/documentStore';
import { fakeKernel } from '../testing/fakeKernel';
import { offerRecovery, restoredPath } from './crashRecovery';

describe('restoredPath', () => {
  it('names the job a backup beside it was made from', () => {
    expect(restoredPath('/Users/me/Jobs/sign.backup.carve')).toBe('/Users/me/Jobs/sign.carve');
    expect(restoredPath('/Users/me/Jobs/Untitled.backup.carve')).toBe(
      '/Users/me/Jobs/Untitled.carve',
    );
  });

  it('gives no path to the backup of a job never saved', () => {
    expect(
      restoredPath(
        '/Users/me/Library/Application Support/CarveStudio/Backups/Untitled.backup.carve',
      ),
    ).toBeNull();
    expect(restoredPath('/Users/me/sign.carve')).toBeNull();
  });
});

interface Calls {
  kernel: string[];
  shell: string[];
  questions: string[];
}

function host(recovery: SessionRecovery | null, answers: { confirm: boolean; choose: string }) {
  const calls: Calls = { kernel: [], shell: [], questions: [] };
  const base = fakeKernel((name, args) => {
    calls.kernel.push(`${name} ${JSON.stringify(args)}`);
    if (name === 'doc.restore') return { revision: 9 };
    if (name === 'doc.info') {
      return {
        revision: 9,
        path: '/j/sign.carve',
        dirty: true,
        job: useDocumentStore.getState().job,
      };
    }
    return {};
  });
  const client: KernelClient = {
    ...base,
    confirm: (message) => {
      calls.shell.push(`confirm ${message}`);
      return Promise.resolve(answers.confirm);
    },
    sessionRecovery: () =>
      Promise.resolve(recovery ?? { crashed: false, report: null, backups: [], kept: [] }),
    showCrashReport: (path) => {
      calls.shell.push(`show ${path}`);
      return Promise.resolve();
    },
    choose: (message, buttons) => {
      calls.questions.push(message);
      calls.shell.push(`choose ${buttons.yes}/${buttons.no}/${buttons.cancel}`);
      return Promise.resolve(answers.choose as 'yes' | 'no' | 'cancel');
    },
    recordBackup: (path) => {
      calls.shell.push(`record ${path}`);
      return Promise.resolve();
    },
    discardBackup: (path) => {
      calls.shell.push(`discard ${path}`);
      return Promise.resolve();
    },
    keepBackup: (path) => {
      calls.shell.push(`keep ${path}`);
      return Promise.resolve('/j/sign recovered.carve');
    },
  };
  setKernelForTests(client);
  return calls;
}

describe('offerRecovery', () => {
  afterEach(() => {
    setKernelForTests(null);
    vi.restoreAllMocks();
  });

  const crashed: SessionRecovery = {
    crashed: true,
    report: '/r/CarveStudio 2026-09-28 00.20.31.txt',
    backups: ['/j/sign.backup.carve'],
    kept: ['/j/sign recovered.carve'],
  };

  it('offers nothing after a clean quit', async () => {
    const calls = host(null, { confirm: true, choose: 'yes' });
    expect(await offerRecovery()).toBe(false);
    expect(calls.shell).toEqual([]);
  });

  it('shows the report when asked and restores the backup as the unsaved job', async () => {
    const calls = host(crashed, { confirm: true, choose: 'yes' });
    expect(await offerRecovery()).toBe(true);
    expect(calls.shell).toEqual([
      expect.stringMatching(/^confirm CarveStudio quit unexpectedly/),
      'show /r/CarveStudio 2026-09-28 00.20.31.txt',
      'choose Restore/Discard/Open Later',
      'record /j/sign.backup.carve',
    ]);
    expect(calls.questions[0]).toContain('while sign.carve had changes');
    expect(calls.questions[0]).toContain('kept at /j/sign.backup.carve.');
    expect(calls.questions[0]).toContain(
      'in the same folder as sign recovered.carve for File > Open',
    );
    expect(calls.kernel[0]).toBe(
      'doc.restore {"path":"/j/sign.backup.carve","as":"/j/sign.carve"}',
    );
  });

  it('discards the backup, or keeps it for later, without touching the job', async () => {
    const discard = host({ ...crashed, report: null }, { confirm: false, choose: 'no' });
    expect(await offerRecovery()).toBe(false);
    expect(discard.shell).toEqual([
      'choose Restore/Discard/Open Later',
      'discard /j/sign.backup.carve',
    ]);
    expect(discard.kernel).toEqual([]);
    const later = host(crashed, { confirm: false, choose: 'cancel' });
    expect(await offerRecovery()).toBe(false);
    expect(later.shell).toEqual([
      expect.stringMatching(/^confirm /),
      'choose Restore/Discard/Open Later',
      'keep /j/sign.backup.carve',
    ]);
    expect(later.kernel).toEqual([]);
  });
});
