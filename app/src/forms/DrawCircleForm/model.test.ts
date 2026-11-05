import { describe, expect, it } from 'vitest';

import { parseQuickEntry } from '../../lib/quickKeys';

import { bboxOfFlat, flattenPath } from '../../lib/geometry';
import {
  circleFromDrag,
  circlePathFromState,
  circleRadius,
  initialCircleState,
  switchSizeBy,
  validateCircle,
  circleFromQuick,
} from './model';

describe('circle form model', () => {
  it('starts with a valid 25 mm radius circle', () => {
    const state = initialCircleState('mm');
    expect(validateCircle(state, 'mm')).toEqual({});
    const bbox = bboxOfFlat(
      flattenPath(circlePathFromState(state, 'mm') ?? { subpaths: [] }, 0.01),
    );
    expect(bbox.map((v) => Math.round(v * 10) / 10)).toEqual([-25, -25, 25, 25]);
  });

  it('treats the size as a diameter when asked', () => {
    const state = switchSizeBy(initialCircleState('mm'), 'mm', 'diameter');
    expect(state.size).toBe('50');
    expect(circleRadius(state, 'mm')).toBe(25);
    expect(switchSizeBy(state, 'mm', 'radius').size).toBe('25');
  });

  it('rejects a zero size and bad numbers', () => {
    const base = initialCircleState('mm');
    expect(validateCircle({ ...base, size: '0' }, 'mm').size).toBeDefined();
    expect(validateCircle({ ...base, centreX: 'x' }, 'mm').centreX).toBeDefined();
  });

  it('fills the fields from a centre to edge drag', () => {
    const state = circleFromDrag(
      initialCircleState('mm'),
      'mm',
      { x: 10, y: 10 },
      { x: 13, y: 14 },
    );
    expect(state.centreX).toBe('10');
    expect(state.centreY).toBe('10');
    expect(state.size).toBe('5');
    const diameter = circleFromDrag(
      switchSizeBy(state, 'mm', 'diameter'),
      'mm',
      { x: 0, y: 0 },
      { x: 3, y: 4 },
    );
    expect(diameter.size).toBe('10');
  });
});

describe('circle quick keys', () => {
  it('sets the radius or the diameter typed while dragging', () => {
    const r = circleFromQuick(
      initialCircleState('mm'),
      'mm',
      { x: 5, y: 5 },
      parseQuickEntry('12') ?? { values: [], tags: {} },
    );
    expect(r).toMatchObject({ centreX: '5', centreY: '5', size: '12' });
    const d = circleFromQuick(
      initialCircleState('mm'),
      'mm',
      { x: 0, y: 0 },
      parseQuickEntry('30d') ?? { values: [], tags: {} },
    );
    expect(d?.size).toBe('15');
    expect(
      circleFromQuick(initialCircleState('mm'), 'mm', { x: 0, y: 0 }, { values: [1, 2], tags: {} }),
    ).toBeNull();
  });

  it('draws across the diameter with Alt', () => {
    const next = circleFromDrag(
      initialCircleState('mm'),
      'mm',
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      true,
    );
    expect([next.centreX, next.size]).toEqual(['5', '5']);
  });
});
