import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB } from '../../bridge/types';
import { exportScene } from './exportPreview';

const triangle = new Float32Array([0, 0, 0, 10, 0, 1, 0, 20, 2]);

describe('export mesh in the 3D view', () => {
  it('lies where the relief shows, heights from the zero plane', () => {
    const scene = exportScene({ xyz: triangle, back: 'flat' }, DEFAULT_JOB, -5, false);
    expect(Array.from(scene.positions.slice(3, 6))).toEqual([10, -4, -0]);
    expect(Array.from(scene.positions.slice(6, 9))).toEqual([0, -3, -20]);
    expect(scene.colours).toHaveLength(9);
    expect(scene.wireframe).toBe(false);
    expect(scene.plane).toBeNull();
  });

  it('puts a wrapped rotary model round the blank', () => {
    const job = {
      ...DEFAULT_JOB,
      kind: 'rotary' as const,
      rotary: { ...DEFAULT_JOB.rotary, diameter: 40, axis: 'x' as const },
    };
    // The blank's top (radius 20 straight up) at scene height nought.
    const scene = exportScene(
      { xyz: new Float32Array([5, 0, 20, 5, 20, 0, 5, 0, -20]), back: 'sides' },
      job,
      0,
      true,
    );
    expect(Array.from(scene.positions)).toEqual([5, 0, 0, 5, -20, 20, 5, -40, 0]);
    expect(scene.wireframe).toBe(true);
  });
});
