// What the drawing view shows in place of a placed bitmap while Trace Bitmap
// works on it: the picture in its reduced colours, with the colours being
// traced shown in the trace colour.

import { create } from 'zustand';

export interface BitmapOverride {
  imageId: number;
  image: ImageBitmap;
}

interface TraceViewState {
  override: BitmapOverride | null;
  setOverride: (override: BitmapOverride | null) => void;
}

export const useTraceViewStore = create<TraceViewState>()((set) => ({
  override: null,
  setOverride: (override) => set({ override }),
}));
