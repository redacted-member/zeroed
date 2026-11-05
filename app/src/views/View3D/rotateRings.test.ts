import { describe, expect, it } from 'vitest';

import {
  axisOffsetAfter,
  angleBetween,
  freeTurn,
  onRingPlane,
  ringAngle,
  ringFrame,
  ringPoints,
  toJob,
  toScene,
  turnedAbout,
  turnsMatrix,
  turnsOf,
  type Mat3,
  type Turns,
  type Vec3,
} from './rotateRings';

function apply(m: Mat3, v: Vec3): Vec3 {
  return [
    m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
    m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
    m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
  ];
}

function close(a: Vec3, b: Vec3): void {
  for (let k = 0; k < 3; k += 1) {
    expect(a[k]).toBeCloseTo(b[k] ?? 0, 6);
  }
}

describe('rotation rings', () => {
  it('turns about X, then Y, then Z like the kernel', () => {
    const m = turnsMatrix({ rotX: 90, rotY: 0, rotZ: 90 });
    // Y goes to Z under X, then Z stays under the Z turn.
    close(apply(m, [0, 1, 0]), [0, 0, 1]);
    // X stays under X, then goes to Y under Z.
    close(apply(m, [1, 0, 0]), [0, 1, 0]);
  });

  it('reads the three turns back out of a matrix', () => {
    const cases: Turns[] = [
      { rotX: 10, rotY: 20, rotZ: 30 },
      { rotX: -120, rotY: 45, rotZ: 170 },
      { rotX: 0, rotY: 0, rotZ: 0 },
    ];
    for (const turns of cases) {
      const back = turnsOf(turnsMatrix(turns));
      close([back.rotX, back.rotY, back.rotZ], [turns.rotX, turns.rotY, turns.rotZ]);
    }
    // A quarter turn about Y keeps the same matrix, whatever angles come back.
    const locked = turnsMatrix({ rotX: 30, rotY: 90, rotZ: 0 });
    const back = turnsMatrix(turnsOf(locked));
    for (let r = 0; r < 3; r += 1) {
      close(back[r] as Vec3, locked[r] as Vec3);
    }
  });

  it('turns about a job axis after the turns already made', () => {
    const start: Turns = { rotX: 30, rotY: 0, rotZ: 0 };
    const turned = turnedAbout(start, [0, 0, 1], Math.PI / 2);
    expect(turned.rotZ).toBeCloseTo(90, 6);
    expect(turned.rotX).toBeCloseTo(30, 6);
    // About X from rest is just the X turn.
    expect(turnedAbout({ rotX: 0, rotY: 0, rotZ: 0 }, [1, 0, 0], -Math.PI / 4).rotX).toBeCloseTo(
      -45,
      6,
    );
    // About X after a Z turn is a turn about job X, not the model's own X.
    const after = turnsMatrix(turnedAbout({ rotX: 0, rotY: 0, rotZ: 90 }, [1, 0, 0], Math.PI / 2));
    close(apply(after, [1, 0, 0]), [0, 0, 1]);
  });

  it('frames the model and lays the rings round its middle', () => {
    // Scene positions of a box 0..10 x 0..20 x 0..4 (job), as (x, z, -y).
    const corners: Vec3[] = [
      [0, 0, 0],
      [10, 20, 4],
    ];
    const positions = corners.flatMap((p) => [...toScene(p)]);
    const frame = ringFrame(positions);
    expect(frame).not.toBeNull();
    if (frame === null) {
      return;
    }
    close(frame.centre, [5, 10, 2]);
    expect(frame.radius).toBeCloseTo(Math.hypot(10, 20, 4) * 0.6, 6);
    const ring = ringPoints(frame, 'z', 8);
    expect(ring).toHaveLength(9);
    for (const p of ring) {
      expect(p[2]).toBeCloseTo(2, 9);
    }
    close(toJob(toScene([1, 2, 3])), [1, 2, 3]);
    expect(ringFrame([])).toBeNull();
  });

  it('measures angles round a ring from a ray', () => {
    const frame = { centre: [0, 0, 0] as Vec3, radius: 10 };
    const hit = onRingPlane(frame, 'z', [5, 5, 10], [0, 0, -1]);
    expect(hit).not.toBeNull();
    if (hit === null) {
      return;
    }
    close(hit, [5, 5, 0]);
    expect(ringAngle(frame, 'z', hit)).toBeCloseTo(Math.PI / 4, 9);
    // Right handed about X: Y towards Z.
    expect(ringAngle(frame, 'x', [0, 0, 1])).toBeCloseTo(Math.PI / 2, 9);
    expect(onRingPlane(frame, 'z', [0, 0, 5], [1, 0, 0])).toBeNull();
    expect(angleBetween(3, -3)).toBeCloseTo(2 * Math.PI - 6, 9);
  });

  it('turns freely from a drag on the ball', () => {
    const rest: Turns = { rotX: 0, rotY: 0, rotZ: 0 };
    // Seen from above (right = X, up = Y), across turns about Y.
    const across = freeTurn(rest, [1, 0, 0], [0, 1, 0], 90, 0, Math.PI / 180);
    expect(across.rotY).toBeCloseTo(90, 6);
    const still = freeTurn(rest, [1, 0, 0], [0, 1, 0], 0, 0, 0.01);
    expect(still).toEqual(rest);
  });
});

describe('the rotation axis cube', () => {
  it('moves the model back by the drag across the axis and up', () => {
    expect(axisOffsetAfter([1, 2], [3, 4, 5], 'x')).toEqual([-4, -2]);
    expect(axisOffsetAfter([1, 2], [3, 4, 5], 'y')).toEqual([-2, -2]);
  });
});
