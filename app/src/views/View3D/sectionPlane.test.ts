import { describe, expect, it } from 'vitest';

import type { PieceRow } from '../../bridge/assembly';
import { keptBySection, sectionPlane, sectionRange } from './sectionPlane';

/** Whether a job point stays under a scene plane. */
function kept(plane: ReturnType<typeof sectionPlane>, job: [number, number, number]): boolean {
  const scene = [job[0], job[2], -job[1]];
  const d =
    plane.normal[0] * (scene[0] ?? 0) +
    plane.normal[1] * (scene[1] ?? 0) +
    plane.normal[2] * (scene[2] ?? 0) +
    plane.constant;
  return d >= 0;
}

describe('sectionPlane', () => {
  it('keeps the side below the offset along each job axis, or above it flipped', () => {
    for (const [axis, k] of [
      ['x', 0],
      ['y', 1],
      ['z', 2],
    ] as const) {
      const below: [number, number, number] = [0, 0, 0];
      const above: [number, number, number] = [0, 0, 0];
      below[k] = 4;
      above[k] = 6;
      const plane = sectionPlane(axis, 5, false);
      expect(kept(plane, below)).toBe(true);
      expect(kept(plane, above)).toBe(false);
      const flipped = sectionPlane(axis, 5, true);
      expect(kept(flipped, below)).toBe(false);
      expect(kept(flipped, above)).toBe(true);
    }
  });

  it('spans the pieces along the axis', () => {
    const piece = {
      position: [50, 20, 5],
      rotation: [0, 0, 0],
      size: [100, 40, 10],
    } as Pick<PieceRow, 'position' | 'rotation' | 'size'>;
    expect(sectionRange([piece], 'x')).toEqual([0, 100]);
    expect(sectionRange([piece], 'z')).toEqual([0, 10]);
    expect(sectionRange([], 'y')).toBeNull();
  });
});

describe('keptBySection', () => {
  it('lets a click pick only material the section leaves', () => {
    // Keep job z <= 5: scene y <= 5.
    const plane = sectionPlane('z', 5, false);
    expect(keptBySection(plane, [0, 4, 0])).toBe(true);
    expect(keptBySection(plane, [0, 5, 0])).toBe(true);
    expect(keptBySection(plane, [0, 6, 0])).toBe(false);
    expect(keptBySection(sectionPlane('z', 5, true), [0, 6, 0])).toBe(true);
    expect(keptBySection(null, [0, 600, 0])).toBe(true);
  });
});
