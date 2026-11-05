// Pure state for the Moulding Toolpath form. The rails and the profile are
// the selection in the order it was made: every vector but the last is a
// rail and the last is the profile. The toolpath's place in the material is
// its gap above (the kernel's start depth) or its gap below.

import type { MouldingParams, MouldingStrategy, Tool } from '../../bridge/cam';
import type { EntitySummary, Path } from '../../bridge/document';
import type { VectorFilter } from '../../bridge/sketch';
import type { Units } from '../../bridge/types';
import { convertLengthFields, fieldError, lengthText, lengthValue } from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

const text = strings.toolpaths.moulding;

/** Which gap the toolpath position is typed as. */
export type MouldingPosition = 'above' | 'below';

export interface MouldingFormState {
  name: string;
  position: MouldingPosition;
  /** Material above the profile's top. */
  gapAbove: string;
  /** Material under the profile's lowest point. */
  gapBelow: string;
  varyStepover: boolean;
  skipFlats: boolean;
  useClearance: boolean;
  /** The larger area clearance tool, kept while it is switched off. */
  clearanceTool: Tool | null;
  machineFlats: boolean;
  clearanceRamp: boolean;
  clearanceRampDistance: string;
  clearanceAllowance: string;
  sharpCorners: boolean;
  boundaryOffset: string;
  autoBoundaryOffset: boolean;
  /** The Profile Selection filter; the Rail Selection is the form's own. */
  profileSelector: VectorFilter | null;
  /** Kept from older toolpaths, which the form no longer offers. */
  kept: {
    strategy: MouldingStrategy;
    widthScale: number;
    heightScale: number;
    allowance: number;
  };
}

export type MouldingField =
  | 'rail'
  | 'profile'
  | 'gapAbove'
  | 'gapBelow'
  | 'boundaryOffset'
  | 'clearanceAllowance'
  | 'clearanceRampDistance';
export type MouldingErrors = Partial<Record<MouldingField, string>>;

/** What the form reads from the job and the drawing, in millimetres. */
export interface MouldingContext {
  /** The Rail Selection is automatic: the kernel picks the vectors. */
  automatic?: boolean;
  rails: readonly number[];
  profile: number | null;
  thickness: number;
  /** The profile's drawn height, or null without a profile. */
  profileHeight: number | null;
}

export interface MouldingArgs {
  /** Rails first, then the profile last, as the kernel reads them. */
  ids: number[];
  params: MouldingParams & { profileSelector?: VectorFilter };
}

const LENGTHS = [
  'gapAbove',
  'gapBelow',
  'boundaryOffset',
  'clearanceAllowance',
  'clearanceRampDistance',
] as const;

export function initialMouldingState(units: Units): MouldingFormState {
  return {
    name: '',
    position: 'above',
    gapAbove: lengthText(0, units),
    gapBelow: lengthText(0, units),
    varyStepover: true,
    skipFlats: false,
    useClearance: false,
    clearanceTool: null,
    machineFlats: false,
    clearanceRamp: false,
    clearanceRampDistance: lengthText(units === 'inch' ? 12.7 : 10, units),
    clearanceAllowance: lengthText(0, units),
    sharpCorners: false,
    boundaryOffset: lengthText(0, units),
    autoBoundaryOffset: true,
    profileSelector: null,
    kept: { strategy: 'along', widthScale: 1, heightScale: 1, allowance: 0 },
  };
}

export function convertMouldingUnits(
  state: MouldingFormState,
  from: Units,
  to: Units,
): MouldingFormState {
  return convertLengthFields(state, LENGTHS, from, to);
}

/** The selected vectors in selection order: the rails, then the profile last. */
export function railsAndProfile(
  ids: readonly number[],
  entities: readonly EntitySummary[],
): { rails: number[]; profile: number | null } {
  const vectors = ids.filter((id) => entities.some((e) => e.id === id && e.kind === 'path'));
  if (vectors.length < 2) {
    return { rails: vectors, profile: null };
  }
  return { rails: vectors.slice(0, -1), profile: vectors[vectors.length - 1] ?? null };
}

/** The profile's height as the moulding cuts it, from its drawn size. */
export function profileHeight(
  profile: number | null,
  entities: readonly EntitySummary[],
  heightScale = 1,
): number | null {
  const entity = profile === null ? undefined : entities.find((e) => e.id === profile);
  return entity === undefined ? null : (entity.bbox[3] - entity.bbox[1]) * heightScale;
}

/** Room for the toolpath's gaps: the material less the profile's height. */
function room(context: MouldingContext): number {
  return context.thickness - (context.profileHeight ?? 0);
}

/** The gap above the profile the position gives, in millimetres. */
export function gapAboveOf(
  state: MouldingFormState,
  units: Units,
  context: MouldingContext,
): number | null {
  if (state.position === 'above') {
    return lengthValue(state.gapAbove, units);
  }
  const below = lengthValue(state.gapBelow, units);
  return below === null ? null : room(context) - below;
}

/** The text the gap that is not being typed shows, worked out from the other. */
export function otherGap(state: MouldingFormState, units: Units, context: MouldingContext): string {
  const above = gapAboveOf(state, units, context);
  if (above === null) {
    return '';
  }
  return lengthText(state.position === 'above' ? room(context) - above : above, units);
}

/** Switches which gap is typed, carrying the position over. */
export function withPosition(
  state: MouldingFormState,
  position: MouldingPosition,
  units: Units,
  context: MouldingContext,
): MouldingFormState {
  if (position === state.position) {
    return state;
  }
  const shown = otherGap(state, units, context);
  return position === 'above'
    ? { ...state, position, gapAbove: shown === '' ? state.gapAbove : shown }
    : { ...state, position, gapBelow: shown === '' ? state.gapBelow : shown };
}

