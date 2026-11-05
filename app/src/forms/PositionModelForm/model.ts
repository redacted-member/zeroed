// Pure helpers for the Position Model form: the zero plane as text in the
// job units, kept between the model's bottom and top.

import type { JobSetup, Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue } from '../../lib/formFields';

/** Slider positions across the model's height. */
export const PLANE_STEPS = 200;

export function planeText(millimetres: number, units: Units): string {
  return lengthText(millimetres, units);
}

export function planeError(text: string, units: Units, height: number): string | undefined {
  return fieldError(lengthValue(text, units), { min: 0, max: height });
}

/** The zero plane in mm, or null while the text is not a height in the model. */
export function planeValue(text: string, units: Units, height: number): number | null {
  if (planeError(text, units, height) !== undefined) {
    return null;
  }
  return lengthValue(text, units);
}

/** The zero plane at slider position `step` of [`PLANE_STEPS`]. */
export function planeAtStep(step: number, height: number): number {
  const clamped = Math.min(Math.max(step, 0), PLANE_STEPS);
  return (clamped / PLANE_STEPS) * height;
}

/** The slider position nearest a zero plane. */
export function stepOfPlane(plane: number, height: number): number {
  return height > 0 ? Math.round((plane / height) * PLANE_STEPS) : 0;
}

/** Create Both Sides only makes sense on a double sided job. */
export function canMakeBothSides(job: JobSetup): boolean {
  return job.kind === 'doubleSided';
}
