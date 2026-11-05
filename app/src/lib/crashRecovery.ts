// After a crash: the shell tells the page, once at start up, that the last
// session ended without quitting, with the crash report it wrote and the
// autosave backup of a job that had unsaved changes. The page offers to show
// the report, then to restore the backup (Restore, Open Later, Discard).
// Hosts without the shell (the browser build) have nothing to offer.

import { getKernel } from '../bridge/client';
import { useDocumentStore } from '../state/documentStore';
import { useViewStore } from '../state/viewStore';
import { fileName } from './fileCommands';
import { strings } from './strings';

const BACKUP_ENDING = '.backup.carve';

/**
 * The job a backup was made from: `sign.backup.carve` beside `sign.carve`,
 * or null for `Untitled.backup.carve` in the Backups folder, a job that was
 * never saved.
 */
export function restoredPath(backup: string): string | null {
  if (!backup.endsWith(BACKUP_ENDING)) {
    return null;
  }
  const slash = backup.lastIndexOf('/');
  const folder = backup.slice(0, slash + 1);
  const name = backup.slice(slash + 1);
  if (name === `${strings.file.untitled}${BACKUP_ENDING}` && folder.endsWith('/Backups/')) {
    return null;
  }
  return `${backup.slice(0, -BACKUP_ENDING.length)}.carve`;
}

/**
 * Offers what a crashed session left. Answers true when a backup was
 * restored as the open job.
 */
export async function offerRecovery(): Promise<boolean> {
  const kernel = getKernel();
  const recovery = await kernel.sessionRecovery?.().catch(() => null);
  if (recovery === undefined || recovery === null || !recovery.crashed) {
    return false;
  }
  const text = strings.recovery;
  if (recovery.report !== null && (await kernel.confirm(text.showReport))) {
    await kernel.showCrashReport?.(recovery.report).catch(() => undefined);
  }
  for (const [index, backup] of recovery.backups.entries()) {
    const path = restoredPath(backup);
    const kept = recovery.kept[index] ?? '';
    const message = text.backupFound(fileName(path), backup, fileName(kept));
    const choice = await kernel.choose?.(message, {
      yes: text.restore,
      no: text.discard,
      cancel: text.openLater,
    });
    if (choice === 'yes') {
      const doc = useDocumentStore.getState();
      if (await doc.restoreDocument(backup, path)) {
        useViewStore.getState().zoomToJob(useDocumentStore.getState().job);
        await kernel.recordBackup?.(backup);
        return true;
      }
    } else if (choice === 'no') {
      await kernel.discardBackup?.(backup).catch(() => undefined);
    } else if (choice === 'cancel') {
      // The next autosave of a job would overwrite the backup under its
      // own name, so a backup kept for later moves out of its way.
      await kernel.keepBackup?.(backup).catch(() => undefined);
    }
  }
  return false;
}
