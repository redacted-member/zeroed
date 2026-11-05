// Pure state for the Vector Selector: geometry and layer filters as the
// dialog edits them, and whether a toolpath picks its vectors automatically.

import { DEFAULT_FILTER, type VectorFilter } from '../../bridge/sketch';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue } from '../../lib/formFields';

export interface FilterState {
  open: boolean;
  closed: boolean;
  onlyCircles: boolean;
  /** Empty takes any diameter. */
  diameter: string;
  tolerance: string;
  allLayers: boolean;
  layers: string[];
  associate: boolean;
}

export type FilterErrors = Partial<Record<'diameter' | 'tolerance', string>>;

export function filterState(filter: VectorFilter | null, units: Units): FilterState {
  const f = filter ?? DEFAULT_FILTER;
  return {
    open: f.open,
    closed: f.closed,
    onlyCircles: f.onlyCircles,
    diameter: f.diameter === null ? '' : lengthText(f.diameter, units),
    tolerance: lengthText(f.tolerance, units),
    allLayers: f.layers.length === 0,
    layers: [...f.layers],
    associate: f.associate,
  };
}

export function validateFilter(state: FilterState, units: Units): FilterErrors {
  const errors: FilterErrors = {};
  if (state.onlyCircles && state.diameter.trim() !== '') {
    const diameter = fieldError(lengthValue(state.diameter, units), { positive: true });
    if (diameter !== undefined) {
      errors.diameter = diameter;
    }
  }
  const tolerance = fieldError(lengthValue(state.tolerance, units), { min: 0 });
  if (tolerance !== undefined) {
    errors.tolerance = tolerance;
  }
  return errors;
}

/** The kernel filter, or null while a field is invalid. */
export function filterFrom(state: FilterState, units: Units): VectorFilter | null {
  if (Object.keys(validateFilter(state, units)).length > 0) {
    return null;
  }
  return {
    open: state.open,
    closed: state.closed,
    onlyCircles: state.onlyCircles,
    diameter:
      state.onlyCircles && state.diameter.trim() !== '' ? lengthValue(state.diameter, units) : null,
    tolerance: lengthValue(state.tolerance, units) ?? 0,
    layers: state.allLayers ? [] : [...state.layers],
    associate: state.associate,
  };
}

/** Adds or removes a layer name from the layer filter. */
export function toggleLayer(state: FilterState, name: string, on: boolean): FilterState {
  const layers = on
    ? state.layers.includes(name)
      ? state.layers
      : [...state.layers, name]
    : state.layers.filter((l) => l !== name);
  return { ...state, layers, allLayers: false };
}

/** Whether a toolpath with this filter picks its vectors itself. */
export function isAutomatic(filter: VectorFilter | null | undefined): boolean {
  return filter !== null && filter !== undefined && filter.associate;
}

/** The params a form sends: with the filter when it is associated. */
export function withSelector<P extends object>(
  params: P,
  filter: VectorFilter | null,
): P & { selector?: VectorFilter } {
  return isAutomatic(filter) && filter !== null ? { ...params, selector: filter } : params;
}
