import { describe, expect, it } from 'vitest';

import { parseQuickEntry } from '../../lib/quickKeys';

import { pathNodes } from '../../lib/geometry';
import {
  initialStarState,
  innerRadius,
  starCentredOn,
  starFromDrag,
  starFromQuick,
  starFromShape,
  starPathFromState,
  validateStar,
} from './model';

describe('star form model', () => {
  it('starts with a valid five point star at 40 percent', () => {
    const state = initialStarState('mm');
    expect(validateStar(state, 'mm')).toEqual({});
    expect(innerRadius(state, 'mm')).toBe(10);
    expect(pathNodes(starPathFromState(state, 'mm') ?? { subpaths: [] })).toHaveLength(10);
  });

  it('rejects an inner radius at or above the outer one', () => {
    expect(validateStar({ ...initialStarState('mm'), inner: '100' }, 'mm').inner).toBeDefined();
    expect(validateStar({ ...initialStarState('mm'), points: '2' }, 'mm').points).toBeDefined();
  });
});

describe('star quick keys', () => {
  it('sets points, inner percent and diameter', () => {
    const next = starFromQuick(
      initialStarState('mm'),
      'mm',
      { x: 0, y: 0 },
      parseQuickEntry('7p30i40d') ?? { values: [], tags: {} },
    );
    expect(next).toMatchObject({
      points: '7',
      inner: '30',
      outerRadius: '20',
    });
  });

  it('shows an existing star with its inner radius as a percent', () => {
    const shape = { centre: { x: 0, y: 0 }, points: 5, outer: 10, inner: 4 };
    const byPercent = starFromShape(initialStarState('mm'), 'mm', shape);
    expect([byPercent.points, byPercent.outerRadius]).toEqual(['5', '10']);
    expect(byPercent.inner).toBe('40');
    expect(starCentredOn(byPercent, 'mm', { centre: { x: 3, y: 0 } }).centreX).toBe('3');
  });

  it('with Alt drags across the shape from its middle', () => {
    const state = initialStarState('mm');
    const plain = starFromDrag(state, 'mm', { x: 0, y: 0 }, { x: 20, y: 0 });
    expect(plain.centreX).toBe('0');
    const across = starFromDrag(state, 'mm', { x: 0, y: 0 }, { x: 20, y: 0 }, true);
    expect(across.centreX).toBe('10');
    expect(Number(across.outerRadius)).toBeLessThan(Number(plain.outerRadius));
  });
});
