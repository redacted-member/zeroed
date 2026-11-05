// Pure form state for the job setup form: what the fields hold, how they read
// in the chosen unit, what is valid and how it maps back to a kernel job.

import type { Material3d } from '../../bridge/prefs';
import type {
  FlipAxis,
  JobKind,
  JobSetup,
  MachineSetup,
  ModelResolution,
  RotaryAxis,
  RotaryBlank,
  RotaryZZero,
  Units,
  XyOrigin,
  ZZero,
} from '../../bridge/types';
import { designScale, isIdentityScale, type DesignScale } from '../../lib/designScale';
import { normaliseRotary, rotaryOf, unwrappedSize } from '../../lib/rotary';
import { doubleSidedOf } from '../../lib/sides';
import { strings } from '../../lib/formStrings';
import { formatDisplay, formatLength, parseLength, toMillimetres } from '../../lib/units';
import { datumOffset, jobBounds } from '../../views/View2D/camera';

export interface JobSetupFormState {
  units: Units;
  width: string;
  height: string;
  thickness: string;
  zZero: ZZero;
  xyOrigin: XyOrigin;
  /** Use Offset: the datum sits `offsetX`, `offsetY` from X0, Y0. */
  useOffset: boolean;
  offsetX: string;
  offsetY: string;
  kind: JobKind;
  rotaryDiameter: string;
  rotaryAxis: RotaryAxis;
  /** Blank length along the rotary axis. */
  rotaryLength: string;
  rotaryBlank: RotaryBlank;
  /** Z zero of a rotary job: on the axis or the blank surface. */
  rotaryZZero: RotaryZZero;
  /** How a double sided job's material is turned over. */
  flip: FlipAxis;
  /** Z zero while a double sided job's bottom side is up. */
  bottomZZero: ZZero;
  /** Rapid heights and home, edited by Material Setup and carried through here. */
  machine: MachineSetup;
  /** How finely the 3D model is calculated. */
  resolution: ModelResolution;
  /** The material look in the 3D view. */
  material: Material3d;
  /** Colour of a solid material, `#rrggbb`. */
  solidColour: string;
  /** The material texture picture laid over it, by id; 0 for none. */
  texture: number;
  /** Stretch the vectors and components with the new job size. */
  scaleDesign: boolean;
  /** Rotary: turn the design with the material when the cylinder axis changes. */
  flipDesign: boolean;
  /**
   * The exact millimetres behind a length field's text while the text is
   * still the one shown for them, so rounding to the display unit never
   * changes a size nobody edited (OK, or switching units there and back).
   */
  exact?: Partial<Record<JobSetupField, ExactLength>>;
}

interface ExactLength {
  text: string;
  mm: number;
}

export type JobSetupField =
  'width' | 'height' | 'thickness' | 'offsetX' | 'offsetY' | 'rotaryDiameter' | 'rotaryLength';

export type JobSetupErrors = Partial<Record<JobSetupField, string>>;

const LENGTH_FIELDS: readonly JobSetupField[] = [
  'width',
  'height',
  'thickness',
  'offsetX',
  'offsetY',
  'rotaryDiameter',
  'rotaryLength',
];

const POSITIVE_FIELDS: readonly JobSetupField[] = ['width', 'height', 'thickness'];

/** Fills the form from a job. Lengths arrive in millimetres. */
export function stateFromJob(job: JobSetup): JobSetupFormState {
  return {
    units: job.units,
    width: formatLength(job.width, job.units),
    height: formatLength(job.height, job.units),
    thickness: formatLength(job.thickness, job.units),
    zZero: job.zZero,
    xyOrigin: job.xyOrigin,
    useOffset: job.offsetX !== 0 || job.offsetY !== 0,
    offsetX: formatLength(job.offsetX, job.units),
    offsetY: formatLength(job.offsetY, job.units),
    kind: job.kind,
    rotaryDiameter: formatLength(job.rotary.diameter, job.units),
    rotaryAxis: job.rotary.axis,
    rotaryLength: formatLength(rotaryOf(job).length, job.units),
    rotaryBlank: rotaryOf(job).blank,
    rotaryZZero: rotaryOf(job).zZero,
    flip: doubleSidedOf(job).flip,
    bottomZZero: doubleSidedOf(job).bottomZZero,
    machine: job.machine,
    resolution: job.resolution ?? 'standard',
    material: job.appearance?.material ?? 'theme',
    solidColour: job.appearance?.colour ?? '#c9b28f',
    texture: job.appearance?.texture ?? 0,
    scaleDesign: false,
    flipDesign: true,
    exact: exactLengths(job),
  };
}

