import { describe, expect, it } from 'vitest';

import type { EntitySummary } from '../../bridge/document';
import {
  initialNestState,
  NEST_SHEET_LIMIT,
  nestMarkers,
  nestOptions,
  nestParams,
  partCopiesError,
  partMark,
  partPropertiesOf,
  validateNest,
} from './model';

const entity = (id: number, nest?: EntitySummary['nest']): EntitySummary => ({
  id,
  kind: 'path',
  layer: 1,
  bbox: [10, 20, 70, 50],
  closed: true,
  revision: 1,
  ...(nest === undefined ? {} : { nest }),
});

describe('nest form model', () => {
  it('starts valid, each sheet nesting at its own size', () => {
    const state = initialNestState('mm');
    expect(validateNest(state, 'mm')).toEqual({});
    expect(nestParams(state, 'mm')).toEqual({
      toolDiameter: 6,
      gap: 1,
      border: 10,
      rotationStepDeg: 90,
      allowMirror: false,
      sheetWidth: 0,
      sheetHeight: 0,
      startCorner: 'bottomLeft',
      direction: 'x',
      maxSheets: NEST_SHEET_LIMIT,
      nestInHoles: false,
    });
  });

  it('has no sheet size fields or components tick', () => {
    const state = initialNestState('mm');
    for (const extra of ['useJobSize', 'sheetWidth', 'sheetHeight', 'withComponents']) {
      expect(Object.keys(state)).not.toContain(extra);
    }
  });

  it('rotates parts in the typed step when Rotate Parts is on', () => {
    const state = initialNestState('mm');
    expect(nestParams(state, 'mm')?.rotationStepDeg).toBe(90);
    expect(nestParams({ ...state, rotate: false }, 'mm')?.rotationStepDeg).toBe(0);
    expect(nestParams({ ...state, rotationStep: '15' }, 'mm')?.rotationStepDeg).toBe(15);
    expect(validateNest({ ...state, rotationStep: '0.5' }, 'mm').rotationStep).toBeDefined();
    expect(validateNest({ ...state, rotate: false, rotationStep: '0' }, 'mm')).toEqual({});
  });

  it('applies the Number of copies and the Filler tick as one mark', () => {
    const state = initialNestState('mm');
    expect(partMark({ ...state, partCopies: '3' })).toEqual({ copies: 3, filler: false });
    expect(partMark({ ...state, partFiller: true })).toEqual({ copies: 1, filler: true });
    const bad = { ...state, partCopies: '0' };
    expect(partCopiesError(bad)).toBeDefined();
    expect(partMark(bad)).toBeNull();
    expect(partMark({ ...state, partCopies: '2.5' })).toBeNull();
  });

  it('shows the copies and filler mark the selected objects keep', () => {
    const entities = [
      entity(4, { copies: 5, filler: false }),
      entity(6, { copies: 1, filler: true }),
      entity(9),
    ];
    expect(partPropertiesOf(entities, [4])).toEqual({ partCopies: '5', partFiller: false });
    expect(partPropertiesOf(entities, [9, 6])).toEqual({ partCopies: '1', partFiller: true });
    expect(partPropertiesOf(entities, [9])).toEqual({ partCopies: '1', partFiller: false });
  });

  it('marks copies with their number and fillers with a star at the middle of the object', () => {
    expect(
      nestMarkers([
        entity(1, { copies: 4, filler: false }),
        entity(2, { copies: 2, filler: true }),
        entity(3),
      ]),
    ).toEqual([
      { id: 1, at: { x: 40, y: 35 }, text: '4' },
      { id: 2, at: { x: 40, y: 35 }, text: '2*' },
    ]);
  });

  it('sends the nest into holes option', () => {
    const state = { ...initialNestState('mm'), nestInHoles: true };
    expect(nestParams(state, 'mm')?.nestInHoles).toBe(true);
  });

  it('passes components, originals and two sided options', () => {
    const state = initialNestState('mm');
    expect(nestOptions(state, [4, 5], true)).toEqual({
      components: [4, 5],
      removeOriginals: true,
      twoSided: false,
    });
    const changed = { ...state, twoSided: true, removeOriginals: false };
    expect(nestOptions(changed, [4], true)).toEqual({
      components: [4],
      removeOriginals: false,
      twoSided: true,
    });
    expect(nestOptions(changed, [4], false).twoSided).toBe(false);
    const plan = [{ sheet: 2, quantity: 1 }, { sheet: 3 }];
    expect(nestOptions({ ...state, sheetPlan: plan }, [], false).sheetPlan).toEqual(plan);
    expect(nestOptions(state, [], false)).not.toHaveProperty('sheetPlan');
    expect(nestOptions({ ...state, boundaryLayer: 2 }, [], false).boundaryLayer).toBe(2);
    expect(nestOptions(state, [], false)).not.toHaveProperty('boundaryLayer');
  });
});
