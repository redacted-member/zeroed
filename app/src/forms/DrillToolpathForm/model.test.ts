import { describe, expect, it } from 'vitest';

import { drillParams, drillStateFrom, initialDrillState, validateDrill } from './model';

const PASS = 3;

describe('drill toolpath form model', () => {
  it('drills 10 mm in one plunge by default', () => {
    expect(drillParams(initialDrillState('mm'), 'mm', PASS)).toEqual({
      selectionOrder: false,
      startDepth: 0,
      cutDepth: 10,
      peckDepth: 0,
      fullRetract: true,
      retractGap: 0,
      dwell: 0,
    });
  });

  it("pecks the tool's pass depth at a time", () => {
    const state = { ...initialDrillState('mm'), peck: true };
    expect(drillParams(state, 'mm', 2.5)?.peckDepth).toBe(2.5);
    expect(drillParams({ ...state, peck: false }, 'mm', 2.5)?.peckDepth).toBe(0);
  });

  it('sends the Retract Gap and refuses a negative one while pecking', () => {
    const state = { ...initialDrillState('inch'), peck: true, retractGap: '0.1' };
    expect(drillParams(state, 'inch', PASS)?.retractGap).toBeCloseTo(2.54);
    expect(validateDrill({ ...state, retractGap: '-1' }, 'inch').retractGap).toBeDefined();
    expect(validateDrill({ ...state, peck: false, retractGap: '-1' }, 'inch').retractGap).toBe(
      undefined,
    );
  });

  it('starts the Retract Gap at 0 and gives older toolpaths the 1 mm they were cut with', () => {
    const params = drillParams(initialDrillState('mm'), 'mm', PASS);
    expect(params).not.toBeNull();
    if (params !== null) {
      const older = { ...params };
      delete older.retractGap;
      expect(drillStateFrom(older, 'Old', 'mm').retractGap).toBe('1');
    }
  });

  it('measures the cut depth from the start depth', () => {
    const state = { ...initialDrillState('mm'), startDepth: '4', cutDepth: '3' };
    const params = drillParams(state, 'mm', PASS);
    expect(params?.cutDepth).toBe(7);
    if (params !== null) {
      expect(drillStateFrom(params, 'Holes', 'mm').cutDepth).toBe('3');
    }
  });

  it('dwells only when the dwell check is on', () => {
    const state = { ...initialDrillState('mm'), dwell: '2' };
    expect(drillParams(state, 'mm', PASS)?.dwell).toBe(0);
    expect(drillParams({ ...state, useDwell: true }, 'mm', PASS)?.dwell).toBe(2);
    expect(validateDrill({ ...state, dwell: '-1' }, 'mm').dwell).toBeUndefined();
    expect(validateDrill({ ...state, useDwell: true, dwell: '-1' }, 'mm').dwell).toBeDefined();
  });

  it('round trips its parameters', () => {
    const params = drillParams(
      { ...initialDrillState('inch'), peck: true, useDwell: true, dwell: '0.5' },
      'inch',
      PASS,
    );
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(drillParams(drillStateFrom(params, 'Holes', 'inch'), 'inch', PASS)).toEqual(params);
    }
  });

  it('sends Use Vector Selection Order and reads it back', () => {
    const state = { ...initialDrillState('mm'), selectionOrder: true };
    const params = drillParams(state, 'mm', PASS);
    expect(params?.selectionOrder).toBe(true);
    if (params !== null) {
      expect(drillStateFrom(params, 'N', 'mm').selectionOrder).toBe(true);
      expect(
        drillStateFrom({ ...params, selectionOrder: undefined }, 'N', 'mm').selectionOrder,
      ).toBe(false);
    }
  });
});
