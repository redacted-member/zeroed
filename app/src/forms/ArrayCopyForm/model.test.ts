import { describe, expect, it } from 'vitest';

import {
  arrayCopyArgs,
  copyCount,
  initialArrayCopyState,
  nextTurn,
  validateArrayCopy,
} from './model';

describe('array copy form model', () => {
  it('builds a two by three grid with 10 mm gaps by default', () => {
    const state = initialArrayCopyState('mm');
    expect(copyCount(state)).toBe(5);
    expect(arrayCopyArgs(state, 'mm')).toEqual({
      rows: 2,
      cols: 3,
      gapX: 10,
      gapY: 10,
      mode: 'gap',
      stagger: 'none',
      group: false,
      symmetry: ['none', 'none', 'none', 'none'],
      rowShift: [0, 0],
      colShift: [0, 0],
    });
  });

  it('steps the symmetry buttons and sends the displacements', () => {
    const state = initialArrayCopyState('mm');
    const once = nextTurn(state.symmetry, 3);
    expect(once).toEqual(['none', 'none', 'none', 'mirrorX']);
    expect(nextTurn(nextTurn(nextTurn(once, 3), 3), 3)[3]).toBe('none');
    expect(nextTurn(once, 0)).toEqual(once);
    const shifted = { ...state, rowX: '5', colY: '-2' };
    expect(arrayCopyArgs(shifted, 'mm')).toMatchObject({ rowShift: [5, 0], colShift: [0, -2] });
    expect(validateArrayCopy({ ...state, rowY: 'a' }, 'mm').rowY).toBeDefined();
  });

  it('refuses fractional counts and a one by one grid', () => {
    const state = { ...initialArrayCopyState('mm'), rows: '1.5' };
    expect(validateArrayCopy(state, 'mm').rows).toBeDefined();
    expect(arrayCopyArgs({ ...state, rows: '1', cols: '1' }, 'mm')).toBeNull();
  });

  it('allows a zero gap but not a zero offset', () => {
    const state = { ...initialArrayCopyState('mm'), gapX: '0' };
    expect(validateArrayCopy(state, 'mm').gapX).toBeUndefined();
    expect(validateArrayCopy({ ...state, mode: 'offset' }, 'mm').gapX).toBeDefined();
  });

  it('converts inch gaps to millimetres', () => {
    const state = { ...initialArrayCopyState('inch'), gapX: '1', gapY: '2' };
    expect(arrayCopyArgs(state, 'inch')).toMatchObject({ gapX: 25.4, gapY: 50.8 });
  });
});
