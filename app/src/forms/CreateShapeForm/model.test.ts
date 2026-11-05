import { describe, expect, it } from 'vitest';

import type { ComponentSummary, ShapeParams } from '../../bridge/model';
import {
  canApply,
  editableShape,
  fieldsOf,
  initialShapeState,
  shapeArgs,
  shapeParams,
  shapeVectors,
  stateFromComponent,
  toolOf,
  validateShape,
  withTotalHeight,
} from './model';

const leafParams: ShapeParams = {
  profile: 'smooth',
  angle: 90,
  height: 3,
  limit: true,
  baseHeight: 0,
  smoothTop: 0.2,
  smoothBottom: 0.8,
  sharpCorners: true,
  blendInner: true,
  scale: true,
  scaleHeight: 4,
};

function shapeComponent(overrides: Partial<ComponentSummary> = {}): ComponentSummary {
  return {
    id: 7,
    name: 'Leaf',
    visible: true,
    combine: 'mergeHigh',
    heightScale: 1,
    level: 1,
    maxHeight: 6,
    baseHeight: 2,
    shape: { params: leafParams, vectors: [3, 4] },
    fade: null,
    tilt: { pivot: { x: 0, y: 0 }, toward: { x: 0, y: 10 }, angleDeg: 12 },
    ...overrides,
  };
}

