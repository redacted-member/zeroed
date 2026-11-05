// Pure state for the 2D Profile Toolpath form.

import type {
  BoxPoint,
  CutDirection,
  CutOrder,
  LeadKind,
  OpenSide,
  ProfileParams,
  ProfileSide,
  RampKind,
  StartAt,
  TabAnchor,
  TabSpec,
} from '../../bridge/cam';
import type { Point } from '../../bridge/document';
import type { JobSetup, Units } from '../../bridge/types';
import { rotaryOf } from '../../lib/rotary';
import type { Anchor } from '../../lib/shapes';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import {
  convertPasses,
  initialPasses,
  passStateFrom,
  passValues,
  validatePasses,
  type PassErrors,
  type PassState,
} from '../../lib/passFields';
import {
  cutDepthText,
  convertDepths,
  depthValues,
  initialDepths,
  validateDepths,
  type DepthErrors,
  type DepthState,
} from '../../lib/toolpathFields';

export type TabPlacement = 'count' | 'distance';
/** Where Add Tabs puts the tabs (Edit Tabs dialog). */
export type TabRule = 'firstAtStart' | 'avoidCorners';
export type RampBy = 'distance' | 'angle';
/** An option of the Order page; none ticked cuts nearest next. */
export type OrderOption = Exclude<CutOrder, 'optimized'>;

export interface ProfileFormState {
  name: string;
  depths: DepthState;
  /** Show advanced toolpath options: last pass, tabs and the option pages.
   * Hidden options are not used. */
  advanced: boolean;
  passes: PassState;
  side: ProfileSide;
  direction: CutDirection;
  allowance: string;
  lastPass: boolean;
  lastPassAllowance: string;
  reverseLastPass: boolean;
  tabs: boolean;
  tabTriangular: boolean;
  tabPlacement: TabPlacement;
  tabCount: string;
  tabSpacing: string;
  tabLength: string;
  tabThickness: string;
  tabRule: TabRule;
  /** Tabs placed in the Edit Tabs dialog; null spreads them by number or distance. */
  tabPositions: Point[] | null;
  /** The kernel's anchors of the placed tabs, sent back as they came. */
  tabAnchors: Array<TabAnchor | null> | null;
  ramp: boolean;
  rampKind: RampKind;
  rampBy: RampBy;
  rampDistance: string;
  rampAngle: string;
  rampOnLeadIn: boolean;
  leadKind: LeadKind;
  leadLength: string;
  leadAngle: string;
  leadRadius: string;
  /** How far round a circular lead turns, in degrees. */
  leadArcAngle: string;
  leadOvercut: string;
  leadOut: boolean;
  orders: OrderOption[];
  startAt: StartAt;
  /** The bounding box point Closest on Bounding Box starts near. */
  startAnchor: Anchor;
  sharpCorners: boolean;
  sharpInternalCorners: boolean;
  /** How far the wall leans back as it rises, degrees; 0 for upright. */
  wallAngle: string;
  /** The most a tapered wall's steps may stand out of the slope. */
  scallop: string;
  /** Kept from a Joint Creator socket wall so a recalculation keeps it rigid. */
  rigid: boolean;
}

export type ProfileField =
  | 'wallAngle'
  | 'scallop'
  | 'allowance'
  | 'lastPassAllowance'
  | 'tabCount'
  | 'tabSpacing'
  | 'tabLength'
  | 'tabThickness'
  | 'rampDistance'
  | 'rampAngle'
  | 'leadLength'
  | 'leadAngle'
  | 'leadRadius'
  | 'leadArcAngle'
  | 'leadOvercut';
export type ProfileErrors = DepthErrors & PassErrors & Partial<Record<ProfileField, string>>;

/**
 * The depth a new Profile cuts to, millimetres: the material thickness,
 * which for a rotary job is the blank's radius (the job's thickness is set
 * to the radius of the cylinder, so a profile cuts down to its axis).
 */
export function profileThroughDepth(job: JobSetup): number {
  return job.kind === 'rotary' ? rotaryOf(job).diameter / 2 : job.thickness;
}

