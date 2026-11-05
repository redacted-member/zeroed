import { describe, expect, it } from 'vitest';

import type { MeasureReply } from '../../bridge/assemblyFit';
import { measureLabel, measureLines } from './model';

const reply = (faceDistance: number | null, exact = true): MeasureReply => ({
  a: { point: [0, 0, 0], normal: [1, 0, 0], exact },
  b: { point: [0.2, 3, 0], normal: [-1, 0, 0], exact: true },
  distance: Math.hypot(0.2, 3),
  delta: [0.2, 3, 0],
  parallel: faceDistance !== null,
  faceDistance,
  angle: faceDistance === null ? 90 : 180,
});

describe('measure model', () => {
  it('labels parallel faces by their gap and lists the distance along each axis', () => {
    expect(measureLabel(reply(0.2), 'mm')).toBe('0.2 mm');
    expect(measureLines(reply(0.2), 'mm')).toEqual([
      'Distance 3.007 mm (X 0.2, Y 3, Z 0)',
      'Face to face 0.2 mm',
      'Snapped to exact faces',
    ]);
  });

  it('gives the angle between faces that are not parallel and says when the grid was used', () => {
    const lines = measureLines(reply(null, false), 'mm');
    expect(lines[1]).toBe('Faces at 90°');
    expect(lines[2]).toBe('Snapped to the simulation grid');
    expect(measureLabel(reply(null), 'mm')).toBe('3.007 mm');
  });
});
