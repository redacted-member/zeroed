import { describe, expect, it } from 'vitest';

import { parseQuickEntry } from '../../lib/quickKeys';

import { bboxOfFlat, flattenPath } from '../../lib/geometry';
import {
  convertEllipseUnits,
  ellipseFromDrag,
  ellipseFromQuick,
  ellipseFromShape,
  ellipsePathFromState,
  ellipsePlacedOn,
  initialEllipseState,
  validateEllipse,
} from './model';

const coarse = (v: number) => Math.round(v * 100) / 100;

describe('ellipse form model', () => {
  it('starts valid, centred at the origin', () => {
    const state = initialEllipseState('mm');
    expect(validateEllipse(state, 'mm')).toEqual({});
    const bbox = bboxOfFlat(
      flattenPath(ellipsePathFromState(state, 'mm') ?? { subpaths: [] }, 0.001),
    );
    expect(bbox.map(coarse)).toEqual([-30, -20, 30, 20]);
  });

  it('honours the anchor', () => {
    const state = { ...initialEllipseState('mm'), anchor: 'bottomLeft' as const, x: '10', y: '5' };
    const bbox = bboxOfFlat(
      flattenPath(ellipsePathFromState(state, 'mm') ?? { subpaths: [] }, 0.001),
    );
    expect(bbox.map(coarse)).toEqual([10, 5, 70, 45]);
  });

  it('rejects zero sizes', () => {
    expect(
      validateEllipse({ ...initialEllipseState('mm'), height: '0' }, 'mm').height,
    ).toBeDefined();
  });

  it('fills from a drag and keeps a circle with shift', () => {
    const state = ellipseFromDrag(
      initialEllipseState('mm'),
      'mm',
      { x: 0, y: 0 },
      { x: 20, y: 10 },
      false,
    );
    expect(state.width).toBe('20');
    expect(state.height).toBe('10');
    expect(state.x).toBe('10');
    expect(state.y).toBe('5');
    const round = ellipseFromDrag(
      initialEllipseState('mm'),
      'mm',
      { x: 0, y: 0 },
      { x: 20, y: 10 },
      true,
    );
    expect(round.height).toBe('20');
  });

  it('converts units', () => {
    expect(convertEllipseUnits(initialEllipseState('mm'), 'mm', 'inch').width).toBe('2.3622');
  });
});

describe('ellipse quick keys', () => {
  it('sets width and height towards the cursor', () => {
    const state = { ...initialEllipseState('mm'), anchor: 'bottomLeft' as const };
    const next = ellipseFromQuick(
      state,
      'mm',
      { x: 0, y: 0 },
      { x: 5, y: 5 },
      parseQuickEntry('40,20') ?? { values: [], tags: {} },
    );
    expect(next).toMatchObject({ x: '0', y: '0', width: '40', height: '20' });
  });

  it('shows an existing ellipse and moves the anchor onto others', () => {
    const state = { ...initialEllipseState('mm'), anchor: 'bottomLeft' as const };
    const shown = ellipseFromShape(state, 'mm', { box: [10, 20, 50, 40] });
    expect([shown.x, shown.y, shown.width, shown.height]).toEqual(['10', '20', '40', '20']);
    const moved = ellipsePlacedOn(shown, 'mm', { box: [0, 0, 5, 5] });
    expect([moved.x, moved.width]).toEqual(['0', '40']);
  });
});
