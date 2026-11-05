import { describe, expect, it } from 'vitest';

import type { DimensionSpec } from '../../bridge/document';

import {
  customTextFor,
  editedTextAt,
  dragDimensionText,
  initialDimensionState,
  onDimensionText,
  isComplete,
  pickCircle,
  pointCount,
  placePoint,
  placingPreview,
  specFromPoints,
  specFromState,
  stateFromDimension,
  textAtOf,
  validateDimension,
  type DimensionFormState,
} from './model';

describe('dimensions form model', () => {
  it('starts with a valid linear dimension', () => {
    const state = initialDimensionState('mm');
    expect(validateDimension(state, 'mm')).toEqual({});
    expect(specFromState(state, 'mm')).toEqual({
      kind: 'linear',
      from: { x: 0, y: 0 },
      to: { x: 50, y: 0 },
      offset: 10,
    });
  });

  it('puts a linear offset on the side of the third point', () => {
    const below = specFromPoints('linear', [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 5, y: -4 },
    ]);
    expect(below).toMatchObject({ offset: -4 });
  });

  it('takes the line position of horizontal and vertical dimensions from the third point', () => {
    const points = [
      { x: 0, y: 0 },
      { x: 30, y: 20 },
      { x: 7, y: 25 },
    ];
    expect(specFromPoints('horizontal', points)).toMatchObject({ offset: 25 });
    expect(specFromPoints('vertical', points)).toMatchObject({ offset: 7 });
    const upright = [
      { x: 0, y: 0 },
      { x: 0, y: 5 },
      { x: 7, y: 25 },
    ];
    expect(specFromPoints('horizontal', upright)).toBeNull();
  });

  it('builds an angle from the first arm, the corner and the second arm', () => {
    const points = [
      { x: 10, y: 0 },
      { x: 0, y: 0 },
      { x: 0, y: 10 },
      { x: 3, y: 4 },
      { x: 6, y: 6 },
    ];
    const spec = specFromPoints('angular', points);
    expect(spec).toMatchObject({
      kind: 'angular',
      centre: { x: 0, y: 0 },
      from: { x: 10, y: 0 },
      radius: 5,
    });
    expect(textAtOf('angular', points)).toEqual({ x: 6, y: 6 });
    expect(pointCount('angular')).toBe(5);
  });

  it('ends a length dimension with the click that places its text', () => {
    const points = [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 5, y: 4 },
      { x: 8, y: 6 },
    ];
    expect(pointCount('linear')).toBe(4);
    expect(textAtOf('linear', points)).toEqual({ x: 8, y: 6 });
    expect(textAtOf('arc', points)).toBeUndefined();
  });

  it('loads a Shift picked dimension into the form, points and all', () => {
    const state = initialDimensionState('mm');
    const arc = { kind: 'radius' as const, centre: { x: 1, y: 2 }, radius: 4, at: { x: 5, y: 0 } };
    const loaded = stateFromDimension(state, 'mm', { ...arc, diameter: true }, 'Hole');
    expect(loaded).toMatchObject({
      kind: 'arc',
      showDiameter: true,
      useCustom: true,
      radius: '4',
      aX: '1',
      aY: '2',
      bX: '5',
      bY: '0',
    });
    const specs: DimensionSpec[] = [
      { kind: 'linear', from: { x: 0, y: 0 }, to: { x: 30, y: 40 }, offset: -7 },
      { kind: 'horizontal', from: { x: 0, y: 0 }, to: { x: 30, y: 4 }, offset: 12 },
      { kind: 'vertical', from: { x: 3, y: 0 }, to: { x: 5, y: 20 }, offset: -6 },
      {
        kind: 'angular',
        centre: { x: 0, y: 0 },
        from: { x: 10, y: 0 },
        to: { x: 0, y: 8 },
        radius: 5,
      },
      { ...arc, diameter: false },
    ];
    for (const spec of specs) {
      const edited = stateFromDimension(state, 'mm', spec, '');
      expect(edited === null ? null : specFromState(edited, 'mm')).toEqual(spec);
    }
    const dragged = stateFromDimension(state, 'mm', specs[0] ?? arc, '', { x: 9, y: 9 });
    expect([dragged?.dX, dragged?.dY]).toEqual(['9', '9']);
    // The text stays where it was until its point is typed over.
    const length = stateFromDimension(state, 'mm', specs[0] ?? arc, '');
    if (length === null) {
      throw new Error('loads');
    }
    expect(editedTextAt({ ...length, bX: '40' }, 'mm', length, undefined)).toBeNull();
    expect(editedTextAt(length, 'mm', length, { x: 1, y: 2 })).toEqual({ x: 1, y: 2 });
    expect(editedTextAt({ ...length, dX: '3', dY: '4' }, 'mm', length, undefined)).toEqual({
      x: 3,
      y: 4,
    });
    const note = { kind: 'note' as const, at: { x: 0, y: 0 }, textAt: { x: 1, y: 1 }, text: 'Oak' };
    expect(stateFromDimension(state, 'mm', note, '')).toBeNull();
  });

  it('places clicks into the fields and knows when it is done', () => {
    let state = initialDimensionState('mm');
    state = placePoint(state, 'mm', { x: 1, y: 2 });
    state = placePoint(state, 'mm', { x: 11, y: 2 });
    expect(state.bX).toBe('11');
    expect(isComplete(state)).toBe(false);
    expect(placingPreview(state, { x: 5, y: 9 })[0]?.subpaths[0]?.segments).toHaveLength(2);
    state = placePoint(state, 'mm', { x: 5, y: 9 });
    expect(isComplete(state)).toBe(false);
    state = placePoint(state, 'mm', { x: 6, y: 12 });
    expect(isComplete(state)).toBe(true);
    expect(specFromState(state, 'mm')).toMatchObject({ offset: 7 });
  });

  it('checks only the points the kind uses', () => {
    const state = { ...initialDimensionState('mm'), eX: 'abc' };
    expect(validateDimension(state, 'mm')).toEqual({});
    expect(validateDimension({ ...state, kind: 'angular' }, 'mm').eX).toBeDefined();
  });

  it('measures a picked arc as a radius, or a diameter with Show Diameter', () => {
    let state: DimensionFormState = {
      ...initialDimensionState('mm'),
      kind: 'arc',
      showDiameter: true,
    };
    expect(pointCount('arc')).toBe(2);
    state = pickCircle(state, 'mm', { x: 5, y: 5 }, 12.5);
    expect([state.aX, state.radius, state.placed.length]).toEqual(['5', '12.5', 1]);
    expect(
      specFromPoints(
        'arc',
        [
          { x: 5, y: 5 },
          { x: 30, y: 5 },
        ],
        12.5,
        true,
      ),
    ).toEqual({
      kind: 'radius',
      centre: { x: 5, y: 5 },
      radius: 12.5,
      at: { x: 30, y: 5 },
      diameter: true,
    });
    expect(
      specFromPoints(
        'arc',
        [
          { x: 0, y: 0 },
          { x: 1, y: 0 },
        ],
        0,
      ),
    ).toBeNull();
    expect(validateDimension({ ...state, radius: '0' }, 'mm').radius).toBeDefined();
  });
});

