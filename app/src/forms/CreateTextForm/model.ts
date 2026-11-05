// Pure state for the Create Text form: where the text starts and, for a
// text block, the column width it wraps to. The text settings themselves
// live in TextSpecFields.

import type { Path, TextAnchor, TextSpec } from '../../bridge/document';
import type { GeneralPrefs } from '../../bridge/prefs';
import type { JobSetup, Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { rectPath } from '../../lib/geometry';
import { jobBounds } from '../../views/View2D/camera';

export interface TextPositionState {
  x: string;
  y: string;
  /** Wrap the text to a column (a text block). */
  wrap: boolean;
  width: string;
  /** The point of the text's box at X and Y; null for text placed by its
   * first baseline (text made before anchors). */
  anchor: TextAnchor | null;
}

/** The anchor new text starts with: the last one used when the preference
 * asks for it, otherwise the bottom left corner. */
export function startAnchor(
  general: Pick<GeneralPrefs, 'lastTextAnchor' | 'textAnchor'>,
): TextAnchor {
  return general.lastTextAnchor ? general.textAnchor : 'bottomLeft';
}

export type TextPositionErrors = Partial<Record<'x' | 'y' | 'width', string>>;

/** A tenth of the way in from the job's left edge, halfway up; a block is
 * half the job wide. */
export function initialTextPosition(
  units: Units,
  job: JobSetup,
  wrap = false,
  anchor: TextAnchor = 'bottomLeft',
): TextPositionState {
  const box = jobBounds(job);
  return {
    x: lengthText(box.minX + (box.maxX - box.minX) * 0.1, units),
    y: lengthText((box.minY + box.maxY) / 2, units),
    wrap,
    width: lengthText((box.maxX - box.minX) / 2, units),
    anchor,
  };
}

export function convertTextPosition(
  state: TextPositionState,
  from: Units,
  to: Units,
): TextPositionState {
  return convertLengthFields(state, ['x', 'y', 'width'], from, to);
}

export function validateTextPosition(state: TextPositionState, units: Units): TextPositionErrors {
  const errors: TextPositionErrors = {};
  const x = fieldError(lengthValue(state.x, units));
  const y = fieldError(lengthValue(state.y, units));
  if (x !== undefined) {
    errors.x = x;
  }
  if (y !== undefined) {
    errors.y = y;
  }
  if (state.wrap) {
    const width = fieldError(lengthValue(state.width, units), { positive: true });
    if (width !== undefined) {
      errors.width = width;
    }
  }
  return errors;
}

/** The column width in millimetres for a block, undefined for plain text,
 * null while the width is invalid. */
export function wrapWidth(state: TextPositionState, units: Units): number | undefined | null {
  if (!state.wrap) {
    return undefined;
  }
  const width = lengthValue(state.width, units);
  return width === null || width <= 0 ? null : width;
}

/** The position in millimetres, or null while a field is invalid. */
export function textPosition(
  state: TextPositionState,
  units: Units,
): { x: number; y: number } | null {
  const x = lengthValue(state.x, units);
  const y = lengthValue(state.y, units);
  return x === null || y === null ? null : { x, y };
}

/** The fields moved to a clicked point, keeping the wrap settings. */
export function positionAt(
  state: TextPositionState,
  point: { x: number; y: number },
  units: Units,
): TextPositionState {
  return { ...state, x: lengthText(point.x, units), y: lengthText(point.y, units) };
}

/** Side of the anchor handle in pixels. */
export const HANDLE_SIZE_PX = 8;

/** The square handle drawn at the anchor of the text being edited, sized
 * in pixels at the given zoom. */
export function anchorHandle(point: { x: number; y: number }, pxPerMm: number): Path {
  const half = HANDLE_SIZE_PX / 2 / pxPerMm;
  return rectPath(point.x - half, point.y - half, half * 2, half * 2);
}

/** What the in-view handles set on a text (kerning, line gaps, arch); the
 * form's fields leave these alone. */
export type TextAdjustments = Pick<TextSpec, 'letterGaps' | 'lineGaps' | 'arch'>;

/** The adjustments a text spec carries. */
export function adjustmentsOf(spec: TextSpec): TextAdjustments {
  const kept: TextAdjustments = {};
  if (spec.letterGaps !== undefined) {
    kept.letterGaps = spec.letterGaps;
  }
  if (spec.lineGaps !== undefined) {
    kept.lineGaps = spec.lineGaps;
  }
  if (spec.arch !== undefined) {
    kept.arch = spec.arch;
  }
  return kept;
}

/** A spec from the form's fields with the text's adjustments put back. */
export function withAdjustments(spec: TextSpec, kept: TextAdjustments): TextSpec {
  const next: TextSpec = { ...spec };
  delete next.letterGaps;
  delete next.lineGaps;
  delete next.arch;
  return { ...next, ...adjustmentsOf({ ...next, ...kept }) };
}