describe('create shape tools', () => {
  it('shows each tool the settings the form has', () => {
    expect(fieldsOf('flat')).toEqual({
      angle: false,
      smoothing: false,
      limit: false,
      sharpCorners: false,
      blendInner: false,
      scale: false,
      totalHeight: false,
    });
    expect(fieldsOf('round')).toMatchObject({ angle: true, limit: true, blendInner: false });
    expect(fieldsOf('angled')).toMatchObject({ angle: true, scale: true, blendInner: false });
    expect(fieldsOf('smooth')).toMatchObject({ angle: false, smoothing: true, blendInner: true });
    expect(fieldsOf('custom')).toMatchObject({ angle: false, smoothing: false, blendInner: true });
  });

  it('makes a round shape at ninety degrees by default, its base on the component', () => {
    const state = { ...initialShapeState('mm'), baseHeight: '1.5' };
    expect(shapeParams(state, 'round', 'mm')).toEqual({
      profile: 'round',
      angle: 90,
      height: 5,
      limit: false,
      baseHeight: 0,
      smoothTop: 0.5,
      smoothBottom: 0.5,
      sharpCorners: false,
      blendInner: false,
      scale: false,
      scaleHeight: 5,
    });
    expect(shapeArgs(state, 'round', 'mm', [4])).toEqual({
      ids: [4],
      params: expect.objectContaining({ profile: 'round' }),
      combine: 'add',
      baseHeight: 1.5,
    });
  });

  it('sends a flat plane with no rise and only the options it has', () => {
    const state = {
      ...initialShapeState('mm'),
      limitOn: true,
      scaleOn: true,
      sharpCorners: true,
      blendInner: true,
    };
    const params = shapeParams(state, 'flat', 'mm');
    expect(params).toMatchObject({
      profile: 'flat',
      height: 0,
      limit: false,
      scale: false,
      sharpCorners: false,
      blendInner: false,
    });
    expect(canApply('flat', [])).toBe(true);
    expect(canApply('round', [])).toBe(false);
    // An open vector alone is not a shape; a custom shape needs one.
    const entities = [
      { id: 1, kind: 'path', closed: true },
      { id: 2, kind: 'path', closed: false },
      { id: 3, kind: 'text', closed: false },
      { id: 4, kind: 'path', closed: false },
    ];
    expect(canApply('round', [2], entities)).toBe(false);
    expect(canApply('flat', [2], entities)).toBe(false);
    expect(canApply('round', [1, 2], entities)).toBe(true);
    expect(canApply('smooth', [3], entities)).toBe(true);
    expect(canApply('custom', [1], entities)).toBe(false);
    expect(canApply('custom', [1, 2], entities)).toBe(true);
    expect(canApply('custom', [1, 2, 4], entities)).toBe(false);
  });

  it('checks the limit and scale heights only when they are on', () => {
    const state = { ...initialShapeState('mm'), limit: '0', scaleHeight: '-1' };
    expect(validateShape(state, 'round', 'mm')).toEqual({});
    const on = { ...state, limitOn: true, scaleOn: true };
    expect(Object.keys(validateShape(on, 'round', 'mm')).sort()).toEqual(['limit', 'scaleHeight']);
    expect(validateShape(on, 'flat', 'mm')).toEqual({});
    expect(shapeParams(on, 'round', 'mm')).toBeNull();
  });

  // Review 033 row 15: Angled opened at 90 degrees with no height limit,
  // which the kernel refuses (upright sides, no top).
  it('opens Angled at 45 degrees, valid, and asks for the limit at 90', () => {
    for (const units of ['mm', 'inch'] as const) {
      const angled = initialShapeState(units, 'angled');
      expect(angled.angle).toBe('45');
      expect(angled.limitOn).toBe(false);
      expect(validateShape(angled, 'angled', units)).toEqual({});
      expect(shapeParams(angled, 'angled', units)).toMatchObject({
        profile: 'angular',
        angle: 45,
        limit: false,
      });
    }
    expect(initialShapeState('mm', 'round').angle).toBe('90');
    const upright = { ...initialShapeState('mm', 'angled'), angle: '90' };
    expect(validateShape(upright, 'angled', 'mm').angle).toMatch(/Limit to Height/);
    expect(shapeParams(upright, 'angled', 'mm')).toBeNull();
    expect(validateShape({ ...upright, angle: '89.99' }, 'angled', 'mm')).toEqual({});
    expect(validateShape({ ...upright, limitOn: true }, 'angled', 'mm')).toEqual({});
    expect(shapeParams({ ...upright, limitOn: true }, 'angled', 'mm')).toMatchObject({
      angle: 90,
      limit: true,
      height: 5,
    });
    expect(validateShape(upright, 'round', 'mm')).toEqual({});
  });

  it('reads angles and smoothing only where they apply', () => {
    const state = { ...initialShapeState('mm'), angle: '0', smoothTop: '150' };
    expect(validateShape(state, 'round', 'mm').angle).toBeDefined();
    expect(validateShape(state, 'smooth', 'mm').angle).toBeUndefined();
    expect(validateShape(state, 'smooth', 'mm').smoothTop).toBeDefined();
    const smooth = shapeParams({ ...state, smoothTop: '20', smoothBottom: '80' }, 'smooth', 'mm');
    expect(smooth).toMatchObject({ profile: 'smooth', smoothTop: 0.2, smoothBottom: 0.8 });
  });

  it('opens on the picked shape its tool made and edits it', () => {
    const leaf = shapeComponent();
    const other = shapeComponent({ id: 8, shape: null });
    expect(editableShape([leaf, other], [7], 'smooth')).toBe(leaf);
    expect(editableShape([leaf, other], [7], 'round')).toBeNull();
    expect(editableShape([leaf, other], [8], 'smooth')).toBeNull();
    expect(editableShape([leaf, other], [7, 8], 'smooth')).toBeNull();
    const state = stateFromComponent(leaf, 'mm');
    expect(state).toMatchObject({
      name: 'Leaf',
      combine: 'mergeHigh',
      limitOn: true,
      limit: '3',
      sharpCorners: true,
      blendInner: true,
      scaleOn: true,
      scaleHeight: '4',
      baseHeight: '2',
      smoothTop: '20',
      smoothBottom: '80',
    });
    expect(state.dynamic.tiltOn).toBe(true);
    expect(state.dynamic.fadeOn).toBe(false);
    expect(shapeVectors([], leaf)).toEqual([3, 4]);
    expect(shapeVectors([9], leaf)).toEqual([9]);
    expect(
      shapeVectors([], {
        ...leaf,
        shape: { params: leafParams, vectors: [3, 4], profileVector: 5 },
      }),
    ).toEqual([3, 4, 5]);
    expect(toolOf('angular')).toBe('angled');
  });

  it('sends a fade or tilt only once both anchors are set', () => {
    const base = initialShapeState('mm');
    const waiting = { ...base, dynamic: { ...base.dynamic, fadeOn: true } };
    expect(shapeArgs(waiting, 'round', 'mm', [1])).not.toHaveProperty('fade');
    const set = {
      ...base,
      dynamic: {
        ...base.dynamic,
        fadeOn: true,
        fadeFrom: { x: 0, y: 0 },
        fadeTo: { x: 5, y: 0 },
        fadePercent: '40',
      },
    };
    expect(shapeArgs(set, 'round', 'mm', [1])?.fade).toEqual({
      from: { x: 0, y: 0 },
      to: { x: 5, y: 0 },
      amount: 0.4,
    });
  });

  it('spreads a typed total height over the shape and its base', () => {
    const state = { ...initialShapeState('mm'), baseHeight: '2' };
    // The last Apply rose 6 on a base of 2: a total of 12 scales both by 1.5.
    const next = withTotalHeight(state, '12', 6, 'mm');
    expect(next).toMatchObject({ scaleOn: true, scaleHeight: '9', baseHeight: '3' });
    // Before any Apply the base stays and the shape takes the rest.
    expect(withTotalHeight(state, '10', null, 'mm')).toMatchObject({
      scaleOn: true,
      scaleHeight: '8',
      baseHeight: '2',
    });
    expect(withTotalHeight(state, 'x', 6, 'mm')).toBe(state);
  });
});