function exactLengths(job: JobSetup): Partial<Record<JobSetupField, ExactLength>> {
  const lengths: Record<JobSetupField, number> = {
    width: job.width,
    height: job.height,
    thickness: job.thickness,
    offsetX: job.offsetX,
    offsetY: job.offsetY,
    rotaryDiameter: job.rotary.diameter,
    rotaryLength: rotaryOf(job).length,
  };
  const exact: Partial<Record<JobSetupField, ExactLength>> = {};
  for (const field of LENGTH_FIELDS) {
    exact[field] = { text: formatLength(lengths[field], job.units), mm: lengths[field] };
  }
  return exact;
}

/** What a length field holds in millimetres, or null when its text does not read. */
function fieldMillimetres(state: JobSetupFormState, field: JobSetupField): number | null {
  const exact = state.exact?.[field];
  if (exact !== undefined && exact.text === state[field]) {
    return exact.mm;
  }
  const typed = parseLength(state[field]);
  return typed === null ? null : toMillimetres(typed, state.units);
}

/**
 * Whether applying the form turns the design with the material: a rotary
 * job staying rotary whose cylinder moves to the other machine axis, with
 * Flip Design on.
 */
export function flipsDesign(state: JobSetupFormState, current: JobSetup): boolean {
  return (
    state.flipDesign &&
    state.kind === 'rotary' &&
    current.kind === 'rotary' &&
    state.rotaryAxis !== current.rotary.axis
  );
}

/** Switches the display unit, converting what the fields already hold. */
export function switchUnits(state: JobSetupFormState, units: Units): JobSetupFormState {
  if (units === state.units) {
    return state;
  }
  const exact: Partial<Record<JobSetupField, ExactLength>> = {};
  const converted: JobSetupFormState = { ...state, units, exact };
  for (const field of LENGTH_FIELDS) {
    const millimetres = fieldMillimetres(state, field);
    if (millimetres === null) {
      continue;
    }
    converted[field] = formatLength(millimetres, units);
    exact[field] = { text: converted[field], mm: millimetres };
  }
  return converted;
}

/** Errors keyed by field. An empty object means the form can be applied. */
export function validate(state: JobSetupFormState): JobSetupErrors {
  const errors: JobSetupErrors = {};
  // A rotary job's size comes from its blank, so the flat size is not asked.
  const offsets: JobSetupField[] = state.useOffset ? ['offsetX', 'offsetY'] : [];
  const fields: JobSetupField[] =
    state.kind === 'rotary'
      ? ['rotaryLength', 'rotaryDiameter', ...offsets]
      : ['width', 'height', 'thickness', ...offsets];

  for (const field of fields) {
    const value = parseLength(state[field]);
    if (value === null) {
      errors[field] = strings.jobSetup.errorNumber;
      continue;
    }
    const mustBePositive =
      POSITIVE_FIELDS.includes(field) || field === 'rotaryDiameter' || field === 'rotaryLength';
    if (mustBePositive && value <= 0) {
      errors[field] = strings.jobSetup.errorPositive;
    }
  }
  return errors;
}

