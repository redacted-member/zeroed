// Pure state for the 3D Roughing and 3D Finishing forms, which share their
// tool, boundary and allowance fields; roughing adds a pass depth, its
// strategies and ramps, finishing its cut direction and stepover retract.

import type {
  Tool,
  Boundary3d,
  CutDirection,
  Finish3dParams,
  Finish3dStrategy,
  Rough3dOrder,
  Rough3dParams,
  Rough3dProfile,
  Rough3dStrategy,
} from '../../bridge/cam';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
  type StringKeys,
} from '../../lib/formFields';

export type Kind3d = 'rough3d' | 'finish3d';

/** Raster direction on the roughing form: along X (0 degrees) or Y (90). */
export type RasterDirection = 'x' | 'y';

export interface Form3dState {
  name: string;
  boundary: Boundary3d;
  /** Model level id for a level boundary; 0 for none chosen. */
  level: number;
  boundaryOffset: string;
  angle: string;
  allowance: string;
  finishStrategy: Finish3dStrategy;
  direction: CutDirection;
  stepoverRetract: string;
  roughStrategy: Rough3dStrategy;
  profile: Rough3dProfile;
  order: Rough3dOrder;
  reverseStep: boolean;
  avoidMachined: boolean;
  ramp: boolean;
  rampDistance: string;
  /** 3D Finishing: the Tools list after the first tool, each rest machining. */
  restTools: Tool[];
  minimumDetail: string;
  restOffset: string;
}

export type Field3d =
  | 'angle'
  | 'allowance'
  | 'boundaryOffset'
  | 'stepoverRetract'
  | 'rampDistance'
  | 'minimumDetail'
  | 'restOffset';
export type Errors3d = Partial<Record<Field3d, string>>;

const LENGTHS: ReadonlyArray<StringKeys<Form3dState>> = [
  'allowance',
  'boundaryOffset',
  'stepoverRetract',
  'rampDistance',
  'minimumDetail',
  'restOffset',
];

export function initial3dState(kind: Kind3d, units: Units): Form3dState {
  const inch = units === 'inch';
  return {
    name: '',
    boundary: 'model',
    level: 0,
    boundaryOffset: lengthText(0, units),
    angle: '0',
    allowance: lengthText(kind === 'rough3d' ? (inch ? 1.016 : 1) : 0, units),
    finishStrategy: 'offset',
    direction: 'climb',
    stepoverRetract: lengthText(0, units),
    roughStrategy: 'zLevelRaster',
    profile: 'last',
    order: 'levelByLevel',
    reverseStep: false,
    avoidMachined: false,
    ramp: false,
    rampDistance: lengthText(inch ? 25.4 : 20, units),
    restTools: [],
    minimumDetail: lengthText(inch ? 0.127 : 0.1, units),
    restOffset: lengthText(inch ? 1.27 : 1, units),
  };
}

/** The Minimum Detail slider's range and step in the job unit: Coarse at
 * the large end, Fine at the small. */
export function minimumDetailRange(units: Units): { min: number; max: number; step: number } {
  return units === 'inch'
    ? { min: 0.0005, max: 0.04, step: 0.0005 }
    : { min: 0.01, max: 1, step: 0.01 };
}

/** The strategies offered; older toolpaths made with the
 * extra ones we had (Z Level offset roughing, Raster X and Y finishing)
 * open on the nearest of them. */
export function referenceRough(strategy: Rough3dStrategy): Rough3dStrategy {
  return strategy === 'zLevelOffset' ? 'zLevelRaster' : strategy;
}

export function referenceFinish(strategy: Finish3dStrategy): Finish3dStrategy {
  return strategy === 'rasterXY' ? 'raster' : strategy;
}

/** Whether the raster angle means anything for the chosen strategy: both
 * roughing strategies clear in rasters. */
export function usesAngle(kind: Kind3d, state: Form3dState): boolean {
  return kind === 'rough3d' || state.finishStrategy !== 'offset';
}

/** Z level options (profile, order, ramps) apply to Z Level roughing. */
export function isZLevel(state: Form3dState): boolean {
  return state.roughStrategy !== 'raster3d';
}

/** The profile choice (First, Last, None) belongs to Z Level roughing. */
export function usesProfile(state: Form3dState): boolean {
  return isZLevel(state);
}

/** The roughing raster direction shown for a raster angle. */
export function rasterDirection(state: Form3dState): RasterDirection {
  const angle = Math.abs((numberValue(state.angle) ?? 0) % 180);
  return angle > 45 && angle < 135 ? 'y' : 'x';
}

/** The raster angle for a roughing raster direction. */
export function angleFor(direction: RasterDirection): string {
  return direction === 'y' ? '90' : '0';
}

export function convert3dUnits(state: Form3dState, from: Units, to: Units): Form3dState {
  return convertLengthFields(state, LENGTHS, from, to);
}

