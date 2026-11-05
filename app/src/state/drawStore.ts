// Link between an open creation form and the drawing view. The form owns
// the parameters; the view forwards pointer and key events to the form's
// controller and draws whatever preview the form publishes.

import { create } from 'zustand';

import type { Path, Point } from '../bridge/document';
import type { QuickEntry } from '../lib/quickKeys';

export interface DrawPointer {
  /** Job millimetres after snapping. */
  x: number;
  y: number;
  /** Job millimetres before snapping. */
  rawX: number;
  rawY: number;
  shift: boolean;
  ctrl: boolean;
  /** Alt (Option): draw a box shape out from its centre. */
  alt?: boolean;
}

/** How a form's overlay path is drawn: in the first or second rail colour,
 * or the selection colour. */
export type OverlayTone = 'first' | 'second' | 'accent';

/** A path a form draws over the drawing view in a colour of its own. */
export interface PreviewOverlay {
  path: Path;
  tone: OverlayTone;
  /** Dashed instead of solid. */
  dashed?: boolean;
  /** Filled (markers) instead of stroked. */
  fill?: boolean;
}

/** A short text a form writes into the drawing view at a job point. */
export interface PreviewLabel {
  at: Point;
  text: string;
  tone: OverlayTone;
}

/** An entry of the right click menu a controller offers. */
export interface ControllerMenuItem {
  id: string;
  label: string;
  checked?: boolean;
  disabled?: boolean;
}

/** What a creation form exposes to the drawing view while it is open. */
export interface DrawController {
  cursor?: string;
  pointerDown(p: DrawPointer): void;
  pointerMove(p: DrawPointer): void;
  pointerUp(p: DrawPointer): void;
  doubleClick?(p: DrawPointer): void;
  /** Returns true when the key was consumed. */
  keyDown?(event: KeyboardEvent): boolean;
  /**
   * Applies values typed as Quick Keys; `cursor` is the pointer in job
   * millimetres. Returns false when the entry does not apply right now
   * (for example no drag in progress).
   */
  quickEntry?(entry: QuickEntry, cursor: Point): boolean;
  /** Right click at a job point: the menu to show there, or null for the
   * drawing view's own menu. */
  contextMenu?(at: Point): ControllerMenuItem[] | null;
  /** An entry of that menu was chosen. */
  menuAction?(id: string): void;
  /** A right click without a drag; true when the tool used it (Extend
   * Vectors starts again), so no menu opens and the form stays. */
  rightClick?(at: Point): boolean;
  cancel(): void;
}

interface DrawState {
  controller: DrawController | null;
  /** Paths drawn as a dashed preview while the form is open. */
  preview: Path[];
  /** Coloured paths drawn over the preview while the form is open. */
  overlays: PreviewOverlay[];
  setOverlays: (overlays: PreviewOverlay[]) => void;
  /** Texts drawn over the view while the form is open (Nest Parts copies). */
  labels: PreviewLabel[];
  setLabels: (labels: PreviewLabel[]) => void;
  /** Quick Keys typed so far in the drawing view, shown in the status bar. */
  quickText: string;
  setController: (controller: DrawController | null) => void;
  setPreview: (preview: Path[]) => void;
  setQuickText: (quickText: string) => void;
}

export const useDrawStore = create<DrawState>()((set) => ({
  controller: null,
  preview: [],
  overlays: [],
  setOverlays: (overlays) => set({ overlays }),
  labels: [],
  setLabels: (labels) => set({ labels }),
  quickText: '',
  setController: (controller) => set({ controller, quickText: '' }),
  setPreview: (preview) => set({ preview }),
  setQuickText: (quickText) => set({ quickText }),
}));
