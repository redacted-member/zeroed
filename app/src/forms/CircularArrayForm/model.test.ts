import { describe, expect, it } from 'vitest';

import {
  centreFromSelection,
  circularArrayArgs,
  initialCircularArrayState,
  validateCircularArray,
} from './model';

describe('circular array form model', () => {
  it('turns six copies about the selection centre by default', () => {
    const state = centreFromSelection(initialCircularArrayState('mm'), 'mm', [0, 0, 20, 10]);
    const args = circularArrayArgs(state, 'mm');
    expect(args).toEqual({
      cx: 10,
      cy: 5,
      count: 6,
      angle: 360,
      rotateCopies: true,
      radius: null,
      group: false,
      step: -60,
    });
  });

  it('shares a total angle out or takes a step angle, clockwise when positive', () => {
    const state = { ...initialCircularArrayState('mm'), count: '4', angle: '180' };
    expect(circularArrayArgs(state, 'mm')?.step).toBe(-45);
    const step = { ...state, angleMode: 'step' as const, angle: '-30' };
    expect(circularArrayArgs(step, 'mm')?.step).toBe(30);
    expect(validateCircularArray({ ...state, angle: '0' }, 'mm').angle).toBeDefined();
  });

  it('takes a typed center', () => {
    const typed = { ...initialCircularArrayState('inch'), cx: '1', cy: '2' };
    expect(circularArrayArgs(typed, 'inch')).toMatchObject({ cx: 25.4, cy: 50.8 });
    expect(centreFromSelection(typed, 'inch', null)).toBe(typed);
  });

  it('needs at least two items and an angle up to a full turn', () => {
    const state = initialCircularArrayState('mm');
    expect(validateCircularArray({ ...state, count: '1' }, 'mm').count).toBeDefined();
    expect(validateCircularArray({ ...state, angle: '400' }, 'mm').angle).toBeDefined();
    expect(circularArrayArgs({ ...state, cx: 'x' }, 'mm')).toBeNull();
  });
});
