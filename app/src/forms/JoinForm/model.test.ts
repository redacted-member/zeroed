import { describe, expect, it } from 'vitest';

import { convertJoinUnits, initialJoinState, joinTolerance, toleranceError } from './model';

describe('join form model', () => {
  it('starts with a small tolerance in millimetres', () => {
    expect(joinTolerance(initialJoinState('mm'), 'mm')).toBeCloseTo(0.1);
  });

  it('rejects a tolerance that is not positive', () => {
    expect(toleranceError({ tolerance: '0' }, 'mm')).toBeDefined();
    expect(joinTolerance({ tolerance: 'x' }, 'mm')).toBeNull();
  });

  it('reads inches as millimetres', () => {
    expect(joinTolerance({ tolerance: '1' }, 'inch')).toBeCloseTo(25.4);
    expect(convertJoinUnits({ tolerance: '25.4' }, 'mm', 'inch').tolerance).toBe('1');
  });
});
