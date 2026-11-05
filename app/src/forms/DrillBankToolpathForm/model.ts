// Pure state for the Drill Bank Toolpath form: the bank, how closely
// circles must match its drills, and the drilling depths and pecks, which
// are the Drilling form's.

import type { DrillBankParams, DrillBankTemplate } from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { defaultTemplate } from '../DrillBankTemplateDialog/model';
import {
  convertDrillUnits,
  drillParams,
  drillStateFrom,
  initialDrillState,
  validateDrill,
  type DrillErrors,
  type DrillFormState,
} from '../DrillToolpathForm/model';

export interface DrillBankFormState {
  name: string;
  /** The bank, lengths in millimetres. */
  template: DrillBankTemplate;
  pitchTolerance: string;
  diameterTolerance: string;
  drill: DrillFormState;
}

export type DrillBankField = 'pitchTolerance' | 'diameterTolerance';
export type DrillBankErrors = Partial<Record<DrillBankField, string>> & DrillErrors;

export function initialDrillBankState(units: Units): DrillBankFormState {
  const tolerance = lengthText(units === 'inch' ? 0.127 : 0.1, units);
  return {
    name: '',
    template: defaultTemplate(),
    pitchTolerance: tolerance,
    diameterTolerance: tolerance,
    drill: initialDrillState(units),
  };
}

export function convertDrillBankUnits(
  state: DrillBankFormState,
  from: Units,
  to: Units,
): DrillBankFormState {
  const converted = convertLengthFields(state, ['pitchTolerance', 'diameterTolerance'], from, to);
  return { ...converted, drill: convertDrillUnits(state.drill, from, to) };
}

export function validateDrillBank(state: DrillBankFormState, units: Units): DrillBankErrors {
  const errors: DrillBankErrors = { ...validateDrill(state.drill, units) };
  const pitch = fieldError(lengthValue(state.pitchTolerance, units), { min: 0 });
  const diameter = fieldError(lengthValue(state.diameterTolerance, units), { min: 0 });
  if (pitch !== undefined) {
    errors.pitchTolerance = pitch;
  }
  if (diameter !== undefined) {
    errors.diameterTolerance = diameter;
  }
  return errors;
}

/** Kernel parameters, or null while a field is invalid. Pecks go the
 * tool's pass depth (`toolPassDepth`, millimetres) at a time. */
export function drillBankParams(
  state: DrillBankFormState,
  units: Units,
  toolPassDepth: number,
): DrillBankParams | null {
  const drill = drillParams(state.drill, units, toolPassDepth);
  if (drill === null || Object.keys(validateDrillBank(state, units)).length > 0) {
    return null;
  }
  return {
    template: state.template,
    pitchTolerance: lengthValue(state.pitchTolerance, units) ?? 0,
    diameterTolerance: lengthValue(state.diameterTolerance, units) ?? 0,
    drill,
  };
}

export function drillBankStateFrom(
  params: DrillBankParams,
  name: string,
  units: Units,
): DrillBankFormState {
  return {
    name,
    template: params.template,
    pitchTolerance: lengthText(params.pitchTolerance, units),
    diameterTolerance: lengthText(params.diameterTolerance, units),
    drill: drillStateFrom(params.drill, '', units),
  };
}

/** A one line description of the bank for the form. */
export function bankSummary(template: DrillBankTemplate): { drills: number; grid: string } {
  return {
    drills: template.drills.filter((d) => d.number > 0).length,
    grid: `${template.horizontal} x ${template.vertical}`,
  };
}
