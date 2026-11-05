// Pure state for the Add Draft to Model form: the draft angle, set with the
// slider or typed.

import { fieldError, numberValue } from '../../lib/formFields';

/** Slider range in degrees; a typed angle may be anything above 0 and below 90. */
export const MIN_SLIDER_ANGLE = 1;
export const MAX_SLIDER_ANGLE = 60;

export interface AddDraftState {
  /** Degrees from vertical, as typed. */
  angle: string;
}

export type AddDraftErrors = Partial<Record<'angle', string>>;

export function initialAddDraftState(): AddDraftState {
  return { angle: '10' };
}

export function validateAddDraft(state: AddDraftState): AddDraftErrors {
  const angle = numberValue(state.angle);
  const error = fieldError(angle, { positive: true, max: 89.9 });
  return error === undefined ? {} : { angle: error };
}

/** The angle to send, or null when it is not valid. */
export function draftAngle(state: AddDraftState): number | null {
  const angle = numberValue(state.angle);
  return angle === null || validateAddDraft(state).angle !== undefined ? null : angle;
}

/** Where the slider sits for the typed angle. */
export function sliderAngle(state: AddDraftState): number {
  const angle = numberValue(state.angle) ?? MIN_SLIDER_ANGLE;
  return Math.min(MAX_SLIDER_ANGLE, Math.max(MIN_SLIDER_ANGLE, angle));
}