/**
 * The form as it first opens. The cut depth starts at the material
 * thickness (millimetres) so a profile cuts the part out and no deeper; with
 * no thickness known, at 18 mm (3/4 inch).
 */
export function initialProfileState(units: Units, thicknessMm?: number): ProfileFormState {
  const inch = units === 'inch';
  const through = thicknessMm !== undefined && thicknessMm > 0 ? thicknessMm : null;
  return {
    name: '',
    depths: initialDepths(units, through ?? (inch ? 19.05 : 18)),
    advanced: true,
    passes: initialPasses(units),
    side: 'outside',
    direction: 'climb',
    allowance: lengthText(0, units),
    lastPass: false,
    lastPassAllowance: lengthText(0, units),
    reverseLastPass: false,
    tabs: false,
    tabTriangular: false,
    tabPlacement: 'count',
    tabCount: '2',
    tabSpacing: lengthText(inch ? 254 : 250, units),
    tabLength: lengthText(inch ? 12.7 : 10, units),
    tabThickness: lengthText(inch ? 3.175 : 3, units),
    tabRule: 'avoidCorners',
    tabPositions: null,
    tabAnchors: null,
    ramp: false,
    rampKind: 'smooth',
    rampBy: 'distance',
    rampDistance: lengthText(inch ? 25.4 : 20, units),
    rampAngle: '20',
    rampOnLeadIn: false,
    leadKind: 'none',
    leadLength: lengthText(inch ? 5.08 : 5, units),
    leadAngle: '45',
    leadRadius: lengthText(inch ? 5.08 : 5, units),
    leadArcAngle: '90',
    leadOvercut: lengthText(0, units),
    leadOut: true,
    orders: [],
    startAt: 'optimized',
    startAnchor: 'bottomLeft',
    sharpCorners: false,
    sharpInternalCorners: false,
    wallAngle: '0',
    scallop: lengthText(inch ? 0.0254 : 0.02, units),
    rigid: false,
  };
}

export function convertProfileUnits(
  state: ProfileFormState,
  from: Units,
  to: Units,
): ProfileFormState {
  return {
    ...convertLengthFields(
      state,
      [
        'allowance',
        'lastPassAllowance',
        'tabSpacing',
        'tabLength',
        'tabThickness',
        'rampDistance',
        'leadLength',
        'leadRadius',
        'leadOvercut',
        'scallop',
      ],
      from,
      to,
    ),
    depths: convertDepths(state.depths, from, to),
    passes: convertPasses(state.passes, from, to),
  };
}

/** The form as last calculated, in the units it was typed in. */
export interface RememberedProfile {
  state: ProfileFormState;
  units: Units;
}

/**
 * A new Profile form: the fields as they were when a profile was last
 * calculated (fields keep their previous setting once edited), in today's
 * units, with a fresh name and no hand placed tabs (those belong to the
 * vectors they were placed on); the first time, the form as it first opens.
 */
export function newProfileState(
  last: RememberedProfile | null,
  units: Units,
  thicknessMm?: number,
): ProfileFormState {
  if (last === null) {
    return initialProfileState(units, thicknessMm);
  }
  const state =
    last.units === units ? last.state : convertProfileUnits(last.state, last.units, units);
  return { ...state, name: '', tabPositions: null, tabAnchors: null };
}

