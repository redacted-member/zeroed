// Cell heights in job millimetres as the kernel sends them: plain f32, or
// u16 codes where code q stands for `base + q * step` mm and HEIGHT_NAN for
// NaN (no material, a cell cut through). The codes are kept as they came:
// the 3D view uploads them as they are and decodes them on the GPU, and
// everything else reads a cell through `heightReader` instead of copying
// the whole grid out into floats.

/** The code of a NaN height. */
export const HEIGHT_NAN = 0xffff;
/** The highest code that stands for a height. */
export const HEIGHT_TOP_CODE = HEIGHT_NAN - 1;

/** Code `q` is `base + q * step` millimetres. */
export interface HeightCode {
  base: number;
  step: number;
}

export type Heights =
  | { readonly data: Float32Array; readonly code: null }
  | { readonly data: Uint16Array; readonly code: HeightCode };

export function floatHeights(data: Float32Array): Heights {
  return { data, code: null };
}

export function codedHeights(data: Uint16Array, code: HeightCode): Heights {
  return { data, code };
}

/** The height code `q` stands for, NaN for {@link HEIGHT_NAN}. */
export function decodeHeight(code: HeightCode, q: number): number {
  return q === HEIGHT_NAN ? Number.NaN : code.base + q * code.step;
}

/** The code nearest `mm`, clamped into the code's range; NaN is {@link HEIGHT_NAN}. */
export function encodeHeight(code: HeightCode, mm: number): number {
  if (Number.isNaN(mm)) {
    return HEIGHT_NAN;
  }
  if (!(code.step > 0)) {
    return 0;
  }
  const q = Math.round((mm - code.base) / code.step);
  return Math.min(Math.max(q, 0), HEIGHT_TOP_CODE);
}

/** Whether `mm` rounds to a code by no more than half a step (NaN always does). */
export function encodes(code: HeightCode, mm: number): boolean {
  if (Number.isNaN(mm)) {
    return true;
  }
  const half = code.step > 0 ? code.step / 2 : 0;
  const top = code.base + HEIGHT_TOP_CODE * Math.max(code.step, 0);
  const slack = 1e-6 * Math.max(1, Math.abs(code.base), Math.abs(top));
  return mm >= code.base - half - slack && mm <= top + half + slack;
}

/** Reads cell `i` in millimetres: NaN where there is no material or `i` is
 * outside the grid. */
export type HeightRead = (i: number) => number;

/** The accessor every reader of the heights goes through. */
export function heightReader(heights: Heights): HeightRead {
  if (heights.code === null) {
    const data = heights.data;
    return (i) => data[i] ?? Number.NaN;
  }
  const data = heights.data;
  const { base, step } = heights.code;
  return (i) => {
    const q = data[i];
    return q === undefined || q === HEIGHT_NAN ? Number.NaN : base + q * step;
  };
}

/** The same heights raised by `by` millimetres, sharing the codes when
 * they are coded (NaN stays NaN). */
export function shiftHeights(heights: Heights, by: number): Heights {
  if (heights.code === null) {
    const out = new Float32Array(heights.data.length);
    for (let i = 0; i < out.length; i += 1) {
      out[i] = (heights.data[i] ?? Number.NaN) + by;
    }
    return floatHeights(out);
  }
  return codedHeights(heights.data, { base: heights.code.base + by, step: heights.code.step });
}

/** Whether any cell is NaN. */
export function hasNan(heights: Heights): boolean {
  if (heights.code === null) {
    return heights.data.some(Number.isNaN);
  }
  return heights.data.includes(HEIGHT_NAN);
}

/** The heights with NaN cells replaced by `mm`: the same codes when there
 * is no NaN, a copy of them when `mm` has a code of its own, floats
 * otherwise. */
export function fillNan(heights: Heights, mm: number): Heights {
  if (!hasNan(heights)) {
    return heights;
  }
  if (heights.code !== null && encodes(heights.code, mm)) {
    const fill = encodeHeight(heights.code, mm);
    const data = heights.data.map((q) => (q === HEIGHT_NAN ? fill : q));
    return codedHeights(data, heights.code);
  }
  const read = heightReader(heights);
  const out = new Float32Array(heights.data.length);
  for (let i = 0; i < out.length; i += 1) {
    const h = read(i);
    out[i] = Number.isNaN(h) ? mm : h;
  }
  return floatHeights(out);
}

/**
 * Copies `count` cells of `from` starting at `fromStart` into `into` at
 * `intoStart`: straight copies of the codes when both share one, each
 * cell re-encoded (or decoded) otherwise.
 */
export function copyHeights(
  into: Heights,
  intoStart: number,
  from: Heights,
  fromStart: number,
  count: number,
): void {
  if (into.code === null && from.code === null) {
    into.data.set(from.data.subarray(fromStart, fromStart + count), intoStart);
    return;
  }
  if (
    into.code !== null &&
    from.code !== null &&
    into.code.base === from.code.base &&
    into.code.step === from.code.step
  ) {
    into.data.set(from.data.subarray(fromStart, fromStart + count), intoStart);
    return;
  }
  const read = heightReader(from);
  if (into.code === null) {
    for (let k = 0; k < count; k += 1) {
      into.data[intoStart + k] = read(fromStart + k);
    }
    return;
  }
  const code = into.code;
  for (let k = 0; k < count; k += 1) {
    into.data[intoStart + k] = encodeHeight(code, read(fromStart + k));
  }
}

/** What {@link floorReader} needs of a heightfield. */
export interface WithFloor {
  z: Heights;
  floor?: Heights | 'flat';
  bottom: number;
}

/** Reads the underside of cell `i`, or null for a field without one (a
 * model): a flat underside is `bottom` under every cell but those cut
 * through. */
export function floorReader(field: WithFloor): HeightRead | null {
  const { floor, bottom } = field;
  if (floor === undefined) {
    return null;
  }
  if (floor !== 'flat') {
    return heightReader(floor);
  }
  const top = heightReader(field.z);
  return (i) => (Number.isNaN(top(i)) ? Number.NaN : bottom);
}

/** The flat underside of `field` as heights in the same form as its top. */
export function solidFloor(field: WithFloor): Heights {
  const read = heightReader(field.z);
  const cells = field.z.data.length;
  if (field.z.code !== null) {
    const code = field.z.code;
    const at = encodeHeight(code, field.bottom);
    const data = new Uint16Array(cells);
    for (let i = 0; i < cells; i += 1) {
      data[i] = Number.isNaN(read(i)) ? HEIGHT_NAN : at;
    }
    return codedHeights(data, code);
  }
  const data = new Float32Array(cells);
  for (let i = 0; i < cells; i += 1) {
    data[i] = Number.isNaN(read(i)) ? Number.NaN : field.bottom;
  }
  return floatHeights(data);
}
