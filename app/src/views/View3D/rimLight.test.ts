import { Vector3 } from 'three';
import { describe, expect, it } from 'vitest';

import { rimDirection } from './renderer';

describe('rim light', () => {
  it('shines from behind the model, opposite the camera and a little above', () => {
    const d = rimDirection(new Vector3(10, 5, 0), new Vector3(0, 0, 0));
    // Opposite the camera's side (x negative), lifted by 0.45 before normalising.
    expect(d.x).toBeCloseTo(-1 / Math.hypot(1, 0.45), 12);
    expect(d.y).toBeCloseTo(0.45 / Math.hypot(1, 0.45), 12);
    expect(d.z).toBeCloseTo(0, 12);
    expect(d.length()).toBeCloseTo(1, 12);
  });

  it('ignores the camera height and follows its side', () => {
    const low = rimDirection(new Vector3(0, 1, 7), new Vector3(0, 0, 0));
    const high = rimDirection(new Vector3(0, 300, 7), new Vector3(0, 0, 0));
    expect(low.toArray()).toEqual(high.toArray());
    expect(low.z).toBeLessThan(0);
  });

  it('comes straight down when the camera is right above the target', () => {
    expect(rimDirection(new Vector3(0, 9, 0), new Vector3(0, 0, 0)).toArray()).toEqual([0, 1, 0]);
  });
});
