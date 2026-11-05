// One shot hand-off of a file chosen in File > Open (or dropped on the
// window) to the form that imports it. The form reads the path in its state
// initializer and clears it in an effect, which survives StrictMode double
// calls.

import { create } from 'zustand';

/** Which import form a pending file is for. */
export type PendingFileKind = 'mesh';

interface PendingFile {
  kind: PendingFileKind;
  path: string;
}

interface PendingFileState {
  pending: PendingFile | null;
  offer: (kind: PendingFileKind, path: string) => void;
  clear: () => void;
}

export const usePendingFileStore = create<PendingFileState>()((set) => ({
  pending: null,
  offer: (kind, path) => set({ pending: { kind, path } }),
  clear: () => set({ pending: null }),
}));

/** The file waiting for a form of `kind`, without taking it. */
export function pendingPath(kind: PendingFileKind): string | null {
  const pending = usePendingFileStore.getState().pending;
  return pending !== null && pending.kind === kind ? pending.path : null;
}
