// Pure state for the Snap Settings form (F4): snapping toggles, grid
// spacing and snap radius, mirrored into the view store on apply.

import type { SnapOptions } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';

export interface SnapSettingsState {
  snap: SnapOptions;
  gridSpacing: string;
  snapRadiusPx: string;
  /** Drags move in round steps that follow the zoom. */
  snapDistances: boolean;
  /** Ctrl+Shift+arrow nudge, in the job unit. */
  nudgeDistance: string;
}

/** The snap radius range the preferences accept, in pixels. */
export const SNAP_RADIUS_MAX_PX = 50;

export type SnapSettingsField = 'gridSpacing' | 'snapRadiusPx' | 'nudgeDistance';
export type SnapSettingsErrors = Partial<Record<SnapSettingsField, string>>;

export interface SnapToggle {
  key: keyof SnapOptions;
  label: string;
}

export function snapSettingsFromView(
  snap: SnapOptions,
  gridSpacingMm: number,
  snapRadiusPx: number,
  units: Units,
  snapDistances = false,
  nudgeDistanceMm = 5,
): SnapSettingsState {
  return {
    snap: { ...snap },
    gridSpacing: lengthText(gridSpacingMm, units),
    snapRadiusPx: String(snapRadiusPx),
    snapDistances,
    nudgeDistance: lengthText(nudgeDistanceMm, units),
  };
}

export function validateSnapSettings(state: SnapSettingsState, units: Units): SnapSettingsErrors {
  const errors: SnapSettingsErrors = {};
  const grid = fieldError(lengthValue(state.gridSpacing, units), { positive: true });
  const radius = fieldError(numberValue(state.snapRadiusPx), { min: 1, max: SNAP_RADIUS_MAX_PX });
  const nudge = fieldError(lengthValue(state.nudgeDistance, units), { positive: true });
  if (nudge !== undefined) {
    errors.nudgeDistance = nudge;
  }
  if (grid !== undefined) {
    errors.gridSpacing = grid;
  }
  if (radius !== undefined) {
    errors.snapRadiusPx = radius;
  }
  return errors;
}

/** Values to push into the view store, or null while invalid. */
export function snapSettingsValues(
  state: SnapSettingsState,
  units: Units,
): {
  snap: SnapOptions;
  gridSpacingMm: number;
  snapRadiusPx: number;
  snapDistances: boolean;
  nudgeDistanceMm: number;
} | null {
  if (Object.keys(validateSnapSettings(state, units)).length > 0) {
    return null;
  }
  return {
    snap: { ...state.snap },
    gridSpacingMm: lengthValue(state.gridSpacing, units) ?? 10,
    snapRadiusPx: numberValue(state.snapRadiusPx) ?? 10,
    snapDistances: state.snapDistances,
    nudgeDistanceMm: lengthValue(state.nudgeDistance, units) ?? 5,
  };
}

export function toggleSnap(state: SnapSettingsState, key: keyof SnapOptions): SnapSettingsState {
  return { ...state, snap: { ...state.snap, [key]: !state.snap[key] } };
}
