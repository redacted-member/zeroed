import { describe, expect, it } from 'vitest';

import type { EntitySummary, Path } from '../../bridge/document';
import {
  arcLengthAt,
  curveOf,
  initialOnCurveState,
  offsetError,
  pickTarget,
  slideTo,
  stateFromCurve,
} from './model';

const summary = (id: number, kind: EntitySummary['kind'], onCurve?: number): EntitySummary => ({
  id,
  kind,
  layer: 1,
  bbox: [0, 0, 1, 1],
  closed: false,
  revision: 1,
  ...(onCurve === undefined ? {} : { onCurve }),
});

const line: Path = {
  subpaths: [
    { start: { x: 0, y: 0 }, closed: false, segments: [{ kind: 'line', to: { x: 100, y: 0 } }] },
  ],
};

describe('text on a curve model', () => {
  it('starts above the curve, centred, at its own size', () => {
    expect(curveOf(initialOnCurveState('mm'), 'mm', 7)).toEqual({
      pathId: 7,
      position: 'above',
      otherSide: false,
      offset: 0,
      align: 'centre',
      keepVertical: false,
      fill: false,
      spacing: 100,
      startAt: 0,
    });
  });

  it('scales the spacing the text had and refuses a negative offset', () => {
    const state = { ...initialOnCurveState('mm'), spacing: 50 };
    expect(curveOf(state, 'mm', 1, 120)?.spacing).toBe(60);
    expect(offsetError({ ...state, offset: '-1' }, 'mm')).toBeDefined();
    expect(curveOf({ ...state, offset: '-1' }, 'mm', 1)).toBeNull();
  });

  it('reopens text on a curve with the spacing slider back at 100%', () => {
    const curve = {
      pathId: 3,
      position: 'below' as const,
      otherSide: true,
      offset: 2,
      align: 'right' as const,
      keepVertical: true,
      fill: true,
      spacing: 140,
      startAt: 5,
    };
    const state = stateFromCurve(curve, 'mm');
    expect(state.spacing).toBe(100);
    expect(curveOf(state, 'mm', 3, curve.spacing)).toEqual(curve);
  });

  it('works on a text and a vector, or on text already on a curve', () => {
    const entities = [summary(1, 'text'), summary(2, 'path'), summary(3, 'text', 2)];
    expect(pickTarget([1, 2], entities)).toEqual({ textId: 1, pathId: 2 });
    expect(pickTarget([2, 1], entities)).toEqual({ textId: 1, pathId: 2 });
    expect(pickTarget([3], entities)).toEqual({ textId: 3, pathId: 2 });
    expect(pickTarget([1], entities)).toBeNull();
    expect(pickTarget([1, 3], entities)).toBeNull();
  });

  it('slides the text by the arc length dragged', () => {
    expect(arcLengthAt(line, { x: 30, y: 5 })).toBeCloseTo(30);
    expect(slideTo(line, { x: 10, y: 0 }, { x: 40, y: 2 }, 5, false)).toBeCloseTo(35);
    expect(slideTo(line, { x: 10, y: 0 }, { x: 40, y: 2 }, 5, true)).toBeCloseTo(-25);
  });
});
