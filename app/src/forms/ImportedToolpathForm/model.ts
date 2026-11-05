// Pure state of the imported toolpath form: where the toolpath sits (a
// move from where its file put it), how much deeper it cuts, its feeds and
// the tool the preview uses. Lengths and feeds follow the display unit.

import type { ImportedEdit, ImportedToolpath, Tool } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface ImportedFormState {
  name: string;
  offsetX: string;
  offsetY: string;
  startDepth: string;
  /** Per minute, in the display unit. */
  feed: string;
  plunge: string;
  /** rpm. */
  spindle: string;
  tool: Tool | null;
}

export type ImportedField = 'offsetX' | 'offsetY' | 'startDepth' | 'feed' | 'plunge' | 'spindle';
export type ImportedErrors = Partial<Record<ImportedField | 'name', string>>;

const LENGTHS = ['offsetX', 'offsetY', 'startDepth', 'feed', 'plunge'] as const;

export function importedState(toolpath: ImportedToolpath, units: Units): ImportedFormState {
  return {
    name: toolpath.name,
    offsetX: lengthText(toolpath.offsetX, units),
    offsetY: lengthText(toolpath.offsetY, units),
    startDepth: lengthText(toolpath.startDepth, units),
    feed: lengthText(toolpath.feed, units),
    plunge: lengthText(toolpath.plunge, units),
    spindle: String(toolpath.spindle),
    tool: toolpath.tool,
  };
}

export function convertImportedUnits(
  state: ImportedFormState,
  from: Units,
  to: Units,
): ImportedFormState {
  return convertLengthFields(state, [...LENGTHS], from, to);
}

export function validateImported(state: ImportedFormState, units: Units): ImportedErrors {
  const errors: ImportedErrors = {};
  const set = (field: keyof ImportedErrors, error: string | undefined) => {
    if (error !== undefined) {
      errors[field] = error;
    }
  };
  if (state.name.trim() === '') {
    set('name', strings.toolpaths.imported.nameRequired);
  }
  set('offsetX', fieldError(lengthValue(state.offsetX, units)));
  set('offsetY', fieldError(lengthValue(state.offsetY, units)));
  set('startDepth', fieldError(lengthValue(state.startDepth, units)));
  set('feed', fieldError(lengthValue(state.feed, units), { positive: true }));
  set('plunge', fieldError(lengthValue(state.plunge, units), { positive: true }));
  set('spindle', fieldError(numberValue(state.spindle), { positive: true }));
  return errors;
}

/** The kernel edit for the form, or null while a field is invalid. */
export function importedEdit(
  id: number,
  state: ImportedFormState,
  units: Units,
): ImportedEdit | null {
  if (Object.keys(validateImported(state, units)).length > 0) {
    return null;
  }
  const mm = (text: string) => lengthValue(text, units) ?? 0;
  const edit: ImportedEdit = {
    id,
    name: state.name.trim(),
    offsetX: mm(state.offsetX),
    offsetY: mm(state.offsetY),
    startDepth: mm(state.startDepth),
    feed: mm(state.feed),
    plunge: mm(state.plunge),
    spindle: numberValue(state.spindle) ?? 0,
  };
  if (state.tool !== null) {
    edit.tool = state.tool;
  }
  return edit;
}
