// The Assembly View's Fit, Explode, Section and Measure while its form is
// open: the target clearance and tolerance, the joints the last Verify Fit
// found (on the assembled places), the joint picked in the list, Show Fit
// (every joint colored at once), how far the pieces are exploded, the
// section plane and a measurement between two picked points (all view
// states, no undo step). Moving a piece, undo and toolpath edits make the
// joints and the measurement stale; the explode then follows the new
// places.

import { create } from 'zustand';

import { api } from '../bridge/formApi';
import {
  pieceKey,
  pieceRef,
  type PieceFace,
  type PieceRef,
  type PieceRow,
  type Triple,
} from '../bridge/assembly';
import type { FitJoint, MeasureReply, SectionAxis } from '../bridge/assemblyFit';
import type { Units } from '../bridge/types';
import { MM_PER_INCH } from '../lib/units';
import { isShown, useAssemblyStore } from './assemblyStore';
import { useDocumentStore } from './documentStore';

/** The section plane: across which job axis, where (null: the middle) and which side stays. */
export interface SectionState {
  on: boolean;
  axis: SectionAxis;
  /** Job coordinate along the axis, mm; null for the middle of the pieces. */
  offset: number | null;
  /** Keep the side above the offset instead of below. */
  flip: boolean;
}

/** A measurement: which point it waits for, the points picked and what they measure. */
export interface MeasureState {
  step: 'first' | 'second' | null;
  a: PieceFace | null;
  b: PieceFace | null;
  result: MeasureReply | null;
  busy: boolean;
}

export const NO_SECTION: SectionState = { on: false, axis: 'x', offset: null, flip: false };
export const NO_MEASURE: MeasureState = { step: null, a: null, b: null, result: null, busy: false };

interface FitState {
  /** Gap wanted each side of a fit, mm; null until set (the job unit's default). */
  target: number | null;
  /** How far a gap may stray from the target, mm; null until set. */
  tolerance: number | null;
  joints: FitJoint[] | null;
  /** Keys of the pieces measured on exact edges. */
  exact: readonly string[];
  /** Each piece's move at a full explode, by piece key. */
  offsets: Readonly<Record<string, Triple>>;
  verifying: boolean;
  error: string | null;
  /** The joint picked in the list, an index into `joints`. */
  picked: number | null;
  showFit: boolean;
  /** Verify Fit checks the hidden pieces too (as the whole assembly is checked by default). */
  includeHidden: boolean;
  /** 0 assembled to 1 fully exploded. */
  explode: number;
  /** The places the joints and offsets were found for. */
  measured: string | null;
  section: SectionState;
  measure: MeasureState;

  setTarget: (mm: number) => void;
  setTolerance: (mm: number) => void;
  /** Finds the joints between the pieces as they are assembled. */
  verify: () => Promise<void>;
  pick: (index: number | null) => void;
  setShowFit: (show: boolean) => void;
  setIncludeHidden: (include: boolean) => void;
  setExplode: (fraction: number) => void;
  setSection: (change: Partial<SectionState>) => void;
  /** Waits for two points picked on pieces. */
  startMeasure: () => void;
  cancelMeasure: () => void;
  /** A point picked in the view while a measurement waits for one. */
  measurePick: (face: PieceFace) => Promise<void>;
  reset: () => void;
}

/** The target clearance and tolerance a job starts with, mm. */
export function defaultFit(units: Units): { target: number; tolerance: number } {
  return units === 'inch'
    ? { target: 0.008 * MM_PER_INCH, tolerance: 0.004 * MM_PER_INCH }
    : { target: 0.2, tolerance: 0.1 };
}

/** The target and tolerance in use: the ones set, else the job unit's. */
export function fitTarget(
  state: Pick<FitState, 'target' | 'tolerance'>,
  units: Units,
): { target: number; tolerance: number } {
  const start = defaultFit(units);
  return { target: state.target ?? start.target, tolerance: state.tolerance ?? start.tolerance };
}

/** What the fit depends on in the pieces: which they are and where they sit. */
export function placesKey(pieces: readonly PieceRow[]): string {
  return pieces
    .map((p) => [pieceKey(p), ...p.position, ...p.rotation, ...p.size].join(':'))
    .join('|');
}

/** The pieces Verify Fit leaves out: none when it checks the hidden ones, else those not shown. */
export function leftOut(
  pieces: readonly PieceRow[],
  isolated: string | null,
  includeHidden: boolean,
): PieceRef[] {
  return includeHidden ? [] : pieces.filter((p) => !isShown(p, isolated)).map(pieceRef);
}

