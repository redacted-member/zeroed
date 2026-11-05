// The rotation rings the 3D view shows while a model is being orientated:
// one ring about each job axis (red X, green Y, blue Z) round the model's
// middle, and a ball in the middle that turns the model freely. Everything
// here is job space (X, Y, Z up); the renderer's scene is (x, z, -y).
//
// The model is turned as the kernel turns it: about X, then Y, then Z, so
// its matrix is Rz Ry Rx. A drag on a ring turns the model about that job
// axis, which is a turn applied after the others; the result goes back to
// the three angles the form shows.

export type RingAxis = 'x' | 'y' | 'z';

export type Vec3 = readonly [number, number, number];

/** A 3 by 3 matrix, rows first. */
export type Mat3 = readonly [Vec3, Vec3, Vec3];

/** The three turns of the Orientate Model form, degrees. */
export interface Turns {
  rotX: number;
  rotY: number;
  rotZ: number;
}

/** Where the rings sit: the middle of the model and their radius, mm. */
export interface RingFrame {
  centre: Vec3;
  radius: number;
}

export const RING_AXES: readonly RingAxis[] = ['x', 'y', 'z'];

const DEG = Math.PI / 180;
/** Points drawn round each ring. */
export const RING_POINTS = 72;
/** Rings reach this far past the model's middle, as a share of its diagonal. */
const RING_REACH = 0.6;

export function axisVector(axis: RingAxis): Vec3 {
  return axis === 'x' ? [1, 0, 0] : axis === 'y' ? [0, 1, 0] : [0, 0, 1];
}

/** A job point in the renderer's scene. */
export function toScene(p: Vec3): Vec3 {
  return [p[0], p[2], -p[1]];
}

/** A scene point in job space. */
export function toJob(p: Vec3): Vec3 {
  return [p[0], -p[2], p[1]];
}

/** The rings for a model whose scene positions (nine floats a triangle) are given. */
export function ringFrame(positions: ArrayLike<number>): RingFrame | null {
  if (positions.length < 3) {
    return null;
  }
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i + 2 < positions.length; i += 3) {
    const p = toJob([positions[i] ?? 0, positions[i + 1] ?? 0, positions[i + 2] ?? 0]);
    for (let k = 0; k < 3; k += 1) {
      lo[k] = Math.min(lo[k] ?? 0, p[k] ?? 0);
      hi[k] = Math.max(hi[k] ?? 0, p[k] ?? 0);
    }
  }
  const at = (k: number) => ((lo[k] ?? 0) + (hi[k] ?? 0)) / 2;
  const diagonal = Math.hypot(
    (hi[0] ?? 0) - (lo[0] ?? 0),
    (hi[1] ?? 0) - (lo[1] ?? 0),
    (hi[2] ?? 0) - (lo[2] ?? 0),
  );
  if (!Number.isFinite(diagonal) || diagonal <= 0) {
    return null;
  }
  return { centre: [at(0), at(1), at(2)], radius: diagonal * RING_REACH };
}

/** Two unit vectors spanning the plane a ring lies in, right handed about its axis. */
function ringBasis(axis: RingAxis): [Vec3, Vec3] {
  switch (axis) {
    case 'x':
      return [
        [0, 1, 0],
        [0, 0, 1],
      ];
    case 'y':
      return [
        [0, 0, 1],
        [1, 0, 0],
      ];
    case 'z':
      return [
        [1, 0, 0],
        [0, 1, 0],
      ];
  }
}

/** Job points round a ring, the last joining the first. */
export function ringPoints(frame: RingFrame, axis: RingAxis, count = RING_POINTS): Vec3[] {
  const [u, v] = ringBasis(axis);
  const out: Vec3[] = [];
  for (let i = 0; i <= count; i += 1) {
    const t = (i / count) * 2 * Math.PI;
    const c = Math.cos(t) * frame.radius;
    const s = Math.sin(t) * frame.radius;
    out.push([
      frame.centre[0] + u[0] * c + v[0] * s,
      frame.centre[1] + u[1] * c + v[1] * s,
      frame.centre[2] + u[2] * c + v[2] * s,
    ]);
  }
  return out;
}

/** The angle of `point` round a ring, radians, right handed about the axis. */
export function ringAngle(frame: RingFrame, axis: RingAxis, point: Vec3): number {
  const [u, v] = ringBasis(axis);
  const d: Vec3 = [
    point[0] - frame.centre[0],
    point[1] - frame.centre[1],
    point[2] - frame.centre[2],
  ];
  return Math.atan2(dot(d, v), dot(d, u));
}

