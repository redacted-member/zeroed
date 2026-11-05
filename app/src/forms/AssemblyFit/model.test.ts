import { describe, expect, it } from 'vitest';

import type { FitJoint, FitSide } from '../../bridge/assemblyFit';
import { gapText, jointLines, jointOrder, problemCount, sideName, sideText } from './model';

function side(normal: [number, number, number], min: number, max: number, fit = true): FitSide {
  return { normal, min, max, area: 10, fit, status: 'ok' };
}

function joint(status: FitJoint['status'], extra: Partial<FitJoint> = {}): FitJoint {
  return {
    a: { sheet: 1, part: 1 },
    b: { sheet: 2, part: 1 },
    status,
    source: 'exact',
    cell: null,
    sides: [side([1, 0, 0], 0.2, 0.2), side([0, 0, 1], 0, 0, false)],
    min: 0.2,
    max: 0.2,
    overlapDepth: 0,
    overlapVolume: 0,
    bounds: [
      [0, 0, 0],
      [1, 1, 1],
    ],
    patches: [],
    ...extra,
  };
}

describe('fit model', () => {
  it('names sides after the axis they face along', () => {
    expect(sideName([1, 0, 0])).toBe('+X');
    expect(sideName([0, -1, 0])).toBe('-Y');
    expect(sideName([0.7071, 0.7071, 0])).toBe('(0.71, 0.71, 0.00)');
  });

  it('writes one gap when a side is even, a range when not, and rests for contacts', () => {
    expect(sideText(side([1, 0, 0], 0.2, 0.2), 'mm')).toBe('+X 0.2');
    expect(sideText(side([-1, 0, 0], -0.1, 0.25), 'mm')).toBe('-X -0.1 to 0.25');
    expect(sideText({ ...side([0, 0, 1], 0.14, 0.24), round: true }, 'mm')).toBe(
      'Round about +Z 0.14 to 0.24',
    );
    expect(sideText(side([0, 0, 1], 0, 0, false), 'mm')).toBe('+Z rests');
    expect(gapText(0.20000004, 'mm')).toBe('0.2');
  });

  it('lists the worst joints first and counts the ones that need a look', () => {
    const joints = [joint('ok'), joint('interference'), joint('loose')];
    expect(jointOrder(joints)).toEqual([1, 2, 0]);
    expect(problemCount(joints)).toBe(2);
  });

  it('says what a joint was measured on and how far the pieces run in', () => {
    const names = (ref: { sheet: number }) => `S${ref.sheet}`;
    const exact = jointLines(joint('ok'), names, 'mm');
    expect(exact.title).toBe('S1 / S2');
    expect(exact.sides).toEqual(['+X 0.2', '+Z rests']);
    expect(exact.overlap).toBeNull();
    expect(exact.source).toBe('Exact edges');
    const grid = jointLines(
      joint('interference', {
        source: 'grid',
        cell: 0.8,
        sides: [],
        overlapDepth: 0,
        overlapVolume: 12.5,
      }),
      names,
      'mm',
    );
    expect(grid.status).toBe('Interference');
    expect(grid.source).toBe('Simulation grid, cell 0.8 mm');
    expect(grid.overlap).toBe('Runs in 0 mm, 12.5 mm³ shared');
  });
});
