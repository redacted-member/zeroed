import { describe, expect, it } from 'vitest';

import {
  bankSummary,
  convertDrillBankUnits,
  drillBankParams,
  drillBankStateFrom,
  initialDrillBankState,
  validateDrillBank,
} from './model';

describe('drill bank toolpath form', () => {
  it('sends the bank, the tolerances and the drilling', () => {
    const params = drillBankParams(initialDrillBankState('mm'), 'mm', 2);
    expect(params).toMatchObject({
      pitchTolerance: 0.1,
      diameterTolerance: 0.1,
      drill: { startDepth: 0, cutDepth: 10, peckDepth: 0 },
    });
    expect(params?.template.drills).toHaveLength(9);
    expect(bankSummary(initialDrillBankState('mm').template)).toEqual({
      drills: 9,
      grid: '9 x 1',
    });
  });

  it('refuses a negative tolerance and reads a toolpath back', () => {
    const state = { ...initialDrillBankState('mm'), pitchTolerance: '-1' };
    expect(validateDrillBank(state, 'mm').pitchTolerance).toBeDefined();
    expect(drillBankParams(state, 'mm', 2)).toBeNull();
    const peck = {
      ...initialDrillBankState('mm'),
      drill: { ...initialDrillBankState('mm').drill, peck: true },
    };
    const params = drillBankParams(peck, 'mm', 2);
    if (params === null) {
      throw new Error('valid');
    }
    expect(params.drill.peckDepth).toBe(2);
    const back = drillBankStateFrom(params, 'Bank', 'inch');
    expect(back.name).toBe('Bank');
    expect(back.drill.peck).toBe(true);
    expect(Number(back.pitchTolerance)).toBeCloseTo(0.1 / 25.4, 4);
    const mm = convertDrillBankUnits(back, 'inch', 'mm');
    expect(Number(mm.pitchTolerance)).toBeCloseTo(0.1, 2);
  });
});
