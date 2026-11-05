// The preview check's colors and a laser's char on the surfaces built on
// the CPU (fields too big for the GPU surface, rotary jobs wrapped round
// their axis). Those meshes sample the field every few cells, so each
// vertex takes the most telling code of the cells it stands for (a gouge
// before an undercut before leftover, so a small gouge between two
// vertices still shows) and the darkest char among them.

import { CHECK_COLOURS } from './checkColours';
import { srgbToLinear, type LinearRgb } from './componentColours';
import { LASER_CHAR } from './surfaceShader';

/**
 * The vertex grid of a CPU built surface: the field's cell rows and
 * columns each vertex row and column samples, vertices row by row from
 * index 0. `rowsAreY` is false when the rows run along X (a rotary job
 * whose axis is X).
 */
export interface VertexGrid {
  rows: readonly number[];
  columns: readonly number[];
  rowsAreY: boolean;
}

/** Name of the per vertex attribute: the mark's linear color, then one
 * less its weight (so a mesh without the attribute, read as 0, 0, 0, 1,
 * shows no mark). */
export const MARK_ATTRIBUTE = 'carveMark';

/** How much of the mark shows over the material, as on the GPU surface. */
export const MARK_WEIGHT = 0.85;

/** Priority of each check code: gouge, then undercut, then leftover. */
const RANK: readonly number[] = [0, 3, 1, 2];

function linear(hex: string): LinearRgb {
  const byte = (at: number) => parseInt(hex.slice(at, at + 2), 16);
  return [srgbToLinear(byte(1)), srgbToLinear(byte(3)), srgbToLinear(byte(5))];
}

/** The char color, mixed in as dark as the beam burnt. */
const CHAR = linear(LASER_CHAR);

const MARKS: readonly (LinearRgb | null)[] = [
  null,
  linear(CHECK_COLOURS.gouge),
  linear(CHECK_COLOURS.leftover),
  linear(CHECK_COLOURS.undercut),
];

/** The cells from `samples[k]` up to the next sample (the last alone). */
function span(samples: readonly number[], k: number): [number, number] {
  const from = samples[k] ?? 0;
  const next = samples[k + 1];
  return [from, next === undefined ? from + 1 : Math.max(next, from + 1)];
}

/**
 * The check code each vertex of `grid` shows for a `width` by `height`
 * field's `codes`, or null when no vertex shows one (or the codes do not
 * fit the field).
 */
export function vertexCodes(
  codes: Uint8Array,
  width: number,
  height: number,
  grid: VertexGrid,
): Uint8Array | null {
  if (codes.length !== width * height) {
    return null;
  }
  const { rows, columns, rowsAreY } = grid;
  const out = new Uint8Array(rows.length * columns.length);
  let any = false;
  for (let r = 0; r < rows.length; r += 1) {
    const [r0, r1] = span(rows, r);
    for (let c = 0; c < columns.length; c += 1) {
      const [c0, c1] = span(columns, c);
      let best = 0;
      for (let a = r0; a < r1 && RANK[best] !== 3; a += 1) {
        for (let b = c0; b < c1; b += 1) {
          const [x, y] = rowsAreY ? [b, a] : [a, b];
          if (x >= width || y >= height) {
            continue;
          }
          const code = codes[y * width + x] ?? 0;
          if ((RANK[code] ?? 0) > (RANK[best] ?? 0)) {
            best = code;
          }
        }
      }
      out[r * columns.length + c] = best;
      any ||= best !== 0;
    }
  }
  return any ? out : null;
}

/**
 * How dark a laser charred each vertex of `grid` for a `width` by `height`
 * field's `burn` (0 none to 255 black a cell): the darkest of the cells it
 * stands for, or null when no vertex shows a char (or it does not fit).
 */
export function vertexBurn(
  burn: Uint8Array,
  width: number,
  height: number,
  grid: VertexGrid,
): Uint8Array | null {
  if (burn.length !== width * height) {
    return null;
  }
  const { rows, columns, rowsAreY } = grid;
  const out = new Uint8Array(rows.length * columns.length);
  let any = false;
  for (let r = 0; r < rows.length; r += 1) {
    const [r0, r1] = span(rows, r);
    for (let c = 0; c < columns.length; c += 1) {
      const [c0, c1] = span(columns, c);
      let darkest = 0;
      for (let a = r0; a < r1 && darkest < 255; a += 1) {
        for (let b = c0; b < c1; b += 1) {
          const [x, y] = rowsAreY ? [b, a] : [a, b];
          if (x < width && y < height) {
            darkest = Math.max(darkest, burn[y * width + x] ?? 0);
          }
        }
      }
      out[r * columns.length + c] = darkest;
      any ||= darkest !== 0;
    }
  }
  return any ? out : null;
}

/** The check mark, else the char, a vertex shows: a linear color and how
 * much of it covers the material. */
function markOf(
  v: number,
  codes: Uint8Array | null,
  chars: Uint8Array | null,
): [LinearRgb, number] | null {
  const mark = codes !== null && v < codes.length ? MARKS[codes[v] ?? 0] : null;
  if (mark !== null && mark !== undefined) {
    return [mark, MARK_WEIGHT];
  }
  const char = chars !== null && v < chars.length ? (chars[v] ?? 0) : 0;
  return char > 0 ? [CHAR, char / 255] : null;
}

/** The mark attribute (four values a vertex) for `count` vertices, the
 * first of them showing `codes` (the check) or else `chars` (a laser's). */
export function markAttribute(
  codes: Uint8Array | null,
  count: number,
  chars: Uint8Array | null = null,
): Float32Array {
  const out = new Float32Array(count * 4);
  for (let v = 0; v < count; v += 1) {
    const shown = markOf(v, codes, chars);
    if (shown === null) {
      out[v * 4 + 3] = 1;
    } else {
      out.set(shown[0], v * 4);
      out[v * 4 + 3] = 1 - shown[1];
    }
  }
  return out;
}

/** Vertex colors (three values a vertex) with `codes`, or else `chars`,
 * mixed over them. */
export function markedColours(
  colours: Float32Array,
  codes: Uint8Array | null,
  chars: Uint8Array | null = null,
): Float32Array {
  const out = colours.slice();
  const marked = Math.max(codes?.length ?? 0, chars?.length ?? 0);
  const count = Math.min(marked, Math.floor(out.length / 3));
  for (let v = 0; v < count; v += 1) {
    const shown = markOf(v, codes, chars);
    if (shown === null) {
      continue;
    }
    const [mark, weight] = shown;
    for (let k = 0; k < 3; k += 1) {
      const own = out[v * 3 + k] ?? 0;
      out[v * 3 + k] = own + ((mark[k] ?? 0) - own) * weight;
    }
  }
  return out;
}
