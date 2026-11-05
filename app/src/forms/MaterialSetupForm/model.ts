// Pure state for the Material Setup form: where the model sits in the
// material, rapid heights above the material and the home position.

import type { JobSetup, MachineSetup, RotaryZZero, Side, Units, ZZero } from '../../bridge/types';
import { fieldError, lengthText, lengthValue, convertLengthFields } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

export interface MaterialSetupState {
  safeZ: string;
  plungeGap: string;
  homeX: string;
  homeY: string;
  homeZ: string;
  /** The gap of the chosen radio: above the model, or below it. */
  modelGap: string;
  /** Gap Below Model (Gap Inside Model on a rotary job) is chosen. */
  gapBelow: boolean;
}

export type MaterialField = Exclude<keyof MaterialSetupState, 'gapBelow'>;
export type MaterialErrors = Partial<Record<MaterialField, string>>;

const FIELDS: readonly MaterialField[] = [
  'safeZ',
  'plungeGap',
  'homeX',
  'homeY',
  'homeZ',
  'modelGap',
];

export function materialStateFrom(machine: MachineSetup, units: Units): MaterialSetupState {
  return {
    safeZ: lengthText(machine.safeZ, units),
    plungeGap: lengthText(machine.plungeGap, units),
    homeX: lengthText(machine.homeX, units),
    homeY: lengthText(machine.homeY, units),
    homeZ: lengthText(machine.homeZ, units),
    modelGap: lengthText(machine.modelGap, units),
    gapBelow: machine.gapBelow ?? false,
  };
}

/** Thickness of the model the gaps measure, in millimetres: the active
 * side's model, plus the other side's on a double sided job (the two stand
 * back to back on one modelling plane). */
export function combinedThickness(activePeak: number, otherPeak: number, job: JobSetup): number {
  const other = job.kind === 'doubleSided' ? Math.max(otherPeak, 0) : 0;
  return Math.max(activePeak, 0) + other;
}

/** True when the model is thicker than the material it is cut from, which
 * not allowed: it then sticks out of the material. */
export function tooThick(modelThickness: number, thickness: number): boolean {
  return modelThickness > thickness + 1e-6;
}

/** The gap on the other side of the model, in millimetres: what is left of
 * the material (a rotary job's radius) beside a model `modelHeight` tall
 * with `gap` on its chosen side. The relation is the same both ways. */
export function otherGap(gap: number, thickness: number, modelHeight: number): number {
  return Math.min(Math.max(thickness - Math.max(modelHeight, 0) - gap, 0), thickness);
}

/** The state with the other gap radio chosen, its field holding the gap
 * that keeps the model where it is. */
export function chooseGap(
  state: MaterialSetupState,
  below: boolean,
  units: Units,
  thickness: number,
  modelHeight: number,
): MaterialSetupState {
  if (state.gapBelow === below) {
    return state;
  }
  const gap = lengthValue(state.modelGap, units);
  return {
    ...state,
    gapBelow: below,
    modelGap:
      gap === null ? state.modelGap : lengthText(otherGap(gap, thickness, modelHeight), units),
  };
}

/** Job Z of the modelling plane (Modeling Plane Z, for information): the
 * plane lies `depth` under the face being machined, whose job Z is
 * `surfaceZ` (0 with Z zero on the surface, the thickness on the bed or a
 * rotary job's axis). */
export function modelingPlaneZ(surfaceZ: number, depth: number): number {
  return surfaceZ - depth;
}

/** Job Z of the face being machined for a Z zero choice and thickness. */
export function surfaceZFor(zZero: ZZero | RotaryZZero, thickness: number): number {
  return zZero === 'bottom' || zZero === 'centre' ? thickness : 0;
}

export function convertMaterialUnits(
  state: MaterialSetupState,
  from: Units,
  to: Units,
): MaterialSetupState {
  return convertLengthFields(state, FIELDS, from, to);
}

export function validateMaterial(
  state: MaterialSetupState,
  units: Units,
  thickness = Number.POSITIVE_INFINITY,
): MaterialErrors {
  const errors: MaterialErrors = {};
  const safe = lengthValue(state.safeZ, units);
  const gap = lengthValue(state.plungeGap, units);
  const checks: Array<[MaterialField, string | undefined]> = [
    ['safeZ', fieldError(safe, { positive: true })],
    ['plungeGap', fieldError(gap, { min: 0 })],
    ['homeX', fieldError(lengthValue(state.homeX, units))],
    ['homeY', fieldError(lengthValue(state.homeY, units))],
    ['homeZ', fieldError(lengthValue(state.homeZ, units), { positive: true })],
    ['modelGap', fieldError(lengthValue(state.modelGap, units), { min: 0, max: thickness })],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  if (errors.plungeGap === undefined && safe !== null && gap !== null && gap > safe) {
    errors.plungeGap = strings.toolpaths.material.errorGap;
  }
  return errors;
}

/** The machine settings in millimetres, or null while a field is invalid. */
export function machineFrom(
  state: MaterialSetupState,
  units: Units,
  thickness = Number.POSITIVE_INFINITY,
): MachineSetup | null {
  if (Object.keys(validateMaterial(state, units, thickness)).length > 0) {
    return null;
  }
  const mm = (field: MaterialField) => lengthValue(state[field], units) ?? 0;
  return {
    safeZ: mm('safeZ'),
    plungeGap: mm('plungeGap'),
    homeX: mm('homeX'),
    homeY: mm('homeY'),
    homeZ: mm('homeZ'),
    modelGap: mm('modelGap'),
    gapBelow: state.gapBelow,
  };
}

/** The Z zero of the side being set up (a double sided job keeps one per side). */
export function sideZZero(job: JobSetup, side: Side): ZZero {
  return job.kind === 'doubleSided' && side === 'bottom'
    ? (job.doubleSided?.bottomZZero ?? job.zZero)
    : job.zZero;
}

/** The job with the Material Setup thickness and the side's Z zero. */
export function withMaterialRows(
  job: JobSetup,
  side: Side,
  thickness: number,
  zZero: ZZero,
): JobSetup {
  const bottom = job.kind === 'doubleSided' && side === 'bottom';
  return {
    ...job,
    thickness,
    zZero: bottom ? job.zZero : zZero,
    ...(bottom && job.doubleSided !== undefined
      ? { doubleSided: { ...job.doubleSided, bottomZZero: zZero } }
      : {}),
  };
}

/** The rotary job with the Material Setup diameter and Z zero. */
export function withRotaryRows(job: JobSetup, diameter: number, zZero: RotaryZZero): JobSetup {
  return { ...job, thickness: diameter / 2, rotary: { ...job.rotary, diameter, zZero } };
}

/** The thickness in millimetres, or null while it is not a positive length. */
export function thicknessValue(text: string, units: Units): number | null {
  const value = lengthValue(text, units);
  return value === null || !(value > 0) ? null : value;
}
