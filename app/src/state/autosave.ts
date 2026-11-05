// Autosave: every few minutes (the general.autosaveMinutes preference, 0 for
// never) a document with unsaved changes is written to a separate backup file
// by doc.saveCopy. The job file itself is only ever written by Save.

import { api } from '../bridge/api';
import { getKernel } from '../bridge/client';
import type { Unsubscribe } from '../bridge/types';
import { useDocumentStore } from './documentStore';
import { usePrefsStore } from './prefsStore';

interface AutosaveState {
  /** Revision of the document when it was last backed up. */
  revision: number | null;
  /** Where the last backup was written. */
  path: string | null;
  /** Why the last backup failed, null after a success. */
  error: string | null;
}

const state: AutosaveState = { revision: null, path: null, error: null };

/** What the last backup did, for the status bar and tests. */
export function autosaveState(): Readonly<AutosaveState> {
  return state;
}

/**
 * Writes a backup when the document has changes the last backup does not
 * have. Answers true when a backup was written.
 */
export async function backupNow(): Promise<boolean> {
  const { dirty, revision, kernelStatus } = useDocumentStore.getState();
  if (kernelStatus !== 'ready' || !dirty || revision === state.revision) {
    return false;
  }
  try {
    const reply = await api.doc.saveCopy();
    state.revision = revision;
    state.path = reply.path;
    state.error = null;
    // The shell offers this backup at the next start if the app crashes
    // before the job is saved.
    await getKernel().recordBackup?.(reply.path);
    return true;
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
    return false;
  }
}

/** Runs backups on the preference's interval until the returned call stops them. */
export function startAutosave(): Unsubscribe {
  let timer: ReturnType<typeof setInterval> | null = null;
  let minutes = -1;
  const arm = (next: number): void => {
    if (next === minutes) {
      return;
    }
    minutes = next;
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
    if (next > 0) {
      timer = setInterval(() => void backupNow(), next * 60_000);
    }
  };
  arm(usePrefsStore.getState().prefs.general.autosaveMinutes);
  const stop = usePrefsStore.subscribe((prefs) => arm(prefs.prefs.general.autosaveMinutes));
  // Once the job is saved (or replaced by a new or opened one) its backup
  // holds nothing worth restoring.
  const stopClean = useDocumentStore.subscribe((doc, before) => {
    if (!doc.dirty && before.dirty && state.path !== null) {
      state.path = null;
      state.revision = null;
      void getKernel().recordBackup?.(null);
    }
  });
  return () => {
    stop();
    stopClean();
    if (timer !== null) {
      clearInterval(timer);
    }
  };
}

/** Forgets the last backup, for a new or opened document and for tests. */
export function resetAutosave(): void {
  state.revision = null;
  state.path = null;
  state.error = null;
}