/** Builds the kernel job, or null when the form is not valid. */
export function jobFromState(state: JobSetupFormState): JobSetup | null {
  if (Object.keys(validate(state)).length > 0) {
    return null;
  }
  const millimetres = (field: JobSetupField): number => fieldMillimetres(state, field) ?? 0;
  return normaliseRotary({
    units: state.units,
    width: millimetres('width'),
    height: millimetres('height'),
    thickness: millimetres('thickness'),
    zZero: state.zZero,
    xyOrigin: state.xyOrigin,
    offsetX: state.useOffset ? millimetres('offsetX') : 0,
    offsetY: state.useOffset ? millimetres('offsetY') : 0,
    kind: state.kind,
    rotary: {
      diameter: millimetres('rotaryDiameter'),
      axis: state.rotaryAxis,
      length: millimetres('rotaryLength'),
      blank: state.rotaryBlank,
      zZero: state.rotaryZZero,
    },
    machine: state.machine,
    doubleSided: { flip: state.flip, bottomZZero: state.bottomZZero },
    appearance: {
      material: state.material,
      colour: state.solidColour,
      ...(state.texture === 0 ? {} : { texture: state.texture }),
    },
    resolution: state.resolution,
  });
}

/**
 * The unwrapped layout a rotary blank gives, as text in the form's unit
 * (width by height), or null while the blank fields do not read.
 */
export function unwrappedLayout(
  state: JobSetupFormState,
): { width: string; height: string } | null {
  const diameter = parseLength(state.rotaryDiameter);
  const length = parseLength(state.rotaryLength);
  if (diameter === null || length === null || diameter <= 0 || length <= 0) {
    return null;
  }
  const size = unwrappedSize({
    diameter,
    length,
    axis: state.rotaryAxis,
    blank: state.rotaryBlank,
    zZero: state.rotaryZZero,
  });
  return {
    width: formatDisplay(size.width, state.units),
    height: formatDisplay(size.height, state.units),
  };
}

/** Text shown next to a length field. */
export function unitLabel(units: Units): string {
  return units === 'inch' ? strings.units.inch : strings.units.mm;
}

/**
 * How applying the form would stretch the design of `current`, or null when
 * Scale Design is off, the form does not read, the job turns from flat to
 * rotary or back, or nothing would move. A rotary job scales with its
 * unwrapped surface: a new diameter stretches the design around the blank
 * by the new circumference over the old, so a line drawn all the way round
 * still goes all the way round.
 */
export function designScaleOf(state: JobSetupFormState, current: JobSetup): DesignScale | null {
  if (!state.scaleDesign || (state.kind === 'rotary') !== (current.kind === 'rotary')) {
    return null;
  }
  const next = jobFromState(state);
  if (next === null) {
    return null;
  }
  const scale = designScale(current, next);
  return scale === null || isIdentityScale(scale) ? null : scale;
}

/** The design scale as percentages for the note under the check box. */
export function scaleSummary(scale: DesignScale): string {
  const percent = (factor: number) => formatDisplay(factor * 100, 'mm');
  return strings.jobSetup.scaleValue(percent(scale.scaleX), percent(scale.scaleY));
}

/** Where the datum the form asks for lies, in the current job space: the
 * chosen corner (with its offset) of the material as it is now. Null while
 * a field does not read. */
export function datumMarker(
  current: JobSetup,
  state: JobSetupFormState,
): { x: number; y: number } | null {
  const next = jobFromState(state);
  if (next === null) {
    return null;
  }
  const box = jobBounds(current);
  const datum = datumOffset({ ...next, width: current.width, height: current.height });
  return { x: box.minX + datum.x, y: box.minY + datum.y };
}

/**
 * True when a new job setup puts the material somewhere else in job space
 * (another size, datum or offset), so the 2D view frames it again.
 */
export function materialMoved(before: JobSetup, after: JobSetup): boolean {
  const a = jobBounds(before);
  const b = jobBounds(after);
  return a.minX !== b.minX || a.minY !== b.minY || a.maxX !== b.maxX || a.maxY !== b.maxY;
}

/**
 * The job look after the Material Appearance dialog's OK: a different
 * material takes the texture picture away, the same one keeps it.
 */
export function withMaterial(
  state: JobSetupFormState,
  material: Material3d,
  solidColour: string,
): JobSetupFormState {
  return {
    ...state,
    material,
    solidColour,
    texture: material === state.material ? state.texture : 0,
  };
}
