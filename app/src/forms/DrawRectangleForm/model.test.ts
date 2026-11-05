import { describe, expect, it } from 'vitest';

import { bboxOfFlat, flattenPath } from '../../lib/geometry';
import { parseQuickEntry } from '../../lib/quickKeys';
import {
  anchorPoint,
  convertRectangleUnits,
  initialRectangleState,
  rectangleAt,
  rectangleFromDrag,
  rectangleFromQuick,
  rectangleFromShape,
  rectanglePath,
  rectanglePlacedOn,
  radiusFromDrag,
  radiusHandles,
  validateRectangle,
} from './model';

describe('rectangle form model', () => {
  it('starts valid with a 50 x 30 box at the origin', () => {
    const state = initialRectangleState('mm');
    expect(validateRectangle(state, 'mm')).toEqual({});
    const path = rectanglePath(state, 'mm');
    expect(path).not.toBeNull();
    expect(bboxOfFlat(flattenPath(path ?? { subpaths: [] }, 0.01))).toEqual([0, 0, 50, 30]);
  });

  it('places the box by its anchor', () => {
    const state = { ...initialRectangleState('mm'), anchor: 'centre' as const, x: '10', y: '10' };
    const bbox = bboxOfFlat(flattenPath(rectanglePath(state, 'mm') ?? { subpaths: [] }, 0.01));
    expect(bbox).toEqual([-15, -5, 35, 25]);
  });

  it('rejects zero sizes, bad numbers and oversized radii', () => {
    const base = initialRectangleState('mm');
    expect(validateRectangle({ ...base, width: '0' }, 'mm').width).toBeDefined();
    expect(validateRectangle({ ...base, height: 'abc' }, 'mm').height).toBeDefined();
    expect(
      validateRectangle({ ...base, corner: 'external', radius: '20' }, 'mm').radius,
    ).toBeDefined();
    expect(validateRectangle({ ...base, corner: 'external', radius: '15' }, 'mm')).toEqual({});
    expect(validateRectangle({ ...base, corner: 'none', radius: 'abc' }, 'mm')).toEqual({});
  });

  it('converts lengths when the unit changes', () => {
    const converted = convertRectangleUnits(initialRectangleState('mm'), 'mm', 'inch');
    expect(converted.width).toBe('1.9685');
    const back = convertRectangleUnits(converted, 'inch', 'mm');
    expect(back.width).toBe('50');
  });

  it('fills the fields from a drag, keeping the anchor', () => {
    const state = { ...initialRectangleState('mm'), anchor: 'topRight' as const };
    const dragged = rectangleFromDrag(state, 'mm', { x: 10, y: 10 }, { x: 30, y: 25 }, false);
    expect(dragged.width).toBe('20');
    expect(dragged.height).toBe('15');
    expect(dragged.x).toBe('30');
    expect(dragged.y).toBe('25');
  });

  it('squares a shift drag and handles reversed drags', () => {
    const state = initialRectangleState('mm');
    const dragged = rectangleFromDrag(state, 'mm', { x: 10, y: 10 }, { x: 0, y: 4 }, true);
    expect(dragged.width).toBe('10');
    expect(dragged.height).toBe('10');
    expect(dragged.x).toBe('0');
    expect(dragged.y).toBe('0');
  });

  it('moves the anchor to a clicked point', () => {
    const state = rectangleAt(initialRectangleState('mm'), 'mm', { x: 3.5, y: -2 });
    expect(state.x).toBe('3.5');
    expect(state.y).toBe('-2');
  });

  it('anchorPoint inverts boxOrigin', () => {
    expect(anchorPoint('centre', { x: 0, y: 0 }, 10, 4)).toEqual({ x: 5, y: 2 });
    expect(anchorPoint('bottomRight', { x: 0, y: 0 }, 10, 4)).toEqual({ x: 10, y: 0 });
  });
});

describe('rectangle quick keys', () => {
  it('makes the typed size from the drag start towards the cursor', () => {
    const state = { ...initialRectangleState('mm'), anchor: 'bottomLeft' as const };
    const entry = parseQuickEntry('4r40,20');
    expect(entry).not.toBeNull();
    const next =
      entry === null
        ? null
        : rectangleFromQuick(state, 'mm', { x: 10, y: 10 }, { x: 0, y: 30 }, entry);
    expect(next).toMatchObject({
      x: '-30',
      y: '10',
      width: '40',
      height: '20',
      corner: 'external',
      radius: '4',
    });
  });

  it('works in inches', () => {
    const entry = parseQuickEntry('2');
    const next =
      entry === null
        ? null
        : rectangleFromQuick(
            initialRectangleState('inch'),
            'inch',
            { x: 0, y: 0 },
            { x: 1, y: 1 },
            entry,
          );
    expect(next?.width).toBe('2');
  });

  it('shows an existing rectangle with its corners', () => {
    const state = { ...initialRectangleState('mm'), anchor: 'centre' as const };
    const shown = rectangleFromShape(state, 'mm', {
      box: [0, 0, 30, 20],
      corner: 'external',
      radius: 4,
    });
    expect([shown.x, shown.y, shown.width, shown.corner, shown.radius]).toEqual([
      '15',
      '10',
      '30',
      'external',
      '4',
    ]);
    expect(rectanglePlacedOn(shown, 'mm', { box: [0, 0, 2, 2] }).x).toBe('1');
  });

  it('draws out from the centre with Alt', () => {
    const state = { ...initialRectangleState('mm'), anchor: 'centre' as const };
    const next = rectangleFromDrag(state, 'mm', { x: 10, y: 10 }, { x: 15, y: 13 }, false, true);
    expect([next.x, next.y, next.width, next.height]).toEqual(['10', '10', '10', '6']);
  });
});

describe('corner radius handles', () => {
  const rounded = { ...initialRectangleState('mm'), corner: 'external' as const, radius: '5' };

  it('shows none on square corners', () => {
    expect(radiusHandles(initialRectangleState('mm'), 'mm')).toEqual([]);
  });

  it('puts four handles where the arcs meet the top and bottom edges', () => {
    const handles = radiusHandles(rounded, 'mm');
    expect(handles.map((h) => h.at)).toEqual([
      { x: 5, y: 0 },
      { x: 45, y: 0 },
      { x: 45, y: 30 },
      { x: 5, y: 30 },
    ]);
  });

  it('drags the radius along the edge, kept inside the box', () => {
    const [first, second] = radiusHandles(rounded, 'mm');
    if (first === undefined || second === undefined) {
      throw new Error('no handles');
    }
    expect(radiusFromDrag(rounded, 'mm', first, { x: 8, y: 3 }).radius).toBe('8');
    expect(radiusFromDrag(rounded, 'mm', second, { x: 40, y: -2 }).radius).toBe('10');
    expect(radiusFromDrag(rounded, 'mm', first, { x: 40, y: 0 }).radius).toBe('15');
    expect(radiusFromDrag(rounded, 'mm', first, { x: -3, y: 0 }).radius).toBe('0.01');
  });
});