/** A fraction held to 0..1 (0 for anything not a number). */
export function clampFraction(value: number): number {
  return Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const EMPTY: Pick<FitState, 'joints' | 'exact' | 'offsets' | 'picked' | 'measured'> = {
  joints: null,
  exact: [],
  offsets: {},
  picked: null,
  measured: null,
};

export const useAssemblyFitStore = create<FitState>((set, get) => {
  let tickets = 0;
  return {
    target: null,
    tolerance: null,
    ...EMPTY,
    verifying: false,
    error: null,
    showFit: false,
    includeHidden: true,
    explode: 0,
    section: NO_SECTION,
    measure: NO_MEASURE,

    setTarget: (mm) => set({ target: mm, joints: null, picked: null }),
    setTolerance: (mm) => set({ tolerance: mm, joints: null, picked: null }),

    verify: async () => {
      const ticket = ++tickets;
      const { target, tolerance } = fitTarget(get(), useDocumentStore.getState().job.units);
      const { pieces, isolated } = useAssemblyStore.getState();
      const places = placesKey(pieces);
      const leaveOut = leftOut(pieces, isolated, get().includeHidden);
      set({ verifying: true, error: null });
      try {
        const reply = await api.assembly.verifyFit(target, tolerance, leaveOut);
        if (ticket !== tickets) {
          return;
        }
        const offsets: Record<string, Triple> = {};
        for (const row of reply.explode) {
          offsets[pieceKey(row.piece)] = row.offset;
        }
        set({
          joints: reply.joints,
          exact: reply.exact.map(pieceKey),
          offsets,
          picked: null,
          measured: places,
        });
      } catch (error) {
        if (ticket === tickets) {
          set({ error: message(error) });
        }
      } finally {
        if (ticket === tickets) {
          set({ verifying: false });
        }
      }
    },

    pick: (index) => set({ picked: index }),
    setShowFit: (show) => set({ showFit: show }),
    setIncludeHidden: (include) => set({ includeHidden: include, joints: null, picked: null }),

    setExplode: (fraction) => {
      const explode = clampFraction(fraction);
      set({ explode });
      const { measured, verifying } = get();
      // Named explode steps move the pieces without Verify Fit's offsets.
      const stepped = useAssemblyStore.getState().explodeSteps.length > 0;
      if (explode > 0 && measured === null && !verifying && !stepped) {
        void get().verify();
      }
    },

    setSection: (change) => set({ section: { ...get().section, ...change } }),

    startMeasure: () => set({ measure: { ...NO_MEASURE, step: 'first' } }),
    cancelMeasure: () => set({ measure: { ...get().measure, step: null } }),

    measurePick: async (face) => {
      const now = get().measure;
      if (now.step === 'first') {
        set({ measure: { ...NO_MEASURE, step: 'second', a: face } });
        return;
      }
      if (now.step !== 'second' || now.a === null) {
        return;
      }
      const a = now.a;
      set({ measure: { ...now, step: null, b: face, busy: true }, error: null });
      try {
        const result = await api.assembly.measure(
          { ...pieceRef(a), point: a.point },
          { ...pieceRef(face), point: face.point },
        );
        set({ measure: { ...get().measure, result, busy: false } });
      } catch (error) {
        set({ measure: { ...get().measure, busy: false }, error: message(error) });
      }
    },

    reset: () => {
      tickets += 1;
      set({
        ...EMPTY,
        verifying: false,
        error: null,
        explode: 0,
        section: NO_SECTION,
        measure: NO_MEASURE,
      });
    },
  };
});

// Closing the Assembly View puts the pieces back together; moving a piece,
// undo, redo and toolpath edits make the joints and the explode stale.
useAssemblyStore.subscribe((state, previous) => {
  const fit = useAssemblyFitStore.getState();
  if (!state.active && previous.active) {
    fit.reset();
    return;
  }
  if (state.pieces === previous.pieces) {
    return;
  }
  const places = placesKey(state.pieces);
  if (places === placesKey(previous.pieces)) {
    return;
  }
  // A measurement's points were on the pieces where they stood.
  if (fit.measure.result !== null || fit.measure.b !== null) {
    useAssemblyFitStore.setState({ measure: NO_MEASURE });
  }
  if (fit.measured !== null && places !== fit.measured) {
    useAssemblyFitStore.setState({ ...EMPTY });
    if (fit.explode > 0) {
      void useAssemblyFitStore.getState().verify();
    }
  }
});
