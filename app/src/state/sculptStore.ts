// Sculpting mode: while the Sculpt form is open, the 3D view turns left drags
// into brush strokes on the selected component with these settings.

import { create } from 'zustand';

import type { Point } from '../bridge/document';
import type { BrushParams, CustomBrushParams } from '../bridge/model';

/** A picture laid over the sculpted model: the bitmap selected when the
 * Sculpt form opened. */
export interface SculptOverlay {
  imageId: number;
  /** The bitmap's corners in job mm: lower left, lower right, upper
   * right, upper left. */
  quad: Point[];
  /** 0 to 1. */
  opacity: number;
  shown: boolean;
}

export const DEFAULT_BRUSH: BrushParams = {
  brush: 'deposit',
  diameter: 10,
  strength: 0.5,
  falloff: 0.5,
  grow: false,
};

/** A mouse wheel change of the brush made in the 3D view: `steps` notches
 * (up is positive) of the diameter, finer with `fine`, of the strength, or
 * (with R held) of the custom brush's angle. */
export interface BrushAdjust {
  kind: 'diameter' | 'strength' | 'angle';
  steps: number;
  fine: boolean;
  /** Counts up so the same change twice still reads as new. */
  seq: number;
}

interface SculptState {
  /** True while the Sculpt form is open. */
  active: boolean;
  /** The brush, or null while a form field is invalid. */
  brush: BrushParams | null;
  /** The custom brush, or null for the round brush. */
  custom: CustomBrushParams | null;
  setCustom: (custom: CustomBrushParams | null) => void;
  /** Show Visible Model: the 3D view shows the whole model while
   * sculpting instead of the sculpted component alone. */
  showVisible: boolean;
  /** The last wheel change for the form to take in. */
  adjust: BrushAdjust | null;
  setActive: (active: boolean) => void;
  setBrush: (brush: BrushParams | null) => void;
  setShowVisible: (on: boolean) => void;
  requestAdjust: (kind: BrushAdjust['kind'], steps: number, fine: boolean) => void;
  overlay: SculptOverlay | null;
  setOverlay: (overlay: SculptOverlay | null) => void;
}

export const useSculptStore = create<SculptState>()((set, get) => ({
  active: false,
  brush: DEFAULT_BRUSH,
  showVisible: false,
  adjust: null,
  custom: null,
  overlay: null,
  setOverlay: (overlay) => set({ overlay }),
  setCustom: (custom) => set({ custom }),
  setActive: (active) => set({ active }),
  setBrush: (brush) => set({ brush }),
  setShowVisible: (showVisible) => set({ showVisible }),
  requestAdjust: (kind, steps, fine) =>
    set({ adjust: { kind, steps, fine, seq: (get().adjust?.seq ?? 0) + 1 } }),
}));

/** The brush a stroke uses: with Shift the Undo Brush erases. */
export function strokeBrush(brush: BrushParams, shift: boolean): BrushParams {
  return shift && brush.brush === 'restore' ? { ...brush, brush: 'erase' } : brush;
}
