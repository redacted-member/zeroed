import { describe, expect, it } from 'vitest';

import { parseQuickEntry } from '../../lib/quickKeys';

import { pathNodes } from '../../lib/geometry';
import {
  initialPolygonState,
  polygonCentredOn,
  polygonFromDrag,
  polygonFromQuick,
  polygonFromShape,
  polygonPathFromState,
  switchPolygonSizeBy,
  validatePolygon,
} from './model';

describe('polygon form model', () => {
  it('starts with a valid hexagon', () => {
    const state = initialPolygonState('mm');
    expect(validatePolygon(state, 'mm')).toEqual({});
    expect(pathNodes(polygonPathFromState(state, 'mm') ?? { subpaths: [] })).toHaveLength(6);
  });

  it('rejects too few sides, fractions and zero radius', () => {
    const base = initialPolygonState('mm');
    expect(validatePolygon({ ...base, sides: '2' }, 'mm').sides).toBeDefined();
    expect(validatePolygon({ ...base, sides: '4.5' }, 'mm').sides).toBeDefined();
    expect(validatePolygon({ ...base, sides: '101' }, 'mm').sides).toBeDefined();
    expect(validatePolygon({ ...base, radius: '0' }, 'mm').radius).toBeDefined();
  });

  it('fills the centre and radius from a drag', () => {
    const state = polygonFromDrag(initialPolygonState('mm'), 'mm', { x: 5, y: 5 }, { x: 8, y: 9 });
    expect(state.centreX).toBe('5');
    expect(state.radius).toBe('5');
  });
});

describe('polygon quick keys', () => {
  it('sets sides and radius', () => {
    const next = polygonFromQuick(
      initialPolygonState('mm'),
      'mm',
      { x: 0, y: 0 },
      parseQuickEntry('8s25r') ?? { values: [], tags: {} },
    );
    expect(next).toMatchObject({ sides: '8', radius: '25' });
  });

  it('shows an existing polygon by its corner radius', () => {
    const shape = { centre: { x: 1, y: 2 }, radius: 10, sides: 6 };
    const byVertex = polygonFromShape(initialPolygonState('mm'), 'mm', shape);
    expect([byVertex.centreX, byVertex.sides, byVertex.radius]).toEqual(['1', '6', '10']);
    expect(polygonCentredOn(byVertex, 'mm', { centre: { x: 7, y: 8 } }).centreY).toBe('8');
  });
});

describe('polygon size by diameter', () => {
  it('reads the size as a diameter and switches without changing the polygon', () => {
    const state = initialPolygonState('mm');
    const byDiameter = switchPolygonSizeBy(state, 'mm', 'diameter');
    expect(byDiameter.radius).toBe('50');
    expect(polygonPathFromState(byDiameter, 'mm')).toEqual(polygonPathFromState(state, 'mm'));
    const dragged = polygonFromDrag(byDiameter, 'mm', { x: 0, y: 0 }, { x: 10, y: 0 });
    expect(dragged.radius).toBe('20');
    expect(switchPolygonSizeBy(dragged, 'mm', 'radius').radius).toBe('10');
  });

  it('with Alt drags across the shape from its middle', () => {
    const state = initialPolygonState('mm');
    const plain = polygonFromDrag(state, 'mm', { x: 0, y: 0 }, { x: 20, y: 0 });
    expect(plain.centreX).toBe('0');
    const across = polygonFromDrag(state, 'mm', { x: 0, y: 0 }, { x: 20, y: 0 }, true);
    expect(across.centreX).toBe('10');
    expect(Number(across.radius)).toBeLessThan(Number(plain.radius));
  });
});