export function validate3d(kind: Kind3d, state: Form3dState, units: Units): Errors3d {
  const errors: Errors3d = {};
  const checks: Array<[Field3d, string | undefined]> = [
    ['allowance', fieldError(lengthValue(state.allowance, units), { min: 0 })],
    ['boundaryOffset', fieldError(lengthValue(state.boundaryOffset, units))],
  ];
  if (usesAngle(kind, state)) {
    checks.push(['angle', fieldError(numberValue(state.angle), { min: -360, max: 360 })]);
  }
  if (kind === 'rough3d' && isZLevel(state) && state.ramp) {
    checks.push([
      'rampDistance',
      fieldError(lengthValue(state.rampDistance, units), { positive: true }),
    ]);
  }
  if (kind === 'finish3d' && state.restTools.length > 0) {
    checks.push(['minimumDetail', fieldError(lengthValue(state.minimumDetail, units), { min: 0 })]);
    checks.push(['restOffset', fieldError(lengthValue(state.restOffset, units), { min: 0 })]);
  }
  if (kind === 'finish3d' && state.finishStrategy === 'offset') {
    checks.push([
      'stepoverRetract',
      fieldError(lengthValue(state.stepoverRetract, units), { min: 0 }),
    ]);
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** True when a level boundary has no level to keep to. */
export function needsLevel(state: Form3dState): boolean {
  return state.boundary === 'level' && state.level === 0;
}

function baseParams(state: Form3dState, units: Units) {
  return {
    stepover: null,
    angle: numberValue(state.angle) ?? 0,
    allowance: lengthValue(state.allowance, units) ?? 0,
    boundary: state.boundary,
    level: state.boundary === 'level' ? state.level : 0,
    boundaryOffset: lengthValue(state.boundaryOffset, units) ?? 0,
  };
}

export function finish3dParams(state: Form3dState, units: Units): Finish3dParams | null {
  if (Object.keys(validate3d('finish3d', state, units)).length > 0 || needsLevel(state)) {
    return null;
  }
  const offset = state.finishStrategy === 'offset';
  return {
    ...baseParams(state, units),
    strategy: state.finishStrategy,
    direction: state.direction,
    stepoverRetract: offset ? (lengthValue(state.stepoverRetract, units) ?? 0) : 0,
    restTools: state.restTools,
    minimumDetail: lengthValue(state.minimumDetail, units) ?? 0,
    restBoundaryOffset: lengthValue(state.restOffset, units) ?? 0,
  };
}

export function rough3dParams(state: Form3dState, units: Units): Rough3dParams | null {
  if (Object.keys(validate3d('rough3d', state, units)).length > 0 || needsLevel(state)) {
    return null;
  }
  const zLevel = isZLevel(state);
  return {
    ...baseParams(state, units),
    passDepth: null,
    strategy: state.roughStrategy,
    profileEachLevel: false,
    profile: usesProfile(state) ? state.profile : 'none',
    order: zLevel ? state.order : 'levelByLevel',
    reverseStep: state.reverseStep,
    avoidMachined: !zLevel && state.avoidMachined,
    ramp: {
      enabled: zLevel && state.ramp,
      distance: lengthValue(state.rampDistance, units) ?? 0,
    },
  };
}

export function state3dFrom(
  kind: Kind3d,
  params: Finish3dParams | Rough3dParams,
  name: string,
  units: Units,
): Form3dState {
  const base = initial3dState(kind, units);
  const rough = 'passDepth' in params ? params : null;
  const finish = rough === null ? (params as Finish3dParams) : null;
  const length = (value: number | undefined, fallback: string) =>
    value === undefined ? fallback : lengthText(value, units);
  return {
    ...base,
    name,
    boundary: params.boundary,
    level: params.level ?? 0,
    boundaryOffset: length(params.boundaryOffset, base.boundaryOffset),
    angle: String(params.angle),
    allowance: lengthText(params.allowance, units),
    finishStrategy: referenceFinish(finish?.strategy ?? base.finishStrategy),
    direction: finish?.direction ?? base.direction,
    stepoverRetract: length(finish?.stepoverRetract, base.stepoverRetract),
    roughStrategy: referenceRough(rough?.strategy ?? base.roughStrategy),
    profile: rough?.profile ?? (rough?.profileEachLevel === true ? 'last' : base.profile),
    order: rough?.order ?? base.order,
    reverseStep: rough?.reverseStep ?? base.reverseStep,
    avoidMachined: rough?.avoidMachined ?? base.avoidMachined,
    ramp: rough?.ramp?.enabled ?? base.ramp,
    rampDistance: length(rough?.ramp?.distance, base.rampDistance),
    restTools: finish?.restTools ?? [],
    minimumDetail: length(finish?.minimumDetail, base.minimumDetail),
    restOffset: length(finish?.restBoundaryOffset, base.restOffset),
  };
}
