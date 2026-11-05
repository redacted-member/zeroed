// What a Quick Keys entry means for each drawing and transform tool. Every
// function takes lengths already converted to millimetres through `toMm`
// and returns null when the entry does not fit the tool.

import type { Point } from '../bridge/document';
import type { QuickEntry, QuickTag } from './quickKeys';

type ToMm = (value: number) => number;

function tagCount(entry: QuickEntry): number {
  return Object.keys(entry.tags).length;
}

function only(entry: QuickEntry, allowed: readonly QuickTag[]): boolean {
  return (Object.keys(entry.tags) as QuickTag[]).every((tag) => allowed.includes(tag));
}

function positive(value: number | undefined): value is number {
  return value !== undefined && Number.isFinite(value) && value > 0;
}

export interface QuickBox {
  width: number;
  height: number;
  /** Corner radius when one was typed (rectangle only). */
  radius?: number;
}

/**
 * Rectangle and ellipse sizes: `s` a square (for the ellipse a circle of
 * diameter s), `w,h`, `wX` or `hY` keeping the other side from the drag,
 * and (rectangle) `rR` combined with any of those for the corner radius.
 */
export function quickBox(
  entry: QuickEntry,
  current: { width: number; height: number },
  toMm: ToMm,
  allowRadius: boolean,
): QuickBox | null {
  if (!only(entry, allowRadius ? ['x', 'y', 'w', 'h', 'r'] : ['x', 'y', 'w', 'h'])) {
    return null;
  }
  const { values, tags } = entry;
  // W and H are the same as X and Y: width and height.
  const tagWidth = tags.x ?? tags.w;
  const tagHeight = tags.y ?? tags.h;
  let width = current.width;
  let height = current.height;
  if (values.length === 1) {
    if (tagWidth !== undefined || tagHeight !== undefined) {
      return null;
    }
    width = toMm(values[0] ?? 0);
    height = width;
  } else if (values.length === 2) {
    if (tagWidth !== undefined || tagHeight !== undefined) {
      return null;
    }
    width = toMm(values[0] ?? 0);
    height = toMm(values[1] ?? 0);
  } else if (values.length > 2) {
    return null;
  } else {
    if (tagWidth === undefined && tagHeight === undefined) {
      return null;
    }
    if (tagWidth !== undefined) {
      width = toMm(tagWidth);
    }
    if (tagHeight !== undefined) {
      height = toMm(tagHeight);
    }
  }
  if (!positive(width) || !positive(height)) {
    return null;
  }
  const box: QuickBox = { width, height };
  if (tags.r !== undefined) {
    const radius = toMm(tags.r);
    if (radius < 0 || radius > Math.min(width, height) / 2) {
      return null;
    }
    box.radius = radius;
  }
  return box;
}

/** Corner of a box of the given size drawn from `start` towards `cursor`. */
export function boxCorner(start: Point, cursor: Point, width: number, height: number): Point {
  return {
    x: start.x + (cursor.x >= start.x ? width : -width),
    y: start.y + (cursor.y >= start.y ? height : -height),
  };
}

/** Circle: `r` or `rR` a radius, `dD` a diameter. Returns the radius. */
export function quickCircleRadius(entry: QuickEntry, toMm: ToMm): number | null {
  const { values, tags } = entry;
  let radius: number | undefined;
  if (values.length === 1 && tagCount(entry) === 0) {
    radius = toMm(values[0] ?? 0);
  } else if (values.length === 0 && tagCount(entry) === 1 && tags.d !== undefined) {
    radius = toMm(tags.d) / 2;
  } else if (values.length === 0 && tagCount(entry) === 1 && tags.r !== undefined) {
    radius = toMm(tags.r);
  }
  return positive(radius) ? radius : null;
}

export interface QuickRadial {
  radius: number;
  /** Sides (polygon) or points (star) when typed. */
  count?: number;
  /** Inner radius as a percentage of the outer one (star only). */
  innerPercent?: number;
}

/**
 * Polygon (`countTag` s) and star (`countTag` p, plus `iI` percent): a
 * value is the radius, `dD` the diameter, `nS`/`nP` the count.
 */
export function quickRadial(
  entry: QuickEntry,
  toMm: ToMm,
  countTag: 's' | 'p',
): QuickRadial | null {
  const allowed: QuickTag[] = countTag === 'p' ? ['p', 'i', 'r', 'd'] : ['s', 'r', 'd'];
  if (!only(entry, allowed)) {
    return null;
  }
  const { values, tags } = entry;
  if (values.length > 1) {
    return null;
  }
  const sizes = [values[0], tags.r, tags.d].filter((v) => v !== undefined).length;
  if (sizes !== 1) {
    return null;
  }
  const radius =
    values[0] !== undefined
      ? toMm(values[0])
      : tags.r !== undefined
        ? toMm(tags.r)
        : toMm(tags.d ?? 0) / 2;
  if (!positive(radius)) {
    return null;
  }
  const result: QuickRadial = { radius };
  const count = tags[countTag];
  if (count !== undefined) {
    if (!Number.isInteger(count) || count < 3 || count > 100) {
      return null;
    }
    result.count = count;
  }
  if (tags.i !== undefined) {
    if (tags.i <= 0 || tags.i >= 100) {
      return null;
    }
    result.innerPercent = tags.i;
  }
  return result;
}

/** Rotate: a value in degrees, counter clockwise. */
export function quickRotation(entry: QuickEntry): number | null {
  if (entry.values.length === 1 && tagCount(entry) === 0) {
    return entry.values[0] ?? null;
  }
  return null;
}

/** Which dimensions a scale handle changes. */
export type ScaleAxes = 'both' | 'x' | 'y';

/**
 * Scale by a handle: a value sets the width (side handles and corners) or
 * the height (top and bottom handles) keeping the proportions on a corner,
 * `w,h` sets both, `fS` scales by the factor f. Returns the factors.
 */
export function quickScale(
  entry: QuickEntry,
  axes: ScaleAxes,
  size: { width: number; height: number },
  toMm: ToMm,
): { sx: number; sy: number } | null {
  const { values, tags } = entry;
  if (size.width <= 0 && size.height <= 0) {
    return null;
  }
  const safe = (numerator: number, denominator: number) =>
    denominator > 1e-9 ? numerator / denominator : 1;
  if (values.length === 0 && tagCount(entry) === 1 && positive(tags.s)) {
    const f = tags.s;
    return { sx: axes === 'y' ? 1 : f, sy: axes === 'x' ? 1 : f };
  }
  if (tagCount(entry) !== 0) {
    return null;
  }
  if (values.length === 2) {
    const [w, h] = values;
    if (!positive(w) || !positive(h)) {
      return null;
    }
    return { sx: safe(toMm(w), size.width), sy: safe(toMm(h), size.height) };
  }
  if (values.length === 1 && positive(values[0])) {
    const target = toMm(values[0]);
    if (axes === 'y') {
      return { sx: 1, sy: safe(target, size.height) };
    }
    const f = safe(target, size.width);
    return { sx: f, sy: axes === 'x' ? 1 : f };
  }
  return null;
}
