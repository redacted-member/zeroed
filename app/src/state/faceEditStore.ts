// A furniture panel's face opened from the Assembly View for editing in
// the 2D or 3D view (Back to Assembly returns), and the panel the pointer
// is over for the status line. No kernel calls here: the start bundle
// reads it.

import { create } from 'zustand';

/** The panel face being edited away from the Assembly View. */
export interface FaceEditing {
  design: number;
  panel: string;
  /** Such as "Door 1, front face". */
  label: string;
  /** Back where the face was opened from (the Furniture Designer), else the Assembly View. */
  back?: () => void;
}

/** Leaves the face: back where it was opened from, else `fallback` (the Assembly View). */
export function leaveFace(fallback: () => void): void {
  const editing = useFaceEditStore.getState().editing;
  useFaceEditStore.getState().setEditing(null);
  if (editing?.back !== undefined) {
    editing.back();
  } else {
    fallback();
  }
}

interface FaceEditState {
  editing: FaceEditing | null;
  /** The furniture panel and face under the pointer, as the status line says it. */
  pointerPanel: string | null;
  setEditing: (editing: FaceEditing | null) => void;
  setPointerPanel: (label: string | null) => void;
}

export const useFaceEditStore = create<FaceEditState>()((set, get) => ({
  editing: null,
  pointerPanel: null,
  setEditing: (editing) => set({ editing }),
  setPointerPanel: (pointerPanel) => {
    if (get().pointerPanel !== pointerPanel) {
      set({ pointerPanel });
    }
  },
}));
