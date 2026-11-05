import { describe, expect, it } from 'vitest';

import {
  convertReplaceBelowUnits,
  initialReplaceBelowState,
  removesEverything,
  replaceBelowArgs,
  replaceTarget,
  validateReplaceBelow,
  type ReplaceBelowState,
} from './model';

const state = (patch: Partial<ReplaceBelowState>): ReplaceBelowState => ({
  ...initialReplaceBelowState('mm'),
  ...patch,
});

describe('replace below form model', () => {
  it('starts at the zero plane, transparent, not lowered', () => {
    expect(initialReplaceBelowState('mm')).toEqual({
      height: '0',
      with: 'transparent',
      lower: false,
    });
  });

  it('works on the picked components, else the active level', () => {
    expect(replaceTarget([3, 4], 1)).toEqual({ kind: 'components', ids: [3, 4] });
    expect(replaceTarget([], 1)).toEqual({ kind: 'level', id: 1 });
    expect(replaceTarget([], null)).toBeNull();
  });

  it('sends the height in millimetres with the options', () => {
    expect(
      replaceBelowArgs(state({ height: '2', with: 'plane', lower: true }), 'mm', {
        kind: 'components',
        ids: [4],
      }),
    ).toEqual({ ids: [4], height: 2, with: 'plane', lower: true });
    expect(replaceBelowArgs(state({ height: '0.5' }), 'inch', { kind: 'level', id: 2 })).toEqual({
      level: 2,
      height: 12.7,
      with: 'transparent',
      lower: false,
    });
  });

  it('needs a number and something to work on', () => {
    expect(validateReplaceBelow(state({ height: 'x' }), 'mm').height).toBeDefined();
    expect(replaceBelowArgs(state({ height: 'x' }), 'mm', { kind: 'level', id: 1 })).toBeNull();
    expect(replaceBelowArgs(state({ height: '2' }), 'mm', null)).toBeNull();
    expect(validateReplaceBelow(state({ height: '-2' }), 'mm')).toEqual({});
  });

  it('converts with the job units', () => {
    expect(convertReplaceBelowUnits(state({ height: '25.4' }), 'mm', 'inch').height).toBe('1');
  });

  it('warns when nothing would be left', () => {
    expect(removesEverything(state({ height: '5' }), 'mm', 5)).toBe(true);
    expect(removesEverything(state({ height: '5', with: 'plane' }), 'mm', 5)).toBe(false);
    expect(removesEverything(state({ height: '4.9' }), 'mm', 5)).toBe(false);
    expect(removesEverything(state({ height: '9' }), 'mm', null)).toBe(false);
  });
});