/** The shortest turn from angle `a` to angle `b`, radians. */
export function angleBetween(a: number, b: number): number {
  let d = b - a;
  while (d > Math.PI) {
    d -= 2 * Math.PI;
  }
  while (d < -Math.PI) {
    d += 2 * Math.PI;
  }
  return d;
}

/** Where a ray meets the plane of a ring, or null when it runs along it. */
export function onRingPlane(
  frame: RingFrame,
  axis: RingAxis,
  origin: Vec3,
  direction: Vec3,
): Vec3 | null {
  const n = axisVector(axis);
  const along = dot(direction, n);
  if (Math.abs(along) < 1e-9) {
    return null;
  }
  const t =
    dot(
      [frame.centre[0] - origin[0], frame.centre[1] - origin[1], frame.centre[2] - origin[2]],
      n,
    ) / along;
  return [origin[0] + direction[0] * t, origin[1] + direction[1] * t, origin[2] + direction[2] * t];
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function multiply(a: Mat3, b: Mat3): Mat3 {
  const row = (r: Vec3): Vec3 => [
    r[0] * b[0][0] + r[1] * b[1][0] + r[2] * b[2][0],
    r[0] * b[0][1] + r[1] * b[1][1] + r[2] * b[2][1],
    r[0] * b[0][2] + r[1] * b[1][2] + r[2] * b[2][2],
  ];
  return [row(a[0]), row(a[1]), row(a[2])];
}

/** A turn of `radians` about the unit vector `axis` (right handed). */
export function turnMatrix(axis: Vec3, radians: number): Mat3 {
  const [x, y, z] = axis;
  const c = Math.cos(radians);
  const s = Math.sin(radians);
  const t = 1 - c;
  return [
    [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
    [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
    [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
  ];
}

/** The matrix of the three turns: about X, then Y, then Z. */
export function turnsMatrix(turns: Turns): Mat3 {
  const x = turnMatrix([1, 0, 0], turns.rotX * DEG);
  const y = turnMatrix([0, 1, 0], turns.rotY * DEG);
  const z = turnMatrix([0, 0, 1], turns.rotZ * DEG);
  return multiply(z, multiply(y, x));
}

/** A tidy angle for a field: rounded to a thousandth, within -180 to 180. */
function tidy(degrees: number): number {
  let d = degrees;
  while (d > 180) {
    d -= 360;
  }
  while (d <= -180) {
    d += 360;
  }
  const rounded = Math.round(d * 1000) / 1000;
  return Object.is(rounded, -0) ? 0 : rounded;
}

/** The three turns that make matrix `m`. */
export function turnsOf(m: Mat3): Turns {
  const sy = -m[2][0];
  if (Math.abs(sy) > 1 - 1e-9) {
    // Y a quarter turn: X and Z turn about the same line; keep Z at nought.
    const rotY = sy > 0 ? 90 : -90;
    const rotX = Math.atan2(sy * m[0][1], m[1][1]) / DEG;
    return { rotX: tidy(rotX), rotY, rotZ: 0 };
  }
  return {
    rotX: tidy(Math.atan2(m[2][1], m[2][2]) / DEG),
    rotY: tidy(Math.asin(Math.max(-1, Math.min(1, sy))) / DEG),
    rotZ: tidy(Math.atan2(m[1][0], m[0][0]) / DEG),
  };
}

/** The turns after a further turn of `radians` about the job direction `axis`. */
export function turnedAbout(turns: Turns, axis: Vec3, radians: number): Turns {
  return turnsOf(multiply(turnMatrix(axis, radians), turnsMatrix(turns)));
}

/** A free turn from a drag on the ball: across the screen turns about the
 * camera's up direction, up and down about its right, `perPixel` radians a
 * pixel. */
export function freeTurn(
  turns: Turns,
  right: Vec3,
  up: Vec3,
  dx: number,
  dy: number,
  perPixel: number,
): Turns {
  const across = turnMatrix(unit(up), dx * perPixel);
  const down = turnMatrix(unit(right), dy * perPixel);
  return turnsOf(multiply(down, multiply(across, turnsMatrix(turns))));
}

function unit(v: Vec3): Vec3 {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}

/** The form angle a ring edits. */
export function turnKey(axis: RingAxis): keyof Turns {
  return axis === 'x' ? 'rotX' : axis === 'y' ? 'rotY' : 'rotZ';
}

/**
 * The model's offset from the rotation axis (across, up) after the axis is
 * dragged by the scene space `delta` (mm) on a job whose axis runs along
 * `axis`: the model moves back by the drag, so the axis follows the pointer.
 */
export function axisOffsetAfter(
  offset: readonly [number, number],
  delta: Vec3,
  axis: 'x' | 'y',
): [number, number] {
  const across = axis === 'x' ? delta[2] : delta[0];
  return [offset[0] - across, offset[1] - delta[1]];
}
