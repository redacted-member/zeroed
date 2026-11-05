// Explode steps model: the directions a step offers, a new step for the
// picked piece, and the list edits (pieces in and out, order), all pure.

import { pieceKey, type ExplodeStep, type PieceRef, type Triple } from '../../bridge/assembly';
import { assemblyFitStrings } from '../../lib/strings/assemblyFit';

const text = assemblyFitStrings.steps;

/** A direction a step can take: along an axis. */
export interface StepDirection {
  id: string;
  label: string;
  vector: Triple;
}

export const STEP_DIRECTIONS: readonly StepDirection[] = [
  { id: '+x', label: '+X', vector: [1, 0, 0] },
  { id: '-x', label: '-X', vector: [-1, 0, 0] },
  { id: '+y', label: '+Y', vector: [0, 1, 0] },
  { id: '-y', label: '-Y', vector: [0, -1, 0] },
  { id: '+z', label: '+Z', vector: [0, 0, 1] },
  { id: '-z', label: '-Z', vector: [0, 0, -1] },
];

/** The axis direction a vector runs along, or null for another way. */
export function directionId(vector: Triple): string | null {
  const found = STEP_DIRECTIONS.find((d) =>
    d.vector.every((v, i) => Math.abs(v - (vector[i] ?? 0)) < 1e-6),
  );
  return found === undefined ? null : found.id;
}

/** Distance a new step moves when nothing better is known, mm. */
export const DEFAULT_STEP_DISTANCE = 50;

/**
 * A new step for `piece`: out of its joints when Verify Fit has worked out
 * its explode offset, else straight up; named after its place in the list.
 */
export function newStep(
  steps: readonly ExplodeStep[],
  piece: PieceRef,
  offset: Triple | undefined,
): ExplodeStep {
  const length = offset === undefined ? 0 : Math.hypot(offset[0], offset[1], offset[2]);
  const along = offset !== undefined && length > 1e-6;
  return {
    id: 0,
    name: text.defaultName(steps.length + 1),
    pieces: [{ sheet: piece.sheet, part: piece.part, component: piece.component ?? null }],
    direction: along ? [offset[0] / length, offset[1] / length, offset[2] / length] : [0, 0, 1],
    distance: along ? Math.round(length * 1000) / 1000 : DEFAULT_STEP_DISTANCE,
  };
}

/** The steps with `piece` added to step `index` (once). */
export function withPiece(
  steps: readonly ExplodeStep[],
  index: number,
  piece: PieceRef,
): ExplodeStep[] {
  return steps.map((step, i) =>
    i !== index || step.pieces.some((p) => pieceKey(p) === pieceKey(piece))
      ? step
      : {
          ...step,
          pieces: [
            ...step.pieces,
            { sheet: piece.sheet, part: piece.part, component: piece.component ?? null },
          ],
        },
  );
}

/** The steps with `key` taken out of step `index`; a step left empty goes. */
export function withoutPiece(
  steps: readonly ExplodeStep[],
  index: number,
  key: string,
): ExplodeStep[] {
  return steps
    .map((step, i) =>
      i === index ? { ...step, pieces: step.pieces.filter((p) => pieceKey(p) !== key) } : step,
    )
    .filter((step) => step.pieces.length > 0);
}

/** The steps with step `index` moved `by` places (kept inside the list). */
export function moveStep(steps: readonly ExplodeStep[], index: number, by: number): ExplodeStep[] {
  const to = Math.min(steps.length - 1, Math.max(0, index + by));
  const out = [...steps];
  const [step] = out.splice(index, 1);
  if (step !== undefined) {
    out.splice(to, 0, step);
  }
  return out;
}
