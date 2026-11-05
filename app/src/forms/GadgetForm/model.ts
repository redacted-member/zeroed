// Form state for a gadget dialog built from the kernel's field list. Numbers
// and lengths are text (lengths in the display unit), checks are booleans,
// choices and text are strings. Values go to the kernel in millimetres.

import type { GadgetField, GadgetValue } from '../../bridge/gadgets';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';
import { roundForUnits, toDisplay } from '../../lib/units';

export type GadgetFormState = Record<string, string | boolean>;

export type GadgetErrors = Record<string, string | undefined>;

/** `given` when it is a value `field` can hold, else the field's default. */
function startingValue(field: GadgetField, given: GadgetValue | undefined): GadgetValue {
  if (given === undefined || typeof given !== typeof field.default) {
    return field.default;
  }
  if (field.kind === 'choice' && !(field.options ?? []).some((o) => o.value === given)) {
    return field.default;
  }
  return given;
}

/** The form state holding every field's value from `values` (a toolpath
 * being edited, or the gadget's last run; lengths in millimetres), or its
 * default where `values` has none that fits. */
export function initialGadgetState(
  fields: readonly GadgetField[],
  units: Units,
  values?: Readonly<Record<string, GadgetValue>>,
): GadgetFormState {
  const state: GadgetFormState = {};
  for (const field of fields) {
    const value = startingValue(field, values?.[field.id]);
    switch (field.kind) {
      case 'length':
        state[field.id] = lengthText(typeof value === 'number' ? value : 0, units);
        break;
      case 'number':
        state[field.id] = String(typeof value === 'number' ? value : 0);
        break;
      case 'check':
        state[field.id] = value === true;
        break;
      case 'choice':
      case 'text':
        state[field.id] = typeof value === 'string' ? value : '';
        break;
      case 'tool':
        // A tool id as text; empty until one is chosen.
        state[field.id] = typeof value === 'number' && value > 0 ? String(value) : '';
        break;
    }
  }
  return state;
}

/** Converts the length fields when the display unit changes. */
export function convertGadgetUnits(
  fields: readonly GadgetField[],
  state: GadgetFormState,
  from: Units,
  to: Units,
): GadgetFormState {
  if (from === to) {
    return state;
  }
  const next = { ...state };
  for (const field of fields) {
    const text = next[field.id];
    if (field.kind !== 'length' || typeof text !== 'string') {
      continue;
    }
    const millimetres = lengthValue(text, from);
    if (millimetres !== null) {
      next[field.id] = lengthText(millimetres, to);
    }
  }
  return next;
}

/** A limit in millimetres as the display unit shows it. */
function displayLimit(limit: number | undefined, units: Units): number | undefined {
  return limit === undefined ? undefined : roundForUnits(toDisplay(limit, units), units);
}

/** True while the check field a field depends on is not ticked. */
export function fieldDisabled(field: GadgetField, state: GadgetFormState): boolean {
  return field.enabledBy !== undefined && state[field.enabledBy] !== true;
}

/** A run of neighbouring fields that share a group heading. */
export interface GadgetSection {
  /** The heading; undefined for fields without a group. */
  group: string | undefined;
  fields: GadgetField[];
}

/** The fields split into sections, in order, one per run of fields that
 * share a group. */
export function gadgetSections(fields: readonly GadgetField[]): GadgetSection[] {
  const sections: GadgetSection[] = [];
  for (const field of fields) {
    const last = sections[sections.length - 1];
    if (last !== undefined && last.group === field.group) {
      last.fields.push(field);
    } else {
      sections.push({ group: field.group, fields: [field] });
    }
  }
  return sections;
}

/** One message per number or length field that does not hold a legal value.
 * Fields turned off by their check are not checked. */
export function validateGadget(
  fields: readonly GadgetField[],
  state: GadgetFormState,
  units: Units,
): GadgetErrors {
  const errors: GadgetErrors = {};
  for (const field of fields) {
    if (fieldDisabled(field, state)) {
      continue;
    }
    const text = state[field.id];
    if (field.kind === 'number') {
      const value = typeof text === 'string' ? numberValue(text) : null;
      errors[field.id] = fieldError(value, { min: field.min, max: field.max });
    } else if (field.kind === 'tool') {
      errors[field.id] = text === '' ? strings.gadgets.noTool : undefined;
    } else if (field.kind === 'length') {
      const value = typeof text === 'string' ? numberValue(text) : null;
      errors[field.id] = fieldError(value, {
        min: displayLimit(field.min, units),
        max: displayLimit(field.max, units),
      });
    }
  }
  return errors;
}

/** True when no field has an error. */
export function gadgetValid(errors: GadgetErrors): boolean {
  return Object.values(errors).every((error) => error === undefined);
}

/** The values gadgets.run takes, or null while a field is not legal. Fields
 * turned off by their check are left out, so the kernel uses their defaults. */
export function gadgetValues(
  fields: readonly GadgetField[],
  state: GadgetFormState,
  units: Units,
): Record<string, GadgetValue> | null {
  if (!gadgetValid(validateGadget(fields, state, units))) {
    return null;
  }
  const values: Record<string, GadgetValue> = {};
  for (const field of fields) {
    if (fieldDisabled(field, state)) {
      continue;
    }
    const current = state[field.id];
    switch (field.kind) {
      case 'number': {
        const value = typeof current === 'string' ? numberValue(current) : null;
        if (value === null) {
          return null;
        }
        values[field.id] = value;
        break;
      }
      case 'length': {
        const value = typeof current === 'string' ? lengthValue(current, units) : null;
        if (value === null) {
          return null;
        }
        // A limit typed in inches must not fail the kernel's millimetre check
        // on rounding alone.
        const low = field.min !== undefined && value < field.min ? field.min : value;
        values[field.id] = field.max !== undefined && low > field.max ? field.max : low;
        break;
      }
      case 'check':
        values[field.id] = current === true;
        break;
      case 'choice':
      case 'text':
        values[field.id] = typeof current === 'string' ? current : '';
        break;
      case 'tool':
        values[field.id] = typeof current === 'string' ? Number(current) : 0;
        break;
    }
  }
  return values;
}