/** The slider moves the toolpath down by `above` from the top. */
export function withSlider(
  state: MouldingFormState,
  above: number,
  units: Units,
  context: MouldingContext,
): MouldingFormState {
  return state.position === 'above'
    ? { ...state, gapAbove: lengthText(above, units) }
    : { ...state, gapBelow: lengthText(room(context) - above, units) };
}

/** Deepest point of the moulding below the surface, for the Cut Depth readout. */
export function cutDepth(
  state: MouldingFormState,
  units: Units,
  context: MouldingContext,
): number | null {
  const above = gapAboveOf(state, units, context);
  return above === null ? null : above + (context.profileHeight ?? 0);
}

export function validateMoulding(
  state: MouldingFormState,
  units: Units,
  context: MouldingContext,
): MouldingErrors {
  const errors: MouldingErrors = {};
  const manual = context.automatic !== true;
  if (manual && context.rails.length === 0) {
    errors.rail = text.needsRail;
  }
  if (manual && context.profile === null) {
    errors.profile = text.needsProfile;
  }
  const typed = state.position === 'above' ? 'gapAbove' : 'gapBelow';
  const gap = fieldError(
    lengthValue(state.position === 'above' ? state.gapAbove : state.gapBelow, units),
    { min: 0 },
  );
  const above = gapAboveOf(state, units, context);
  if (gap !== undefined) {
    errors[typed] = gap;
  } else if (above !== null && above < -1e-9) {
    errors[typed] = text.tooDeep;
  }
  const checks: Array<[MouldingField, string | undefined]> = [
    [
      'boundaryOffset',
      state.autoBoundaryOffset
        ? undefined
        : fieldError(lengthValue(state.boundaryOffset, units), { min: 0 }),
    ],
    [
      'clearanceAllowance',
      state.useClearance
        ? fieldError(lengthValue(state.clearanceAllowance, units), { min: 0 })
        : undefined,
    ],
    [
      'clearanceRampDistance',
      state.useClearance && state.clearanceRamp
        ? fieldError(lengthValue(state.clearanceRampDistance, units), { positive: true })
        : undefined,
    ],
  ];
  for (const [field, error] of checks) {
    if (error !== undefined) {
      errors[field] = error;
    }
  }
  return errors;
}

/** Kernel ids and params, or null while the form is not valid. */
export function mouldingArgs(
  state: MouldingFormState,
  units: Units,
  context: MouldingContext,
): MouldingArgs | null {
  const { rails, profile } = context;
  const above = gapAboveOf(state, units, context);
  if (Object.keys(validateMoulding(state, units, context)).length > 0 || above === null) {
    return null;
  }
  const clearance = state.useClearance ? state.clearanceTool : null;
  return {
    ids: profile === null ? [...rails] : [...rails, profile],
    params: {
      stepover: null,
      widthScale: state.kept.widthScale,
      heightScale: state.kept.heightScale,
      allowance: state.kept.allowance,
      strategy: state.kept.strategy,
      startDepth: Math.max(0, above),
      boundaryOffset: lengthValue(state.boundaryOffset, units) ?? 0,
      autoBoundaryOffset: state.autoBoundaryOffset,
      varyStepover: state.varyStepover,
      sharpCorners: state.sharpCorners,
      clearanceTool: clearance,
      clearanceAllowance: lengthValue(state.clearanceAllowance, units) ?? 0,
      clearanceRamp: {
        enabled: state.clearanceRamp,
        distance: lengthValue(state.clearanceRampDistance, units) ?? 10,
      },
      machineFlatRegions: clearance !== null && state.machineFlats,
      skipFlatRegions: clearance !== null && state.machineFlats && state.skipFlats,
      ...(state.profileSelector === null ? {} : { profileSelector: state.profileSelector }),
    },
  };
}

export function mouldingStateFrom(
  params: MouldingParams & { profileSelector?: VectorFilter | null },
  name: string,
  units: Units,
): MouldingFormState {
  const base = initialMouldingState(units);
  const clearance = params.clearanceTool ?? null;
  return {
    ...base,
    name,
    position: 'above',
    gapAbove: lengthText(params.startDepth ?? 0, units),
    varyStepover: params.varyStepover ?? false,
    skipFlats: params.skipFlatRegions ?? false,
    useClearance: clearance !== null,
    clearanceTool: clearance,
    machineFlats: params.machineFlatRegions ?? false,
    clearanceRamp: params.clearanceRamp?.enabled ?? false,
    clearanceRampDistance:
      params.clearanceRamp === undefined
        ? base.clearanceRampDistance
        : lengthText(params.clearanceRamp.distance, units),
    clearanceAllowance: lengthText(params.clearanceAllowance ?? 0.5, units),
    sharpCorners: params.sharpCorners ?? false,
    boundaryOffset: lengthText(params.boundaryOffset ?? 0, units),
    autoBoundaryOffset: params.autoBoundaryOffset ?? false,
    profileSelector: params.profileSelector ?? null,
    kept: {
      strategy: params.strategy,
      widthScale: params.widthScale,
      heightScale: params.heightScale,
      allowance: params.allowance,
    },
  };
}

/** Preview paths: the rails and the profile that are loaded. */
export function mouldingPreview(
  context: Pick<MouldingContext, 'rails' | 'profile'>,
  paths: ReadonlyMap<number, Path>,
): Path[] {
  const out: Path[] = [];
  for (const id of [...context.rails, context.profile]) {
    const path = id === null ? undefined : paths.get(id);
    if (path !== undefined) {
      out.push(path);
    }
  }
  return out;
}
