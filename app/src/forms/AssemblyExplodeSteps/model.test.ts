import { describe, expect, it } from 'vitest';

import type { ExplodeStep } from '../../bridge/assembly';
import { directionId, moveStep, newStep, withoutPiece, withPiece } from './model';

const step = (id: number, sheets: number[]): ExplodeStep => ({
  id,
  name: `S${id}`,
  pieces: sheets.map((sheet) => ({ sheet, part: 1, component: null })),
  direction: [0, 0, 1],
  distance: 10,
});

describe('explode steps', () => {
  it('starts a step out of the joints when the offset is known, else straight up', () => {
    const along = newStep([step(1, [1])], { sheet: 2, part: 1 }, [0, 30, 40]);
    expect(along).toMatchObject({ id: 0, name: 'Step 2', distance: 50 });
    expect(along.direction).toEqual([0, 0.6, 0.8]);
    expect(directionId(along.direction)).toBeNull();
    const up = newStep([], { sheet: 2, part: 1 }, undefined);
    expect(up).toMatchObject({ name: 'Step 1', direction: [0, 0, 1], distance: 50 });
    expect(directionId(up.direction)).toBe('+z');
  });

  it('adds a piece once, takes it out, and drops a step left empty', () => {
    const steps = [step(1, [1]), step(2, [2])];
    const added = withPiece(steps, 0, { sheet: 3, part: 1 });
    expect(added[0]?.pieces.map((p) => p.sheet)).toEqual([1, 3]);
    expect(withPiece(added, 0, { sheet: 3, part: 1 })).toEqual(added);
    expect(withoutPiece(added, 0, '1:1')[0]?.pieces.map((p) => p.sheet)).toEqual([3]);
    expect(withoutPiece(steps, 1, '2:1').map((s) => s.id)).toEqual([1]);
  });

  it('moves a step up and down the list, staying inside it', () => {
    const steps = [step(1, [1]), step(2, [2]), step(3, [3])];
    expect(moveStep(steps, 2, -1).map((s) => s.id)).toEqual([1, 3, 2]);
    expect(moveStep(steps, 0, -1).map((s) => s.id)).toEqual([1, 2, 3]);
    expect(moveStep(steps, 0, 1).map((s) => s.id)).toEqual([2, 1, 3]);
  });
});
