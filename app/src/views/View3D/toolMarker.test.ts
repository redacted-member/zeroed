import { Vector3 } from 'three';
import { describe, expect, it } from 'vitest';

import { wrapPoint } from '../../lib/rotary';
import { blankTurnMatrix } from './blankTurn';
import { toolScenePosition, toolShape, wrappedToolPose } from './toolMarker';

/** A tool outline as the kernel sends it: 33 samples from the tip up. */
function outline(radiusAt: (h: number) => number, top: number): Array<[number, number]> {
  return Array.from({ length: 33 }, (_, k) => {
    const h = (top * k) / 32;
    return [radiusAt(h), h];
  });
}

describe('tool shape', () => {
  it('turns a V-bit into a cone of its angle and diameter under a shank', () => {
    // 60 degrees included, 12 mm across.
    const tan = Math.tan(Math.PI / 6);
    const shape = toolShape(outline((h) => Math.min(h * tan, 6), 24));
    expect(shape).not.toBeNull();
    const cutter = shape?.cutter ?? [];
    expect(cutter[0]).toEqual([0, 0]);
    // Halfway up the cone the radius follows the angle.
    const mid = cutter.find(([, h]) => h === 6);
    expect(mid?.[0]).toBeCloseTo(6 * tan);
    expect(shape?.radius).toBeCloseTo(6);
    // The shank stands above the cutter, narrower.
    const shank = shape?.shank ?? [];
    expect(shank[0]?.[1]).toBeCloseTo(24);
    expect(shank[1]?.[0]).toBeLessThan(6);
    expect(shape?.height).toBeGreaterThan(24);
  });

  it('closes a flat bottomed end mill across the tip', () => {
    const shape = toolShape(outline(() => 3, 12));
    expect(shape?.cutter[0]).toEqual([0, 0]);
    expect(shape?.cutter[1]).toEqual([3, 0]);
    // Repeated samples down the straight side are kept only where they differ.
    expect(shape?.cutter.every(([r]) => r === 0 || r === 3)).toBe(true);
  });

  it('rounds a ball nose from a point at the tip', () => {
    const shape = toolShape(outline((h) => (h >= 2 ? 2 : Math.sqrt(h * (4 - h))), 8));
    const cutter = shape?.cutter ?? [];
    expect(cutter[0]).toEqual([0, 0]);
    const quarter = cutter.find(([, h]) => h === 0.25);
    expect(quarter?.[0]).toBeCloseTo(Math.sqrt(0.25 * 3.75));
  });

  it('draws the shank the checks use and the holder when it is checked', () => {
    const shape = toolShape(
      outline(() => 3, 20),
      {
        shank: [4, 20, 30],
        holder: [15, 30, 70],
      },
    );
    expect(shape?.shank).toEqual([
      [0, 20],
      [4, 20],
      [4, 30],
      [0, 30],
    ]);
    expect(shape?.holder?.[1]).toEqual([15, 30]);
    expect(shape?.height).toBe(70);
    // Without them, a stand-in shank and no holder.
    const plain = toolShape(outline(() => 3, 20));
    expect(plain?.holder).toBeNull();
    expect(plain?.shank[1]?.[0]).toBeLessThan(3);
  });

  it('has no shape without a size', () => {
    expect(toolShape([])).toBeNull();
    expect(toolShape([[0, 0]])).toBeNull();
  });

  it('stands on the tip in scene axes', () => {
    expect(toolScenePosition({ x: 10, y: 20, z: -5 }, 0)).toEqual([10, -5, -20]);
    expect(toolScenePosition({ x: 1, y: 2, z: 20 }, 25)).toEqual([1, -5, -2]);
  });
});

describe('wrappedToolPose', () => {
  const quarter = (Math.PI * 40) / 4;
  const close = (a: readonly number[], b: readonly number[]) => {
    a.forEach((value, i) => expect(value).toBeCloseTo(b[i] ?? 0, 6));
  };

  it('stands over the top of the blank whatever the angle, moving in X and height', () => {
    const rotary = { axis: 'x', diameter: 40 } as const;
    close(wrappedToolPose({ x: 15, y: 0, z: 18 }, rotary, 20).at, [15, -2, 0]);
    close(wrappedToolPose({ x: 15, y: quarter, z: 18 }, rotary, 20).at, [15, -2, 0]);
    close(wrappedToolPose({ x: 15, y: 3 * quarter, z: 25 }, rotary, 20).at, [15, 5, 0]);
    // A blank along Y keeps it on the axis the other way.
    close(
      wrappedToolPose({ x: quarter, y: 15, z: 18 }, { axis: 'y', diameter: 40 }, 20).at,
      [0, -2, -15],
    );
  });

  it('turns the blank one turn for once round', () => {
    const rotary = { axis: 'x', diameter: 40 } as const;
    expect(wrappedToolPose({ x: 0, y: 0, z: 0 }, rotary, 0).turn).toBe(0);
    expect(wrappedToolPose({ x: 0, y: quarter, z: 0 }, rotary, 0).turn).toBeCloseTo(Math.PI / 2, 9);
    expect(wrappedToolPose({ x: 0, y: 4 * quarter, z: 0 }, rotary, 0).turn).toBeCloseTo(
      2 * Math.PI,
      9,
    );
  });

  it('brings the spot on the blank up under the tool, on both axes and at any depth', () => {
    for (const axis of ['x', 'y'] as const) {
      for (const distance of [0, 7, quarter, 2 * quarter + 3, 5 * quarter, -11]) {
        for (const z of [20, 17.5, 24]) {
          const radial = 20 + z - 20;
          const tip = axis === 'x' ? { x: 33, y: distance, z } : { x: distance, y: 33, z };
          const pose = wrappedToolPose(tip, { axis, diameter: 40 }, 20);
          const spot = wrapPoint(axis, 40, 33, distance, radial);
          const moved = new Vector3(spot.x, spot.y, spot.z).applyMatrix4(
            blankTurnMatrix(axis, 20, pose.turn),
          );
          close([moved.x, moved.y, moved.z], pose.at);
        }
      }
    }
  });

  it('leaves the blank where it is at no turn', () => {
    const still = new Vector3(5, -7, 3).applyMatrix4(blankTurnMatrix('x', 20, 0));
    close([still.x, still.y, still.z], [5, -7, 3]);
  });
});
