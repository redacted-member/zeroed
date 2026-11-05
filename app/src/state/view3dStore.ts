// Requests from menus, keys and forms to the 3D view: standard views, zoom
// to fit, framing a box (a preview issue) and twiddle. The view follows `twiddle` and acts on each new `request`
// (the sequence number tells a repeat of the same request apart).

import { create } from 'zustand';

import type { ViewPreset } from '../views/View3D/camera3d';
import { SHADOW_DEPTH, stepShadowDepth } from '../lib/shadowDepth';

/** A box in job space, mm. */
export interface JobBox {
  min: [number, number, number];
  max: [number, number, number];
}

export type View3dRequest =
  | { kind: 'preset'; preset: ViewPreset; seq: number }
  | { kind: 'fit'; seq: number }
  | { kind: 'frame'; box: JobBox; surfaceZ: number; seq: number };

interface View3dState {
  /** The view turns slowly while this is on. */
  twiddle: boolean;
  setTwiddle: (on: boolean) => void;
  toggleTwiddle: () => void;
  request: View3dRequest | null;
  requestPreset: (preset: ViewPreset) => void;
  requestFit: () => void;
  /** Frames `box` from the current view angles; `surfaceZ` is the job Z
   * of the material top (the thickness when Z zero is at the bottom). */
  requestFrame: (box: JobBox, surfaceZ: number) => void;
  /** Depth of shadow shading for this session. */
  shadowDepth: number;
  stepShadow: (by: 1 | -1) => void;
}

export const useView3dStore = create<View3dState>()((set, get) => ({
  twiddle: false,
  setTwiddle: (twiddle) => set({ twiddle }),
  toggleTwiddle: () => set((state) => ({ twiddle: !state.twiddle })),
  request: null,
  requestPreset: (preset) =>
    set({ request: { kind: 'preset', preset, seq: (get().request?.seq ?? 0) + 1 } }),
  requestFit: () => set({ request: { kind: 'fit', seq: (get().request?.seq ?? 0) + 1 } }),
  requestFrame: (box, surfaceZ) =>
    set({ request: { kind: 'frame', box, surfaceZ, seq: (get().request?.seq ?? 0) + 1 } }),
  shadowDepth: SHADOW_DEPTH.initial,
  stepShadow: (by) => set((state) => ({ shadowDepth: stepShadowDepth(state.shadowDepth, by) })),
}));
