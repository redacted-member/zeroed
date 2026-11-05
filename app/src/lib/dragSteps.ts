// Modifier and step rules for dragging objects in the 2D view: Alt keeps a
// move horizontal or vertical, and Snap Distances rounds a move to a step
// that follows the zoom.

/** Fewest screen pixels one Snap Distances step spans. */
export const MIN_STEP_PX = 8;

/** The round step (1, 2 or 5 times a power of ten) of at least MIN_STEP_PX at this zoom. */
export function distanceStep(mmPerPx: number): number {
  const wanted = Math.max(mmPerPx * MIN_STEP_PX, 1e-6);
  const power = 10 ** Math.floor(Math.log10(wanted));
  for (const factor of [1, 2, 5, 10]) {
    if (factor * power >= wanted - 1e-12) {
      return factor * power;
    }
  }
  return 10 * power;
}

/** `value` rounded to a whole number of `step`s. */
export function roundToStep(value: number, step: number): number {
  return step > 0 ? Math.round(value / step) * step : value;
}

/** The move kept along its longer axis (Alt drag). */
export function axisLocked(dx: number, dy: number): { dx: number; dy: number } {
  return Math.abs(dx) >= Math.abs(dy) ? { dx, dy: 0 } : { dx: 0, dy };
}

export interface DragModifiers {
  /** Keep the move horizontal or vertical. */
  alt: boolean;
  /** Snap Distances is on and nothing snapped: round the move. */
  stepMm: number | null;
}

/** The move a drag makes from its raw delta and the modifiers in force. */
export function dragDelta(
  dx: number,
  dy: number,
  modifiers: DragModifiers,
): { dx: number; dy: number } {
  let move = { dx, dy };
  if (modifiers.stepMm !== null) {
    move = { dx: roundToStep(dx, modifiers.stepMm), dy: roundToStep(dy, modifiers.stepMm) };
  }
  return modifiers.alt ? axisLocked(move.dx, move.dy) : move;
}
