import { describe, expect, it } from 'vitest';

import type { DropLanding, PieceRef, Triple } from '../../bridge/assembly';
import {
  groundPoint,
  landingPoint,
  pieceDropAt,
  registerPieceDrop,
  restingPosition,
  type PieceDropResolver,
} from './pieceDrop';

const close = (a: readonly number[], b: readonly number[]) =>
  a.forEach((v, i) => expect(v).toBeCloseTo(b[i] ?? NaN, 9));

describe('dropping a piece on the 3D view', () => {
  it('asks the registered surface under the pointer, or its nearest registered ancestor', () => {
    const view = { parentElement: null } as unknown as Element;
    const canvas = { parentElement: view } as unknown as Element;
    const elsewhere = { parentElement: null } as unknown as Element;
    const resolve: PieceDropResolver = (x, y) => ({ point: [x, y, 0], onto: null });
    const registry = new Map([[view, resolve]]);
    expect(pieceDropAt(3, 4, () => canvas, registry)).toEqual({ point: [3, 4, 0], onto: null });
    expect(pieceDropAt(3, 4, () => elsewhere, registry)).toBeNull();
    expect(pieceDropAt(3, 4, () => null, registry)).toBeNull();
  });

  it('forgets a surface when it unregisters', () => {
    const canvas = { parentElement: null } as unknown as Element;
    const landing: DropLanding = { point: [1, 2, 3], onto: null };
    const undo = registerPieceDrop(canvas, () => landing);
    expect(pieceDropAt(0, 0, () => canvas)).toBe(landing);
    undo();
    expect(pieceDropAt(0, 0, () => canvas)).toBeNull();
  });

  it('meets the ground in front of the eye only', () => {
    expect(groundPoint({ origin: [0, 0, 100], direction: [0.6, 0, -0.8] })).toEqual([75, 0, 0]);
    expect(groundPoint({ origin: [0, 0, 100], direction: [0, 0, 1] })).toBeNull();
    expect(groundPoint({ origin: [0, 0, 100], direction: [1, 0, 0] })).toBeNull();
  });

  it('lands on the piece under the pointer, else the ground, never on the piece being moved', () => {
    const ray = { origin: [0, 0, 10] as Triple, direction: [0, 0, -1] as Triple };
    const onTop = { key: 'c5', point: [1, 2, 18] as Triple };
    const picker = { pickPiece: () => onTop, jobRay: () => ray };
    const five: PieceRef = { sheet: 1, part: 1, component: 5 };
    const refOf = (key: string) => (key === 'c5' ? five : null);
    expect(landingPoint(picker, 0, 0, null, refOf)).toEqual({ point: [1, 2, 18], onto: five });
    const ground = { point: [0, 0, 0], onto: null };
    expect(landingPoint(picker, 0, 0, 'c5', refOf)).toEqual(ground);
    expect(landingPoint(picker, 0, 0, null, () => null)).toEqual(ground);
    const missing = { pickPiece: () => null, jobRay: () => ray };
    expect(landingPoint(missing, 0, 0, null, refOf)).toEqual(ground);
    const sky = {
      pickPiece: () => null,
      jobRay: () => ({ ...ray, direction: [0, 0, 1] as Triple }),
    };
    expect(landingPoint(sky, 0, 0, null, refOf)).toBeNull();
  });

  it('rests the middle of the underside on the point, keeping the turns', () => {
    const flat = {
      position: [0, 0, 0] as Triple,
      rotation: [0, 0, 0] as Triple,
      size: [40, 20, 6] as Triple,
    };
    close(restingPosition(flat, [100, 50, 12]), [100, 50, 15]);
    // Stood on its end: the 40 mm side runs up Z.
    const upright = { ...flat, position: [7, 7, 7] as Triple, rotation: [0, 90, 0] as Triple };
    close(restingPosition(upright, [0, 0, 0]), [0, 0, 20]);
  });
});
