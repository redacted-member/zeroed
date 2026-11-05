import { describe, expect, it } from 'vitest';

import type { EntitySummary } from '../../bridge/document';
import { polylinePath } from '../../lib/geometry';
import {
  initialTurnSpinState,
  profileFromSelection,
  revolveArgs,
  revolvePreview,
  spinCentre,
  spinRadius,
  validateTurnSpin,
} from './model';

const entities = [
  { id: 1, kind: 'path', closed: false },
  { id: 2, kind: 'path', closed: true },
] as EntitySummary[];

const leftward = polylinePath([
  { x: 40, y: 30 },
  { x: 40, y: 33 },
  { x: 30, y: 33 },
  { x: 30, y: 30 },
]);

describe('turn and spin form model', () => {
  it('takes one open vector as the profile', () => {
    expect(profileFromSelection([1], entities)).toBe(1);
    expect(profileFromSelection([2], entities)).toBeNull();
    expect(profileFromSelection([1, 2], entities)).toBeNull();
  });

  it('sends no base and, when asked, the exact height for either mode', () => {
    const state = initialTurnSpinState('mm');
    expect(revolveArgs(state, 'mm')).toEqual({
      mode: 'turn',
      params: { baseHeight: 0, exactHeight: false, height: 10 },
    });
    const spin = { ...state, mode: 'spin' as const, exactHeight: true, height: '4' };
    expect(revolveArgs(spin, 'mm')).toEqual({
      mode: 'spin',
      params: { baseHeight: 0, exactHeight: true, height: 4 },
    });
  });

  it('checks the height only when Scale to Exact Height is on', () => {
    const state = { ...initialTurnSpinState('mm'), height: '0' };
    expect(validateTurnSpin(state, 'mm')).toEqual({});
    expect(validateTurnSpin({ ...state, exactHeight: true }, 'mm').height).toBeDefined();
    expect(revolveArgs({ ...state, exactHeight: true }, 'mm')).toBeNull();
  });

  it('spins round the left end, whichever way the profile was drawn', () => {
    expect(spinCentre(leftward)).toEqual({ x: 30, y: 30 });
    expect(spinRadius(leftward)).toBeCloseTo(10);
    const spin = { ...initialTurnSpinState('mm'), mode: 'spin' as const };
    const preview = revolvePreview(spin, leftward);
    expect(preview).toHaveLength(2);
    expect(preview[1]?.subpaths[0]?.start).toEqual({ x: 40, y: 30 });
    const turn = revolvePreview(initialTurnSpinState('mm'), leftward);
    expect(turn[1]?.subpaths[0]?.segments[0]?.to).toEqual({ x: 30, y: 30 });
    expect(revolvePreview(spin, undefined)).toEqual([]);
  });
});
