import { describe, expect, it } from 'vitest';

import {
  convertOffsetModelUnits,
  initialOffsetModelState,
  offsetArgs,
  validateOffsetModel,
} from './model';

describe('offset model form model', () => {
  it('starts with a small outward offset and no clipping', () => {
    expect(initialOffsetModelState('mm')).toEqual({ distance: '1', clipToZero: false });
  });

  it('sends millimetres and adds onto the previous preview', () => {
    expect(offsetArgs({ distance: '-0.1', clipToZero: true }, 'inch', null)).toEqual({
      distance: -2.54,
      clipToZero: true,
    });
    expect(offsetArgs({ distance: '2', clipToZero: false }, 'mm', { revision: 8, id: 3 })).toEqual({
      distance: 2,
      clipToZero: false,
      merge: { revision: 8, id: 3 },
    });
  });

  it('refuses zero and text that is not a number', () => {
    expect(validateOffsetModel({ distance: '0', clipToZero: false }, 'mm').distance).toBeDefined();
    expect(offsetArgs({ distance: 'x', clipToZero: false }, 'mm', null)).toBeNull();
  });

  it('converts with the job units', () => {
    expect(
      convertOffsetModelUnits({ distance: '25.4', clipToZero: false }, 'mm', 'inch').distance,
    ).toBe('1');
  });
});
