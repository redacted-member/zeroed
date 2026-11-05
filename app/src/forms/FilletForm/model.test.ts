import { describe, expect, it } from 'vitest';

import {
  FILLET_KINDS,
  filletRadius,
  initialFilletState,
  pickTolerance,
  radiusError,
} from './model';

describe('fillet form model', () => {
  it('reads the radius in the job unit', () => {
    expect(filletRadius(initialFilletState('mm'), 'mm')).toBe(3);
    expect(filletRadius({ radius: '0.25', kind: 'dogbone' }, 'inch')).toBeCloseTo(6.35);
  });

  it('refuses a radius that is not positive', () => {
    expect(radiusError({ radius: '-1', kind: 'normal' }, 'mm')).toBeDefined();
    expect(filletRadius({ radius: '', kind: 'normal' }, 'mm')).toBeNull();
  });

  it('offers the four reference fillet types, Plasma / Drag Knife last', () => {
    expect(FILLET_KINDS).toEqual(['normal', 'dogbone', 'tbone', 'plasma']);
    expect(filletRadius({ radius: '2', kind: 'plasma' }, 'mm')).toBe(2);
  });

  it('turns the snap radius into millimetres at the current zoom', () => {
    expect(pickTolerance(8, 2)).toBe(4);
    expect(pickTolerance(8, 0)).toBe(1);
  });
});