describe('dragging dimension text', () => {
  const horizontal = {
    kind: 'horizontal' as const,
    from: { x: 0, y: 0 },
    to: { x: 100, y: 0 },
    offset: 20,
  };

  it('picks only the text box', () => {
    expect(onDimensionText([40, 21, 60, 26], { x: 50, y: 23 }, 0.5)).toBe(true);
    expect(onDimensionText([40, 21, 60, 26], { x: 50, y: 10 }, 0.5)).toBe(false);
    expect(onDimensionText(undefined, { x: 50, y: 23 }, 0.5)).toBe(false);
  });

  it('moves the line with the pointer and keeps the text in the middle', () => {
    const moved = dragDimensionText(horizontal, { x: 50, y: 23 }, { x: 70, y: 33 }, false);
    expect(moved.spec).toEqual({ ...horizontal, offset: 30 });
    expect(moved.textAt).toBeNull();
  });

  it('slides the text along with Shift', () => {
    const moved = dragDimensionText(horizontal, { x: 50, y: 23 }, { x: 70, y: 33 }, true);
    expect(moved.textAt).toEqual({ x: 70, y: 33 });
  });

  it('moves a linear line along its normal and an angle by radius', () => {
    const linear = { ...horizontal, kind: 'linear' as const, offset: 5 };
    const moved = dragDimensionText(linear, { x: 50, y: 5 }, { x: 60, y: 12 }, false);
    expect(moved.spec.kind === 'linear' ? moved.spec.offset : 0).toBeCloseTo(12);
    const angle = {
      kind: 'angular' as const,
      centre: { x: 0, y: 0 },
      from: { x: 10, y: 0 },
      to: { x: 0, y: 10 },
      radius: 20,
    };
    const opened = dragDimensionText(angle, { x: 20, y: 0 }, { x: 30, y: 0 }, false);
    expect(opened.spec.kind === 'angular' ? opened.spec.radius : 0).toBeCloseTo(30);
  });

  it('moves the text point of a note or a radius dimension', () => {
    const note = { kind: 'note' as const, at: { x: 0, y: 0 }, textAt: { x: 10, y: 10 }, text: 'A' };
    const moved = dragDimensionText(note, { x: 11, y: 11 }, { x: 21, y: 16 }, false);
    expect(moved.spec.kind === 'note' ? moved.spec.textAt : null).toEqual({ x: 20, y: 15 });
  });
});

describe('dimension custom text', () => {
  it('shows the custom text only with Use Custom Text on', () => {
    const state = { ...initialDimensionState('mm'), customText: ' Gap ' };
    expect(state.useCustom).toBe(false);
    expect(customTextFor(state)).toBeUndefined();
    expect(customTextFor({ ...state, useCustom: true })).toBe('Gap');
    expect(customTextFor({ ...state, useCustom: true, customText: '  ' })).toBeUndefined();
  });
});
