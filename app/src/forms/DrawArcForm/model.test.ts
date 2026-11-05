import { describe, expect, it } from 'vitest';

import { bboxOfFlat, flattenPath } from '../../lib/geometry';
import {
  arcBetween,
  arcFromPoints,
  arcPathFromState,
  arcPreview,
  arcRadiusOf,
  fillFromArc,
  initialArcState,
  placePoint,
  validateArc,
  withSizeBy,
} from './model';

const coarse = (v: number) => Math.round(v * 100) / 100;

describe('arc form model', () => {
  it('starts with a valid arc from its exact size', () => {
    const state = initialArcState('mm');
    expect(validateArc(state, 'mm')).toEqual({});
    const path = arcPathFromState(state, 'mm');
    expect(path?.subpaths[0]?.segments[0]?.kind).toBe('arc');
    const bbox = bboxOfFlat(flattenPath(path ?? { subpaths: [] }, 0.01));
    expect(bbox[0]).toBeCloseTo(0, 1);
    expect(bbox[2]).toBeCloseTo(50, 1);
    // A height of -15 rises 15 above the chord.
    expect(Math.max(Math.abs(bbox[1]), Math.abs(bbox[3]))).toBeCloseTo(15, 1);
  });

  it('builds arcs from a radius, on either side', () => {
    const up = arcBetween({ x: 0, y: 0 }, { x: 20, y: 0 }, 'radius', 10);
    expect(coarse(arcRadiusOf(up) ?? 0)).toBe(10);
    const down = arcBetween({ x: 0, y: 0 }, { x: 20, y: 0 }, 'radius', -10);
    const b1 = up?.subpaths[0]?.segments[0];
    const b2 = down?.subpaths[0]?.segments[0];
    expect(b1?.kind === 'arc' && b2?.kind === 'arc' ? b1.bulge === -b2.bulge : false).toBe(true);
    expect(arcBetween({ x: 0, y: 0 }, { x: 20, y: 0 }, 'radius', 5)).toBeNull();
    expect(validateArc({ ...initialArcState('mm'), sizeBy: 'radius', size: '5' }, 'mm').size).toBe(
      'The radius must be at least half the distance between the points.',
    );
  });

  it('switches between radius and height keeping the arc', () => {
    const state = { ...initialArcState('mm'), sizeBy: 'radius' as const, size: '25' };
    const height = withSizeBy(state, 'mm', 'height');
    expect(coarse(Number(height.size))).toBe(25);
    expect(withSizeBy(height, 'mm', 'radius').size).toBe('25');
  });

  it('builds a quarter arc in centre mode and fills the fields from clicks', () => {
    const path = arcFromPoints('centreStartEnd', { x: 0, y: 0 }, { x: 10, y: 0 }, { x: 0, y: 10 });
    expect(coarse(arcRadiusOf(path) ?? 0)).toBe(10);
    const filled = fillFromArc(
      { ...initialArcState('mm'), sizeBy: 'radius' },
      'mm',
      path ?? { subpaths: [] },
    );
    expect([filled.startX, filled.endY, filled.size]).toEqual(['10', '10', '10']);
  });

  it('returns null for degenerate input', () => {
    expect(arcFromPoints('threePoints', { x: 0, y: 0 }, { x: 2, y: 2 }, { x: 1, y: 1 })).toBeNull();
    expect(
      arcFromPoints('centreStartEnd', { x: 0, y: 0 }, { x: 0, y: 0 }, { x: 1, y: 1 }),
    ).toBeNull();
  });

  it('previews clicks with the cursor', () => {
    let state = placePoint(initialArcState('mm'), { x: 1, y: 2 });
    expect(arcPreview(state, { x: 5, y: 5 })[0]?.subpaths[0]?.segments[0]?.kind).toBe('line');
    state = placePoint(state, { x: 11, y: 2 });
    expect(arcPreview(state, { x: 6, y: 6 })[0]?.subpaths[0]?.segments[0]?.kind).toBe('arc');
    expect(arcPreview(state, null)).toEqual([]);
  });
});
