// The Assembly View's section: a cutting plane across a job axis at an
// offset, keeping the material below it (or above it, flipped), as the
// scene's clipping plane. Three-free, so it can be tested alone.

import type { PieceRow } from '../../bridge/assembly';
import type { SectionAxis } from '../../bridge/assemblyFit';
import { pieceCorners } from './pieceGizmo';

/** A plane in scene space: points with `normal . p + constant >= 0` stay. */
export interface ScenePlane {
  normal: [number, number, number];
  constant: number;
}

const INDEX: Readonly<Record<SectionAxis, 0 | 1 | 2>> = { x: 0, y: 1, z: 2 };

/**
 * The plane cutting across job `axis` at `offset` mm, keeping the side
 * below the offset, or above it when `flip`. Scene space is job (x, z, -y).
 */
export function sectionPlane(axis: SectionAxis, offset: number, flip: boolean): ScenePlane {
  const sign = flip ? -1 : 1;
  switch (axis) {
    case 'x':
      return { normal: [-sign, 0, 0], constant: sign * offset };
    case 'y':
      // Scene z is minus job y: keeping y <= c keeps scene z >= -c.
      return { normal: [0, 0, sign], constant: sign * offset };
    case 'z':
      return { normal: [0, -sign, 0], constant: sign * offset };
  }
}

/** Whether a scene point is on the side of the section that stays (always, with no section). */
export function keptBySection(
  plane: ScenePlane | null,
  point: readonly [number, number, number],
): boolean {
  if (plane === null) {
    return true;
  }
  const [a, b, c] = plane.normal;
  // A hair of slack, so a click on the cut face itself still counts.
  return a * point[0] + b * point[1] + c * point[2] + plane.constant >= -1e-6;
}

/** Where the pieces reach along a job axis: min and max, or null with none. */
export function sectionRange(
  pieces: ReadonlyArray<Pick<PieceRow, 'position' | 'rotation' | 'size'>>,
  axis: SectionAxis,
): [number, number] | null {
  const k = INDEX[axis];
  let lo = Infinity;
  let hi = -Infinity;
  for (const corner of pieces.flatMap(pieceCorners)) {
    lo = Math.min(lo, corner[k] ?? 0);
    hi = Math.max(hi, corner[k] ?? 0);
  }
  return lo <= hi ? [lo, hi] : null;
}
