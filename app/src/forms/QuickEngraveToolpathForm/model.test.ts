import { describe, expect, it } from 'vitest';

import { engraveParams, engraveStateFrom, initialEngraveState, validateEngrave } from './model';

describe('quick engrave form model', () => {
  it('engraves the outline half a millimetre deep in one pass by default', () => {
    expect(engraveParams(initialEngraveState('mm'), 'mm')).toEqual({
      depth: 0.5,
      passes: 1,
      mode: 'outline',
      stepover: 0.3,
      angle: 0,
      pattern: 'raster',
      noseCone: false,
    });
  });

  it('checks the fill fields only when filling', () => {
    const state = { ...initialEngraveState('mm'), stepover: '0' };
    expect(validateEngrave(state, 'mm')).toEqual({});
    expect(validateEngrave({ ...state, mode: 'fill' }, 'mm').stepover).toBeDefined();
    expect(validateEngrave({ ...state, passes: '0' }, 'mm').passes).toBeDefined();
  });

  it('round trips its parameters', () => {
    const params = engraveParams(
      {
        ...initialEngraveState('inch'),
        mode: 'fill',
        passes: '3',
        pattern: 'crossHatch',
        noseCone: true,
      },
      'inch',
    );
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(engraveParams(engraveStateFrom(params, 'E', 'inch'), 'inch')).toEqual(params);
    }
  });

  it('opens an engraving saved before fill patterns as a raster', () => {
    const state = engraveStateFrom(
      { depth: 0.2, passes: 1, mode: 'fill', stepover: 0.3, angle: 0 },
      'Old',
      'mm',
    );
    expect(state.pattern).toBe('raster');
    expect(state.noseCone).toBe(false);
  });

  it('cuts at the nose cone Tool Depth while the nose cone is on', () => {
    const state = { ...initialEngraveState('mm'), depth: '0.2', toolDepth: '0.7', passes: '3' };
    expect(engraveParams(state, 'mm')?.depth).toBe(0.2);
    const cone = { ...state, noseCone: true };
    expect(engraveParams(cone, 'mm')).toMatchObject({ depth: 0.7, passes: 3, noseCone: true });
    expect(validateEngrave({ ...cone, depth: '0' }, 'mm').depth).toBeUndefined();
    expect(validateEngrave({ ...cone, toolDepth: '0' }, 'mm').toolDepth).toBeDefined();
    const params = engraveParams(cone, 'mm');
    if (params !== null) {
      expect(engraveStateFrom(params, 'E', 'mm').toolDepth).toBe('0.7');
    }
  });
});