export function validateProfile(state: ProfileFormState, units: Units): ProfileErrors {
  const errors: ProfileErrors = {
    ...validateDepths(state.depths, units),
    ...validatePasses(state.passes, units),
  };
  const checks: Array<[ProfileField, string | undefined]> = [
    ['allowance', fieldError(lengthValue(state.allowance, units))],
    ['wallAngle', fieldError(numberValue(state.wallAngle), { min: 0, max: 45 })],
  ];
  if (tapered(state)) {
    checks.push(['scallop', scallopError(state, units)]);
  }
  const advanced = state.advanced;
  if (advanced && state.lastPass) {
    checks.push([
      'lastPassAllowance',
      fieldError(lengthValue(state.lastPassAllowance, units), { min: 0 }),
    ]);
  }
  if (advanced && state.tabs) {
    checks.push(
      ['tabLength', fieldError(lengthValue(state.tabLength, units), { positive: true })],
      ['tabThickness', fieldError(lengthValue(state.tabThickness, units), { positive: true })],
    );
    if (state.tabPlacement === 'count') {
      checks.push([
        'tabCount',
        fieldError(numberValue(state.tabCount), { min: 1, max: 100, integer: true }),
      ]);
    } else {
      checks.push([
        'tabSpacing',
        fieldError(lengthValue(state.tabSpacing, units), { positive: true }),
      ]);
    }
  }
  const ramp = rampFields(state);
  if (ramp.angle) {
    checks.push(['rampAngle', fieldError(numberValue(state.rampAngle), { min: 0.1, max: 89 })]);
  }
  if (ramp.distance) {
    checks.push([
      'rampDistance',
      fieldError(lengthValue(state.rampDistance, units), { positive: true }),
    ]);
  }
  if (advanced && state.leadKind === 'straight') {
    checks.push(
      ['leadLength', fieldError(lengthValue(state.leadLength, units), { positive: true })],
      ['leadAngle', fieldError(numberValue(state.leadAngle), { min: 0, max: 90 })],
    );
  }
  if (advanced && state.leadKind === 'arc') {
    checks.push(
      ['leadRadius', fieldError(lengthValue(state.leadRadius, units), { positive: true })],
      ['leadArcAngle', fieldError(numberValue(state.leadArcAngle), { min: 0.1, max: 90 })],
    );
  }
  if (advanced && state.leadKind !== 'none') {
    checks.push(['leadOvercut', fieldError(lengthValue(state.leadOvercut, units), { min: 0 })]);
  }
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** Whether the form asks for a tapered wall. */
export function tapered(state: ProfileFormState): boolean {
  const angle = numberValue(state.wallAngle);
  return angle !== null && angle > 0;
}

/** The scallop must be from 0.001 to 1 mm. */
function scallopError(state: ProfileFormState, units: Units): string | undefined {
  return fieldError(lengthValue(state.scallop, units), { min: 0.001, max: 1 });
}

/** Open vectors are cut to the right for Outside and to the left for Inside. */
export function openSideFor(side: ProfileSide): OpenSide {
  if (side === 'outside') {
    return 'right';
  }
  return side === 'inside' ? 'left' : 'on';
}

/** Which ramp controls are in use: Specify Ramp, its Distance and its Angle.
 * A smooth ramp takes a distance or an angle, a zig zag a distance or an
 * angle and a distance; a spiral and a ramp on the lead in take neither. */
export function rampFields(state: ProfileFormState): {
  by: boolean;
  distance: boolean;
  angle: boolean;
} {
  const on =
    state.advanced &&
    state.ramp &&
    rampKindFor(state) !== 'spiral' &&
    !(state.rampOnLeadIn && state.leadKind !== 'none');
  return {
    by: on,
    distance: on && (state.rampBy === 'distance' || state.rampKind === 'zigZag'),
    angle: on && state.rampBy === 'angle',
  };
}

/** The length of a circular lead from its radius and angle, or null. */
export function leadArcLength(state: ProfileFormState, units: Units): number | null {
  const radius = lengthValue(state.leadRadius, units);
  const angle = numberValue(state.leadArcAngle);
  if (radius === null || angle === null || radius <= 0 || angle <= 0) {
    return null;
  }
  return (radius * angle * Math.PI) / 180;
}

const TO_BOX_POINT: Record<Anchor, BoxPoint> = {
  topLeft: 'topLeft',
  top: 'topCentre',
  topRight: 'topRight',
  left: 'middleLeft',
  centre: 'centre',
  right: 'middleRight',
  bottomLeft: 'bottomLeft',
  bottom: 'bottomCentre',
  bottomRight: 'bottomRight',
};

function anchorOf(point: BoxPoint | undefined): Anchor {
  const found = (Object.keys(TO_BOX_POINT) as Anchor[]).find((a) => TO_BOX_POINT[a] === point);
  return found ?? 'bottomLeft';
}

/** Whether a toolpath uses any of the advanced options. */
function usesAdvanced(params: ProfileParams): boolean {
  return (
    (params.lastPass?.enabled ?? false) ||
    params.tabs.enabled ||
    params.ramp.enabled ||
    (params.lead !== undefined && params.lead.kind !== 'none') ||
    (params.orders ?? []).length > 0 ||
    (params.order ?? 'optimized') !== 'optimized' ||
    (params.startAt ?? 'optimized') !== 'optimized' ||
    (params.sharpCorners ?? false) ||
    (params.sharpInternalCorners ?? false)
  );
}

/** A spiral ramp needs the whole loop, so it is only offered without leads. */
export function spiralAllowed(state: ProfileFormState): boolean {
  return state.leadKind === 'none';
}

function rampKindFor(state: ProfileFormState): RampKind {
  return state.rampKind === 'spiral' && !spiralAllowed(state) ? 'smooth' : state.rampKind;
}

/** Kernel parameters, or null while a field is invalid. */
export function profileParams(state: ProfileFormState, units: Units): ProfileParams | null {
  const depths = depthValues(state.depths, units);
  if (depths === null || Object.keys(validateProfile(state, units)).length > 0) {
    return null;
  }
  const advanced = state.advanced;
  return {
    ...depths,
    ...passValues(state.passes, units, depths.startDepth),
    side: state.side,
    openSide: openSideFor(state.side),
    direction: state.direction,
    allowance: lengthValue(state.allowance, units) ?? 0,
    lastPass: {
      enabled: advanced && state.lastPass,
      allowance: lengthValue(state.lastPassAllowance, units) ?? 0,
      reverse: state.reverseLastPass,
    },
    tabs: { ...tabSpec(state, units), enabled: advanced && state.tabs },
    tabPositions: state.tabPositions,
    tabAnchors: state.tabAnchors,
    ramp: {
      enabled: advanced && state.ramp,
      distance: lengthValue(state.rampDistance, units) ?? 20,
      kind: rampKindFor(state),
      angle: state.rampBy === 'angle' ? numberValue(state.rampAngle) : null,
      onLeadIn: state.rampOnLeadIn,
    },
    lead: {
      kind: advanced ? state.leadKind : 'none',
      length: lengthValue(state.leadLength, units) ?? 5,
      angleDeg: numberValue(state.leadAngle) ?? 45,
      radius: lengthValue(state.leadRadius, units) ?? 5,
      arcAngleDeg: numberValue(state.leadArcAngle) ?? 90,
      overcut: lengthValue(state.leadOvercut, units) ?? 0,
      leadOut: state.leadOut,
    },
    order: 'optimized',
    orders: advanced ? ORDER_OPTIONS.filter((o) => state.orders.includes(o)) : [],
    startAt: advanced ? state.startAt : 'optimized',
    startAnchor: TO_BOX_POINT[state.startAnchor],
    sharpCorners: advanced && state.sharpCorners,
    sharpInternalCorners: advanced && state.sharpInternalCorners,
    wallAngle: numberValue(state.wallAngle) ?? 0,
    scallop: lengthValue(state.scallop, units) ?? 0.02,
    ...(state.rigid ? { rigid: true } : {}),
  };
}

/** The Order page's check boxes, in the order they are shown. */
export const ORDER_OPTIONS: readonly OrderOption[] = [
  'selectionOrder',
  'leftToRight',
  'bottomToTop',
  'grid',
];

/** Ticks or unticks one Order option. */
export function toggleOrder(
  state: ProfileFormState,
  option: OrderOption,
  on: boolean,
): OrderOption[] {
  const rest = state.orders.filter((o) => o !== option);
  return on ? [...rest, option] : rest;
}

/** The tab settings the kernel takes, for the toolpath and the Edit Tabs dialog. */
export function tabSpec(state: ProfileFormState, units: Units): TabSpec {
  return {
    enabled: state.tabs,
    count: numberValue(state.tabCount) ?? 2,
    length: lengthValue(state.tabLength, units) ?? 10,
    thickness: lengthValue(state.tabThickness, units) ?? 3,
    triangular: state.tabTriangular,
    spacing: state.tabPlacement === 'distance' ? lengthValue(state.tabSpacing, units) : null,
    placement: state.tabRule,
  };
}

/** Form state for an existing toolpath's parameters. Fields added after a
 * toolpath was saved are missing from it and keep their defaults. */
export function profileStateFrom(
  params: ProfileParams,
  name: string,
  units: Units,
  advanced = true,
): ProfileFormState {
  const base = initialProfileState(units);
  const spacing = params.tabs.spacing ?? null;
  const rampAngle = params.ramp.angle ?? null;
  return {
    ...base,
    name,
    advanced: advanced || usesAdvanced(params),
    depths: {
      startDepth: lengthText(params.startDepth, units),
      cutDepth: cutDepthText(params.startDepth, params.cutDepth, units),
    },
    passes: passStateFrom(params.passCount, params.lastPassThickness, units, {
      exact: params.exactPassDepth ?? false,
      list: params.passList ?? null,
      startDepth: params.startDepth,
    }),
    side: params.side,
    direction: params.direction,
    allowance: lengthText(params.allowance, units),
    lastPass: params.lastPass?.enabled ?? false,
    lastPassAllowance:
      params.lastPass === undefined
        ? base.lastPassAllowance
        : lengthText(params.lastPass.allowance, units),
    reverseLastPass: params.lastPass?.reverse ?? false,
    tabs: params.tabs.enabled,
    tabTriangular: params.tabs.triangular ?? false,
    tabPlacement: spacing === null ? 'count' : 'distance',
    tabRule: params.tabs.placement === 'firstAtStart' ? 'firstAtStart' : 'avoidCorners',
    tabPositions: params.tabPositions ?? null,
    tabAnchors: params.tabAnchors ?? null,
    tabCount: String(params.tabs.count),
    tabSpacing: spacing === null ? base.tabSpacing : lengthText(spacing, units),
    tabLength: lengthText(params.tabs.length, units),
    tabThickness: lengthText(params.tabs.thickness, units),
    ramp: params.ramp.enabled,
    rampKind: params.ramp.kind ?? 'smooth',
    rampBy: rampAngle === null ? 'distance' : 'angle',
    rampDistance: lengthText(params.ramp.distance, units),
    rampAngle: rampAngle === null ? base.rampAngle : String(rampAngle),
    rampOnLeadIn: params.ramp.onLeadIn ?? false,
    ...leadStateFrom(params.lead, base, units),
    orders: ordersFrom(params),
    startAt: params.startAt ?? 'optimized',
    startAnchor: anchorOf(params.startAnchor),
    sharpCorners: params.sharpCorners ?? false,
    sharpInternalCorners: params.sharpInternalCorners ?? false,
    wallAngle: String(params.wallAngle ?? 0),
    scallop: params.scallop === undefined ? base.scallop : lengthText(params.scallop, units),
    rigid: params.rigid ?? false,
  };
}

/** The ticked Order options; older toolpaths had one order. */
function ordersFrom(params: ProfileParams): OrderOption[] {
  if (params.orders !== undefined) {
    return ORDER_OPTIONS.filter((o) => params.orders?.includes(o) ?? false);
  }
  const order = params.order ?? 'optimized';
  return order === 'optimized' ? [] : [order];
}

type LeadState = Pick<
  ProfileFormState,
  | 'leadKind'
  | 'leadLength'
  | 'leadAngle'
  | 'leadRadius'
  | 'leadArcAngle'
  | 'leadOvercut'
  | 'leadOut'
>;

/** Toolpaths saved before leads existed have no lead: keep the defaults. */
function leadStateFrom(
  lead: ProfileParams['lead'] | undefined,
  base: ProfileFormState,
  units: Units,
): LeadState {
  if (lead === undefined) {
    return {
      leadKind: base.leadKind,
      leadLength: base.leadLength,
      leadAngle: base.leadAngle,
      leadRadius: base.leadRadius,
      leadArcAngle: base.leadArcAngle,
      leadOvercut: base.leadOvercut,
      leadOut: base.leadOut,
    };
  }
  return {
    leadKind: lead.kind,
    leadLength: lengthText(lead.length, units),
    leadAngle: String(lead.angleDeg),
    leadRadius: lengthText(lead.radius, units),
    leadArcAngle: String(lead.arcAngleDeg ?? 90),
    leadOvercut: lengthText(lead.overcut, units),
    leadOut: lead.leadOut ?? true,
  };
}
