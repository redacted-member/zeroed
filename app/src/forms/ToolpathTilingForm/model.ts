// Pure state for the Toolpath Tiling form: how the job is divided into
// tiles that fit the machine.

import type { Tiling, TilingMethod } from '../../bridge/cam';
import type { Units, XyOrigin } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

export interface TilingFormState {
  enabled: boolean;
  method: TilingMethod;
  tileWidth: string;
  tileHeight: string;
  overlap: string;
  smallestFirst: boolean;
}

export type TilingField = 'tileWidth' | 'tileHeight' | 'overlap';
export type TilingErrors = Partial<Record<TilingField, string>>;

const LENGTHS: ReadonlyArray<TilingField> = ['tileWidth', 'tileHeight', 'overlap'];

/**
 * The form's fields from the document's tiling, or defaults before it is
 * read. Tile Toolpaths shows whether the document tiles its toolpaths, so a
 * job that has never been tiled opens with it off.
 */
export function initialTilingForm(tiling: Tiling | null, units: Units): TilingFormState {
  const spec = tiling?.spec;
  const fallback = units === 'inch' ? 609.6 : 600;
  return {
    enabled: tiling?.enabled ?? false,
    method: spec?.method ?? 'individual',
    tileWidth: lengthText(
      spec !== undefined && spec.tileWidth > 0 ? spec.tileWidth : fallback,
      units,
    ),
    tileHeight: lengthText(
      spec !== undefined && spec.tileHeight > 0 ? spec.tileHeight : fallback,
      units,
    ),
    overlap: lengthText(spec?.overlap ?? 0, units),
    smallestFirst: spec?.smallestFirst ?? false,
  };
}

/** The tiling Delete All Tiling Data leaves: a new document's, off. */
export function clearedTiling(): Tiling {
  return {
    enabled: false,
    spec: {
      method: 'individual',
      tileWidth: 600,
      tileHeight: 600,
      overlap: 0,
      smallestFirst: false,
    },
    active: 0,
    originalPosition: false,
  };
}

export function convertTilingForm(state: TilingFormState, from: Units, to: Units): TilingFormState {
  return convertLengthFields(state, LENGTHS, from, to);
}

/** The size fields the layout uses: feeding through uses only one. */
export function usedFields(method: TilingMethod): ReadonlyArray<TilingField> {
  switch (method) {
    case 'feedX':
      return ['tileWidth', 'overlap'];
    case 'feedY':
      return ['tileHeight', 'overlap'];
    default:
      return LENGTHS;
  }
}

export function validateTilingForm(state: TilingFormState, units: Units): TilingErrors {
  const errors: TilingErrors = {};
  for (const field of usedFields(state.method)) {
    const value = lengthValue(state[field], units);
    const error = fieldError(value, field === 'overlap' ? { min: 0 } : { positive: true });
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  const overlap = lengthValue(state.overlap, units);
  if (errors.overlap === undefined && overlap !== null) {
    for (const field of usedFields(state.method)) {
      const size = lengthValue(state[field], units);
      if (field !== 'overlap' && size !== null && overlap >= size) {
        errors.overlap = strings.toolpaths.tiling.overlapTooBig;
      }
    }
  }
  return errors;
}

/** True when tiling is asked for on a job whose XY datum is not at the
 * bottom left corner, which is not allowed. */
export function datumBlocksTiling(enabled: boolean, origin: XyOrigin): boolean {
  return enabled && origin !== 'bottomLeft';
}

/**
 * The tiling Update Tiles applies, keeping the view choices of `current`;
 * null while a field is invalid. A size the layout ignores is sent as 0.
 */
export function tilingFromForm(
  state: TilingFormState,
  units: Units,
  current: Tiling | null,
): Tiling | null {
  if (Object.keys(validateTilingForm(state, units)).length > 0) {
    return null;
  }
  const used = usedFields(state.method);
  const size = (field: TilingField) =>
    used.includes(field) ? (lengthValue(state[field], units) ?? 0) : 0;
  return {
    enabled: state.enabled,
    spec: {
      method: state.method,
      tileWidth: size('tileWidth'),
      tileHeight: size('tileHeight'),
      overlap: size('overlap'),
      smallestFirst: state.smallestFirst,
    },
    active: current?.active ?? 0,
    originalPosition: current?.originalPosition ?? false,
  };
}
