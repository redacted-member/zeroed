// Edit > Notes: the job notes, and showing them when a job opens whose
// notes start with a full stop (the convention).

import { api } from '../../bridge/api';
import { useDialogStore } from '../../state/dialogStore';

/** True for notes that ask to be shown when the job opens. */
export function showsOnOpen(notes: string): boolean {
  return notes.trimStart().startsWith('.');
}

/** Opens the Notes dialog if the open job's notes ask for it. */
export async function showNotesIfFlagged(): Promise<void> {
  try {
    const { notes } = await api.doc.getNotes();
    if (showsOnOpen(notes)) {
      useDialogStore.getState().show('notes');
    }
  } catch {
    // An older kernel without notes shows nothing.
  }
}
