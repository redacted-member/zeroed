import { describe, expect, it } from 'vitest';

import type { Triple } from '../../bridge/assembly';
import {
  alongAxis,
  apply4,
  movedAlong,
  movedByScene,
  pieceMatrix,
  pieceSceneMatrix,
  piecesBox,
  ringsOf,
  tidy,
  toPieceLocal,
  turnedBy,
} from './pieceGizmo';

const close = (a: readonly number[], b: readonly number[]) =>
  a.forEach((v, i) => expect(v).toBeCloseTo(b[i] ?? NaN, 6));

const piece = (position: Triple, rotation: Triple = [0, 0, 0]) => ({
  position,
  rotation,
  size: [100, 50, 10] as Triple,
});

describe('pieceMatrix', () => {
  it('puts the middle of the material at the position', () => {
    close(apply4(pieceMatrix(piece([10, 20, 30])), [50, 25, 5]), [10, 20, 30]);
  });

  it('turns about the middle, as the kernel does', () => {
    const m = pieceMatrix(piece([10, 20, 30], [0, 0, 90]));
    close(apply4(m, [50, 25, 5]), [10, 20, 30]);
    // The material's X axis now runs along Y.
    close(apply4(m, [100, 25, 5]), [10, 70, 30]);
  });
});

describe('pieceSceneMatrix', () => {
  // A 100 x 50 x 10 sheet, datum bottom left, Z zero on top: cells of 1 mm.
  const field = { cellSize: 1, originX: 0.5, originY: 0.5, top: 0, bottom: -10 };

  it('lays the uncut top corner of the relief mesh where the piece sits', () => {
    const m = pieceSceneMatrix(piece([50, 25, 5]), field);
    // Mesh point (x 0, height 0, -y 0) is the material's top corner: job
    // (0, 0, 10), scene (0, 10, 0).
    close(apply4(m, [0, 0, 0]), [0, 10, 0]);
    // Mesh (100, -10, -50): the far bottom corner, job (100, 50, 0).
    close(apply4(m, [100, -10, -50]), [100, 0, -50]);
  });

  it('follows a stood up piece', () => {
    const m = pieceSceneMatrix(piece([0, 0, 50], [90, 0, 0]), field);
    // Turned about X: the material's Y runs up, its top faces -Y.
    const topCentre = apply4(m, [50, 0, -25]);
    close(topCentre, [0, 50, 5]);
  });
});

describe('piecesBox and rings', () => {
  it('spans every piece', () => {
    const box = piecesBox([piece([50, 25, 5]), piece([200, 25, 5])]);
    expect(box).not.toBeNull();
    close(box?.min ?? [], [0, 0, 0]);
    close(box?.max ?? [], [250, 50, 10]);
    expect(piecesBox([])).toBeNull();
  });

  it('centres the rings on the piece', () => {
    const frame = ringsOf(piece([1, 2, 3]));
    close(frame.centre, [1, 2, 3]);
    expect(frame.radius).toBeGreaterThan(0);
  });
});

describe('dragging', () => {
  it('finds how far along an axis the pointer is', () => {
    // A ray straight down through (30, 0, 100) meets the X axis at 30.
    expect(alongAxis([0, 0, 0], [1, 0, 0], [30, 0, 100], [0, 0, -1])).toBeCloseTo(30, 9);
    // A ray along the axis has no answer.
    expect(alongAxis([0, 0, 0], [1, 0, 0], [0, 0, 0], [1, 0, 0])).toBeNull();
  });

  it('moves along an axis and across the view', () => {
    expect(movedAlong([1, 2, 3], 'z', 5)).toEqual([1, 2, 8]);
    // Scene (x, z, -y): up in the scene is up in the job.
    expect(movedByScene([0, 0, 0], [1, 2, 3])).toEqual([1, -3, 2]);
  });

  it('turns about a job axis on top of the turns there already are', () => {
    close(turnedBy([0, 0, 0], 'x', 90), [90, 0, 0]);
    close(turnedBy([90, 0, 0], 'z', 90), [90, 0, 90]);
  });

  it('tidies values', () => {
    expect(tidy(1.23456)).toBe(1.235);
    expect(Object.is(tidy(-0.0001), 0)).toBe(true);
  });
});

describe('toPieceLocal', () => {
  it('undoes the pose', () => {
    const turned = piece([10, 20, 30], [30, -45, 120]);
    const local: Triple = [12, 34, 5];
    close(toPieceLocal(turned, apply4(pieceMatrix(turned), local)), local);
  });
});
