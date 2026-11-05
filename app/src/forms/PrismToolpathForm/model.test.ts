import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import {
  convertPrismUnits,
  initialPrismState,
  prismDirectionHint,
  prismParams,
  prismStateFrom,
  prismToolWarning,
  validatePrism,
  withFullDepth,
  withPrismDepth,
} from './model';

function tool(kind: Tool['toolType']['kind']): Tool {
  return {
    id: 1,
    name: 'Tool',
    number: 1,
    diameter: 12,
    toolType: { kind, diameter: 12 },
    passDepth: 3,
    stepover: 1,
    stepoverPercent: 10,
    feedRate: 1000,
    plungeRate: 300,
    feedUnits: 'mm/min',
    spindleSpeed: 18000,
    notes: '',
    units: 'mm',
    clearance: 0,
    cuttingData: [],
  };
}

describe('prism toolpath form model', () => {
  it('raises a 25 mm prism, cut conventional, by default', () => {
    expect(prismParams(initialPrismState('mm'), 'mm')).toEqual({
      selectionOrder: false,
      direction: 'outside',
      maxDepth: 25,
      startDepth: 0,
      cutDirection: 'conventional',
    });
  });

  it('carves full prisms while the Prism Depth is empty', () => {
    const empty = withPrismDepth(initialPrismState('mm'), ' ');
    expect(empty.limitDepth).toBe(false);
    expect(validatePrism(empty, 'mm')).toEqual({});
    expect(prismParams(empty, 'mm')?.maxDepth).toBeNull();
    const four = withPrismDepth({ ...empty, startDepth: '0.5' }, '4');
    expect(prismParams(four, 'mm')).toMatchObject({ maxDepth: 4, startDepth: 0.5 });
  });

  it('checks the Prism Depth and the start depth', () => {
    const state = withPrismDepth(initialPrismState('mm'), '0');
    expect(validatePrism(state, 'mm').maxDepth).toBe(strings.form.errorPositive);
    expect(validatePrism({ ...state, startDepth: '-1' }, 'mm').startDepth).toBeDefined();
    expect(validatePrism({ ...state, startDepth: 'x' }, 'mm').startDepth).toBe(
      strings.form.errorNumber,
    );
    expect(prismParams(state, 'mm')).toBeNull();
  });

  it('converts inches to millimetres and round trips', () => {
    const state = withPrismDepth(initialPrismState('inch'), '0.25');
    const params = prismParams(state, 'inch');
    expect(params?.maxDepth).toBeCloseTo(6.35);
    if (params !== null) {
      const back = prismStateFrom(params, 'P', 'inch');
      expect(back.limitDepth).toBe(true);
      expect(back.name).toBe('P');
      expect(prismParams(back, 'inch')).toEqual(params);
    }
    const legacy = prismStateFrom({ direction: 'inside', maxDepth: null, startDepth: 0 }, '', 'mm');
    expect(legacy).toMatchObject({ direction: 'inside', limitDepth: false, maxDepth: '' });
    expect(prismParams(legacy, 'mm')?.maxDepth).toBeNull();
    expect(convertPrismUnits(state, 'inch', 'mm').maxDepth).toBe('6.35');
  });

  it('explains each direction and warns about tools without a cone', () => {
    expect(prismDirectionHint('outside')).toBe(strings.toolpaths.prism.outsideHint);
    expect(prismDirectionHint('inside')).toBe(strings.toolpaths.prism.insideHint);
    expect(prismToolWarning(null)).toBeUndefined();
    expect(prismToolWarning(tool('vBit'))).toBeUndefined();
    expect(prismToolWarning(tool('engravingTool'))).toBeUndefined();
    expect(prismToolWarning(tool('ballNose'))).toBe(strings.toolpaths.prism.notV);
  });

  it('fills in the full prism depth and keeps the cut direction', () => {
    const state = withFullDepth(initialPrismState('inch'), 25.4, 'inch');
    expect(state.limitDepth).toBe(true);
    expect(prismParams(state, 'inch')?.maxDepth).toBeCloseTo(25.4);
    const conventional = { ...state, cutDirection: 'conventional' as const };
    const params = prismParams(conventional, 'inch');
    expect(params?.cutDirection).toBe('conventional');
    if (params !== null) {
      expect(prismStateFrom(params, 'P', 'inch').cutDirection).toBe('conventional');
      expect(prismStateFrom({ ...params, cutDirection: undefined }, 'P', 'inch').cutDirection).toBe(
        'climb',
      );
    }
  });

  it('sends Use Vector Selection Order and reads it back', () => {
    const state = { ...initialPrismState('mm'), selectionOrder: true };
    const params = prismParams(state, 'mm');
    expect(params?.selectionOrder).toBe(true);
    if (params !== null) {
      expect(prismStateFrom(params, 'N', 'mm').selectionOrder).toBe(true);
      expect(
        prismStateFrom({ ...params, selectionOrder: undefined }, 'N', 'mm').selectionOrder,
      ).toBe(false);
    }
  });
});
