// Pure state for the guide position form: one guide, its exact position
// in the display unit, and its locked flag.

import type { Guide } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue } from '../../lib/formFields';

export interface GuidePositionState {
  position: string;
  locked: boolean;
}

export function guideStateFrom(guide: Guide, units: Units): GuidePositionState {
  return { position: lengthText(guide.position, units), locked: guide.locked };
}

export function guidePositionError(state: GuidePositionState, units: Units): string | undefined {
  return fieldError(lengthValue(state.position, units));
}

/** Position in millimetres, or null while invalid. */
export function guidePositionMm(state: GuidePositionState, units: Units): number | null {
  return guidePositionError(state, units) === undefined ? lengthValue(state.position, units) : null;
}
