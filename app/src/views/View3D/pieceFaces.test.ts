import { describe, expect, it } from 'vitest';

import type { Triple } from '../../bridge/assembly';
import { floatHeights } from '../../bridge/heights';
import { boxPlanes, faceRegion, faceSnap, piecePlanes, type SnapPiece } from './pieceFaces';
import { pieceBox } from './pieceGizmo';

/** A 10 x 10 mm, 10 mm thick relief on 1 mm cells with a 4 x 4 pocket 4 mm deep at (3, 3). */
function pocketed() {
  const z = new Float32Array(100);
  for (let y = 0; y < 10; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      const inPocket = x >= 3 && x < 7 && y >= 3 && y < 7;
      z[y * 10 + x] = inPocket ? 6 : 10;
    }
  }
  return {
    width: 10,
    height: 10,
    cellSize: 1,
    bottom: 0,
    z: floatHeights(z),
    floor: 'flat' as const,
  };
}

/** The total area of a triangle list. */
function area(triangles: Float32Array): number {
  let sum = 0;
  for (let i = 0; i + 8 < triangles.length; i += 9) {
    const t = (k: number) => triangles[i + k] ?? 0;
    const u = [t(3) - t(0), t(4) - t(1), t(5) - t(2)];
    const v = [t(6) - t(0), t(7) - t(1), t(8) - t(2)];
    const c = [
      (u[1] ?? 0) * (v[2] ?? 0) - (u[2] ?? 0) * (v[1] ?? 0),
      (u[2] ?? 0) * (v[0] ?? 0) - (u[0] ?? 0) * (v[2] ?? 0),
      (u[0] ?? 0) * (v[1] ?? 0) - (u[1] ?? 0) * (v[0] ?? 0),
    ];
    sum += Math.hypot(c[0] ?? 0, c[1] ?? 0, c[2] ?? 0) / 2;
  }
  return sum;
}

describe('faceRegion', () => {
  it('lights the flat top round the point, not the pocket floor below it', () => {
    const field = pocketed();
    expect(area(faceRegion(field, { point: [1, 1, 10], normal: [0, 0, 1] }))).toBeCloseTo(84, 6);
    expect(area(faceRegion(field, { point: [4.5, 4.5, 6], normal: [0, 0, 1] }))).toBeCloseTo(16, 6);
    expect(area(faceRegion(field, { point: [5, 5, 0], normal: [0, 0, -1] }))).toBeCloseTo(100, 6);
  });

  it('lights a wall from where the material starts to where it ends', () => {
    const field = pocketed();
    // The outside right wall: 10 long, 10 tall.
    const right = faceRegion(field, { point: [10, 5, 5], normal: [1, 0, 0] });
    expect(area(right)).toBeCloseTo(100, 6);
    // Every corner sits just off the wall.
    for (let i = 0; i < right.length; i += 3) {
      expect(right[i]).toBeCloseTo(10.05, 6);
    }
    // The pocket's left side: 4 long, from the pocket floor up to the top.
    const side = faceRegion(field, { point: [3, 5, 8], normal: [1, 0, 0] });
    expect(area(side)).toBeCloseTo(16, 6);
  });

  it('lights nothing where there is no material', () => {
    const field = pocketed();
    expect(faceRegion(field, { point: [5, 5, 20], normal: [0, 0, 1] }).length).toBe(0);
  });
});

describe('piecePlanes', () => {
  it('finds the box sides, the pocket floor and the pocket walls', () => {
    const planes = piecePlanes(pocketed());
    const has = (axis: number, sign: number, offset: number) =>
      planes.some((p) => p.axis === axis && p.sign === sign && Math.abs(p.offset - offset) < 1e-6);
    expect(has(0, -1, 0) && has(0, 1, 10) && has(1, -1, 0) && has(1, 1, 10)).toBe(true);
    expect(has(2, -1, 0) && has(2, 1, 10)).toBe(true);
    expect(has(2, 1, 6)).toBe(true);
    expect(has(0, 1, 3) && has(0, -1, 7) && has(1, 1, 3) && has(1, -1, 7)).toBe(true);
  });
});

function snapPiece(
  position: Triple,
  rotation: Triple,
  size: Triple,
  planes = piecePlanes(pocketed()),
): SnapPiece {
  const piece = { position, rotation, size };
  return { piece, box: pieceBox(piece), planes };
}

describe('faceSnap', () => {
  const size: Triple = [10, 10, 10];

  it('lays a piece hovering just above another on its top', () => {
    const base = snapPiece([5, 5, 5], [0, 0, 0], size);
    const above = snapPiece([5, 5, 15.4], [0, 0, 0], size);
    const shift = faceSnap(above, [base], 2, 1);
    expect(shift[2]).toBeCloseTo(-0.4, 6);
    expect(faceSnap(above, [base], 2, 0.2)).toEqual([0, 0, 0]);
  });

  it('snaps down into line with a pocket floor, not only the box top', () => {
    const base = snapPiece([5, 5, 5], [0, 0, 0], size);
    // Its underside 0.3 above the pocket floor (at 6).
    const shelf = snapPiece([5, 5, 11.3], [0, 0, 0], size);
    expect(faceSnap(shelf, [base], 2, 0.5)[2]).toBeCloseTo(-0.3, 6);
  });

  it('lines sides up with the ball and leaves pieces far off to the side', () => {
    const plain: Triple = [100, 50, 10];
    const base = snapPiece([0, 0, 0], [0, 0, 0], plain, boxPlanes(plain));
    // Touching the base's right side is 1 mm away; the other sides line up.
    const beside = snapPiece([101, 1, 0], [0, 0, 0], plain, boxPlanes(plain));
    const shift = faceSnap(beside, [base], null, 2);
    expect(shift[0]).toBeCloseTo(-1, 6);
    expect(shift[1]).toBeCloseTo(-1, 6);
    expect(shift[2]).toBeCloseTo(0, 6);
    const far = snapPiece([0, 300, 11], [0, 0, 0], plain, boxPlanes(plain));
    expect(faceSnap(far, [base], 2, 2)).toEqual([0, 0, 0]);
  });

  it('meets turned faces along their normal with the ball', () => {
    const angle = (30 * Math.PI) / 180;
    const along = (d: number): Triple => [d * Math.cos(angle), d * Math.sin(angle), 5];
    const first = snapPiece(along(0), [0, 0, 30], size);
    const second = snapPiece(along(10.5), [0, 0, 30], size);
    const shift = faceSnap(second, [first], null, 1);
    expect(shift[0]).toBeCloseTo(-0.5 * Math.cos(angle), 6);
    expect(shift[1]).toBeCloseTo(-0.5 * Math.sin(angle), 6);
    expect(shift[2]).toBeCloseTo(0, 6);
  });
});
