import { describe, expect, it } from 'vitest';

import { DEFAULT_ORIENTATION } from '../../bridge/meshImport';
import {
  normalAngle,
  orientateState,
  orientationPatch,
  scaleByInch,
  sizeError,
  sizeValue,
  switchUnits,
  turnZ,
  typeSize,
  withTurns,
} from './model';

const start = () => orientateState(DEFAULT_ORIENTATION, [10, 20, 5], 'mm');

describe('orientate model form', () => {
  it('shows turns made with the rings', () => {
    const turned = withTurns(start(), { rotX: 12.34567, rotY: -90, rotZ: 0 });
    expect([turned.rotX, turned.rotY, turned.rotZ]).toEqual(['12.346', '-90', '0']);
    expect(turned.size).toEqual(start().size);
  });

  it('starts from the kernel size with the ratio locked and no perspective', () => {
    const state = start();
    expect(state.size).toEqual(['10', '20', '5']);
    expect(state.lockRatio).toBe(true);
    expect(orientationPatch(state)).toEqual({
      top: 'top',
      rotX: 0,
      rotY: 0,
      rotZ: 0,
      perspective: 0,
    });
  });

  it('keeps the proportions while the ratio is locked', () => {
    const locked = typeSize(start(), 0, '20', [10, 20, 5]);
    expect(locked.size).toEqual(['20', '40', '10']);
    const free = typeSize({ ...start(), lockRatio: false }, 0, '20', [10, 20, 5]);
    expect(free.size).toEqual(['20', '20', '5']);
    expect(sizeValue(free)).toEqual([20, 20, 5]);
  });

  it('refuses a size that is not positive, allowing a flat Z', () => {
    const bad = typeSize({ ...start(), lockRatio: false }, 1, '0', null);
    expect(sizeValue(bad)).toBeNull();
    expect(sizeError(bad, 1)).toBeDefined();
    const flat = typeSize({ ...start(), lockRatio: false }, 2, '0', null);
    expect(sizeError(flat, 2)).toBeUndefined();
  });

  it('scales by 25.4 either way and shows other units without resizing', () => {
    const up = scaleByInch(start(), true);
    expect(sizeValue(up)?.[0]).toBeCloseTo(254);
    const down = scaleByInch(up, false);
    expect(sizeValue(down)?.[0]).toBeCloseTo(10);
    const inches = switchUnits(up, 'inch');
    expect(inches.units).toBe('inch');
    expect(Number(inches.size[0])).toBeCloseTo(10);
    expect(sizeValue(inches)?.[0]).toBeCloseTo(254);
  });

  it('quick turns add up about Z and wrap', () => {
    expect(turnZ(start(), 90).rotZ).toBe('90');
    expect(turnZ(turnZ(start(), 90), 180).rotZ).toBe('-90');
    expect(normalAngle(-180)).toBe(180);
    expect(normalAngle(540)).toBe(180);
    expect(orientationPatch({ ...start(), rotY: 'x' })).toBeNull();
  });

  it('sends perspective only when it is on', () => {
    const on = { ...start(), perspectiveOn: true, perspective: 40 };
    expect(orientationPatch(on)?.perspective).toBeCloseTo(0.4);
    expect(orientationPatch({ ...on, perspectiveOn: false })?.perspective).toBe(0);
  });
});
