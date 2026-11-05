// Pure state of the Edit Sheet form: the fields in the job unit, what is
// valid, and the material it sends to the kernel in millimetres.

import type { SheetMaterial } from '../../bridge/document';
import type { Units, XyOrigin, ZZero } from '../../bridge/types';
import { strings } from '../../lib/strings';
import { formatLength, parseLength, toMillimetres } from '../../lib/units';

export interface SheetEditorState {
  width: string;
  height: string;
  thickness: string;
  zZero: ZZero;
  xyOrigin: XyOrigin;
  offsetX: string;
  offsetY: string;
}

export type SheetEditorField = 'width' | 'height' | 'thickness' | 'offsetX' | 'offsetY';

export type SheetEditorErrors = Partial<Record<SheetEditorField, string>>;

const FIELDS: readonly SheetEditorField[] = ['width', 'height', 'thickness', 'offsetX', 'offsetY'];
const POSITIVE: readonly SheetEditorField[] = ['width', 'height', 'thickness'];

/** Fills the form from a sheet's material, shown in `units`. */
export function stateFromMaterial(material: SheetMaterial, units: Units): SheetEditorState {
  return {
    width: formatLength(material.width, units),
    height: formatLength(material.height, units),
    thickness: formatLength(material.thickness, units),
    zZero: material.zZero,
    xyOrigin: material.xyOrigin,
    offsetX: formatLength(material.offsetX, units),
    offsetY: formatLength(material.offsetY, units),
  };
}

/** Errors keyed by field; empty when the form can be applied. */
export function validate(state: SheetEditorState): SheetEditorErrors {
  const errors: SheetEditorErrors = {};
  for (const field of FIELDS) {
    const value = parseLength(state[field]);
    if (value === null) {
      errors[field] = strings.form.errorNumber;
    } else if (POSITIVE.includes(field) && value <= 0) {
      errors[field] = strings.form.errorPositive;
    }
  }
  return errors;
}

/** The material to send, in millimetres, or null while a field is wrong. */
export function materialFromState(state: SheetEditorState, units: Units): SheetMaterial | null {
  if (Object.keys(validate(state)).length > 0) {
    return null;
  }
  const mm = (field: SheetEditorField) => toMillimetres(parseLength(state[field]) ?? 0, units);
  return {
    width: mm('width'),
    height: mm('height'),
    thickness: mm('thickness'),
    zZero: state.zZero,
    xyOrigin: state.xyOrigin,
    offsetX: mm('offsetX'),
    offsetY: mm('offsetY'),
  };
}
