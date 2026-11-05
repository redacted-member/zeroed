import { describe, expect, it } from 'vitest';

import {
  INITIAL_FREEHAND,
  freehandRequest,
  smoothnessError,
  smoothnessValue,
  stabilisationValue,
  trailPreview,
} from './model';

const stroke = [
  { x: 0, y: 0 },
  { x: 5, y: 1 },
  { x: 10, y: 0 },
];

describe('Freehand form model', () => {
  it('checks the smoothness range', () => {
    expect(smoothnessError(INITIAL_FREEHAND)).toBeUndefined();
    expect(smoothnessError({ ...INITIAL_FREEHAND, smoothness: '11' })).toBeDefined();
    expect(smoothnessError({ ...INITIAL_FREEHAND, smoothness: '-1' })).toBeDefined();
    expect(smoothnessValue({ ...INITIAL_FREEHAND, smoothness: 'abc' })).toBe(5);
    expect(smoothnessValue({ ...INITIAL_FREEHAND, smoothness: '2.5' })).toBe(2.5);
  });

  it('builds the request with a zoom sized tolerance', () => {
    const request = freehandRequest(stroke, { ...INITIAL_FREEHAND, smoothness: '4' }, 10, 8);
    expect(request).toEqual({
      points: stroke,
      smoothing: 8,
      tolerance: 0.15,
      closed: false,
    });
  });

  it('closes a stroke that ends on its start', () => {
    const loop = [...stroke, { x: 10, y: 10 }, { x: 0, y: 10 }, { x: 0.1, y: 0.1 }];
    expect(freehandRequest(loop, INITIAL_FREEHAND, 10, 8)?.closed).toBe(true);
  });

  it('closes when the pointer lets go on the start behind a lagging brush', () => {
    const loop = [...stroke, { x: 10, y: 10 }, { x: 0, y: 10 }, { x: 0, y: 1.5 }];
    expect(freehandRequest(loop, INITIAL_FREEHAND, 10, 8)?.closed).toBe(false);
    expect(freehandRequest(loop, INITIAL_FREEHAND, 10, 8, false, { x: 0.1, y: 0 })?.closed).toBe(
      true,
    );
    expect(freehandRequest(loop, INITIAL_FREEHAND, 10, 8, false, { x: 0, y: 5 })?.closed).toBe(
      false,
    );
  });

  it('ignores a click or a tiny wobble', () => {
    expect(freehandRequest([{ x: 0, y: 0 }], INITIAL_FREEHAND, 10, 8)).toBeNull();
    const wobble = [
      { x: 0, y: 0 },
      { x: 0.1, y: 0 },
    ];
    expect(freehandRequest(wobble, INITIAL_FREEHAND, 10, 8)).toBeNull();
    expect(freehandRequest(stroke, INITIAL_FREEHAND, 0, 8)).toBeNull();
  });

  it('previews the stroke as drawn', () => {
    expect(trailPreview([])).toEqual([]);
    const preview = trailPreview(stroke);
    expect(preview[0]?.subpaths[0]?.segments).toHaveLength(2);
  });

  it('closes on Tab and reads the stabilisation', () => {
    expect(freehandRequest(stroke, INITIAL_FREEHAND, 10, 8, true)?.closed).toBe(true);
    expect(stabilisationValue({ ...INITIAL_FREEHAND, stabilisation: '12' })).toBe(10);
  });
});
