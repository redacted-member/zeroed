// Pure state for the Orientate Model form: face on top, turns, size with a
// locked ratio, the size units and the perspective slider.

import type { MeshOrientation, TopFace, Vec3 } from '../../bridge/meshImport';
import type { Units } from '../../bridge/types';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';

export type Axis = 0 | 1 | 2;

export interface OrientateState {
  top: TopFace;
  rotX: string;
  rotY: string;
  rotZ: string;
  /** Size along X, Y, Z in `units`. */
  size: [string, string, string];
  units: Units;
  lockRatio: boolean;
  perspectiveOn: boolean;
  /** 0 to 100. */
  perspective: number;
}

const INCH = 25.4;

function angleText(degrees: number): string {
  return String(Math.round(degrees * 1000) / 1000);
}

/** Form fields for the store's orientation and the model size in mm. */
export function orientateState(
  orient: MeshOrientation,
  sizeMm: Vec3 | null,
  units: Units,
): OrientateState {
  const size = sizeMm ?? [0, 0, 0];
  return {
    top: orient.top,
    rotX: angleText(orient.rotX),
    rotY: angleText(orient.rotY),
    rotZ: angleText(orient.rotZ),
    size: [lengthText(size[0], units), lengthText(size[1], units), lengthText(size[2], units)],
    units,
    lockRatio: true,
    perspectiveOn: orient.perspective > 0,
    perspective: Math.round(orient.perspective * 100),
  };
}

/** The turn fields for turns made elsewhere (the rings in the 3D view). */
export function withTurns(
  state: OrientateState,
  turns: { rotX: number; rotY: number; rotZ: number },
): OrientateState {
  return {
    ...state,
    rotX: angleText(turns.rotX),
    rotY: angleText(turns.rotY),
    rotZ: angleText(turns.rotZ),
  };
}

/** The size fields for a size in mm, keeping everything else. */
export function withSize(state: OrientateState, sizeMm: Vec3): OrientateState {
  const t = (mm: number) => lengthText(mm, state.units);
  return { ...state, size: [t(sizeMm[0]), t(sizeMm[1]), t(sizeMm[2])] };
}

/** The size in mm, or null while a field is not a positive number. */
export function sizeValue(state: OrientateState): Vec3 | null {
  const values = state.size.map((text) => lengthValue(text, state.units));
  const [x, y, z] = values;
  if (
    x === null ||
    x === undefined ||
    y === null ||
    y === undefined ||
    z === null ||
    z === undefined
  ) {
    return null;
  }
  if (x <= 0 || y <= 0 || z < 0) {
    return null;
  }
  return [x, y, z];
}

export function sizeError(state: OrientateState, axis: Axis): string | undefined {
  const value = lengthValue(state.size[axis], state.units);
  return axis === 2 ? fieldError(value, { min: 0 }) : fieldError(value, { positive: true });
}

/**
 * Types into one size field. With the ratio locked the other two follow in
 * the proportions of `current` (the size last applied, mm).
 */
export function typeSize(
  state: OrientateState,
  axis: Axis,
  text: string,
  current: Vec3 | null,
): OrientateState {
  const size: [string, string, string] = [...state.size];
  size[axis] = text;
  const typed = lengthValue(text, state.units);
  const base = current?.[axis] ?? 0;
  if (state.lockRatio && current !== null && typed !== null && typed > 0 && base > 0) {
    const k = typed / base;
    for (const other of [0, 1, 2] as const) {
      if (other !== axis) {
        size[other] = lengthText(current[other] * k, state.units);
      }
    }
  }
  return { ...state, size };
}

/** Shows the size fields in other units; the size itself does not change. */
export function switchUnits(state: OrientateState, units: Units): OrientateState {
  const mm = sizeValue(state);
  if (mm === null || units === state.units) {
    return { ...state, units };
  }
  return withSize({ ...state, units }, mm);
}

/**
 * Scale mm/inches: a model drawn in inches but read as mm grows by 25.4
 * (`up`), one drawn in mm but read as inches shrinks by it.
 */
export function scaleByInch(state: OrientateState, up: boolean): OrientateState {
  const mm = sizeValue(state);
  if (mm === null) {
    return state;
  }
  const k = up ? INCH : 1 / INCH;
  return withSize(state, [mm[0] * k, mm[1] * k, mm[2] * k]);
}

/** An angle in degrees brought into -180 to 180. */
export function normalAngle(degrees: number): number {
  const wrapped = ((((degrees + 180) % 360) + 360) % 360) - 180;
  return wrapped === -180 ? 180 : wrapped;
}

/** The Z turn after a quick rotate button adds `delta` degrees. */
export function turnZ(state: OrientateState, delta: number): OrientateState {
  const now = numberValue(state.rotZ) ?? 0;
  return { ...state, rotZ: angleText(normalAngle(now + delta)) };
}

export function angleError(text: string): string | undefined {
  return fieldError(numberValue(text));
}

/** The orientation settings from the fields, or null while an angle is bad. */
export function orientationPatch(state: OrientateState): Partial<MeshOrientation> | null {
  const [x, y, z] = [numberValue(state.rotX), numberValue(state.rotY), numberValue(state.rotZ)];
  if (x === null || y === null || z === null) {
    return null;
  }
  return {
    top: state.top,
    rotX: x,
    rotY: y,
    rotZ: z,
    perspective: state.perspectiveOn ? state.perspective / 100 : 0,
  };
}
