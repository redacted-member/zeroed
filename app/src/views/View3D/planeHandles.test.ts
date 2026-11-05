import { describe, expect, it } from 'vitest';

import type { CutPlane } from '../../bridge/meshImport';
import { planeNormal } from './importPreview';
import { anglesOf, dragPlane, gripStart, handlePoints, planeAxes } from './planeHandles';

const level: CutPlane = { point: [10, 20, 5], angleX: 0, angleY: 0 };

function close(a: readonly number[], b: readonly number[], tol = 1e-6): boolean {
  return a.every((v, i) => Math.abs(v - (b[i] ?? 0)) < tol);
}

describe('segmenting plane handles', () => {
  it('reads the angles back from a normal', () => {
    for (const [angleX, angleY] of [
      [0, 0],
      [30, 0],
      [0, -40],
      [25, 60],
    ] as const) {
      const n = planeNormal({ point: [0, 0, 0], angleX, angleY });
      const back = anglesOf(n);
      expect(back.angleX).toBeCloseTo(angleX, 2);
      expect(back.angleY).toBeCloseTo(angleY, 2);
    }
  });

  it('puts the arrow along the normal and the knobs on the plane', () => {
    const points = handlePoints(level, 4);
    expect(close(points.move, [10, 20, 9])).toBe(true);
    const { u, v } = planeAxes(level);
    expect(Math.abs(u[2]) + Math.abs(v[2])).toBeCloseTo(0);
    expect(close(points.tiltU, [10 + 4 * u[0], 20 + 4 * u[1], 5])).toBe(true);
  });

  it('moves the plane along its normal with the pointer', () => {
    // A ray looking along +X, 3 above the plane's point.
    const start = {
      origin: [0, 20, 5] as [number, number, number],
      direction: [1, 0, 0] as [number, number, number],
    };
    const from = gripStart(level, start);
    const later = {
      origin: [0, 20, 8] as [number, number, number],
      direction: [1, 0, 0] as [number, number, number],
    };
    const moved = dragPlane(level, 'move', from, later);
    expect(moved?.point).toEqual([10, 20, 8]);
    expect(moved?.angleX).toBe(0);
  });

  it('tilts the plane so a knob follows the pointer', () => {
    const { u, v } = planeAxes(level);
    // Aim at the point one up and one out along u from the plane's point,
    // looking along v (square to the hinge plane).
    const target: [number, number, number] = [10 + u[0], 20 + u[1], 5 + 1];
    const ray = {
      origin: [target[0] - v[0] * 50, target[1] - v[1] * 50, target[2] - v[2] * 50] as [
        number,
        number,
        number,
      ],
      direction: [v[0], v[1], v[2]] as [number, number, number],
    };
    const tilted = dragPlane(level, 'tiltU', 0, ray);
    expect(tilted).not.toBeNull();
    if (tilted !== null) {
      const n = planeNormal(tilted);
      // Tilted 45 degrees: the normal leans away from u.
      expect(n[2]).toBeCloseTo(Math.SQRT1_2, 3);
      expect(n[0] * u[0] + n[1] * u[1]).toBeCloseTo(-Math.SQRT1_2, 3);
    }
  });
});
