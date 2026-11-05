// The 3D Segmenting plane's handles: an arrow along the plane's normal that
// moves it, and two knobs on the plane that tilt it about its own axes.
// Pure maths in job space (x, y, z up); the view projects the points and
// turns pointer rays into new planes.

import type { CutPlane, Vec3 } from '../../bridge/meshImport';
import { planeNormal } from './importPreview';

export type PlaneGrip = 'move' | 'tiltU' | 'tiltV';

function add(a: Vec3, b: Vec3, k = 1): Vec3 {
  return [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k];
}

function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function cross(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

function unit(a: Vec3): Vec3 {
  const l = Math.hypot(a[0], a[1], a[2]);
  return l > 0 ? [a[0] / l, a[1] / l, a[2] / l] : a;
}

/** The plane's normal and two axes lying on it, as its preview square
 * uses them. */
export function planeAxes(plane: CutPlane): { n: Vec3; u: Vec3; v: Vec3 } {
  const n = planeNormal(plane);
  const helper: Vec3 = Math.abs(n[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0];
  const u = unit(cross(n, helper));
  const v = unit(cross(n, u));
  return { n, u, v };
}

/** The angles (about X, then Y, degrees) that turn straight up to `n`. */
export function anglesOf(n: Vec3): { angleX: number; angleY: number } {
  const m = unit(n);
  const deg = 180 / Math.PI;
  return {
    angleX: Math.round(Math.asin(Math.max(-1, Math.min(1, -m[1]))) * deg * 1000) / 1000,
    angleY: Math.round(Math.atan2(m[0], m[2]) * deg * 1000) / 1000,
  };
}

/** Where the handles sit: the plane's point, the arrow's tip, and the two
 * tilt knobs, `size` from the point. */
export function handlePoints(
  plane: CutPlane,
  size: number,
): { centre: Vec3; move: Vec3; tiltU: Vec3; tiltV: Vec3 } {
  const { n, u, v } = planeAxes(plane);
  const centre = plane.point;
  return {
    centre,
    move: add(centre, n, size),
    tiltU: add(centre, u, size),
    tiltV: add(centre, v, size),
  };
}

/** How far along `dir` from `origin` the line through `point` along `axis`
 * passes closest to the ray, as a distance along `axis`. */
function alongAxis(point: Vec3, axis: Vec3, origin: Vec3, dir: Vec3): number | null {
  const w = sub(point, origin);
  const b = dot(axis, dir);
  const d = dot(axis, w);
  const e = dot(dir, w);
  const denom = 1 - b * b;
  if (Math.abs(denom) < 1e-9) {
    return null;
  }
  return (b * e - d) / denom;
}

/** Where a ray meets the plane through `point` square to `normal`. */
function meet(point: Vec3, normal: Vec3, origin: Vec3, dir: Vec3): Vec3 | null {
  const denom = dot(normal, dir);
  if (Math.abs(denom) < 1e-9) {
    return null;
  }
  const t = dot(normal, sub(point, origin)) / denom;
  return add(origin, dir, t);
}

/** The plane when `grip` is dragged along a pointer ray. `start` is the
 * plane when the drag began and `from` where along the normal the drag
 * began (from [`gripStart`]). */
export function dragPlane(
  start: CutPlane,
  grip: PlaneGrip,
  from: number,
  ray: { origin: Vec3; direction: Vec3 },
): CutPlane | null {
  const { n, u, v } = planeAxes(start);
  const dir = unit(ray.direction);
  if (grip === 'move') {
    const t = alongAxis(start.point, n, ray.origin, dir);
    return t === null ? null : { ...start, point: add(start.point, n, t - from) };
  }
  // Tilting: the knob follows the pointer round the plane's other axis.
  const hinge = grip === 'tiltU' ? v : u;
  const hit = meet(start.point, hinge, ray.origin, dir);
  if (hit === null) {
    return null;
  }
  const w = sub(hit, start.point);
  if (Math.hypot(w[0], w[1], w[2]) < 1e-9) {
    return null;
  }
  const knob = unit(w);
  const normal = grip === 'tiltU' ? cross(knob, v) : cross(u, knob);
  return { ...start, ...anglesOf(normal) };
}

/** Where along the normal a move drag starts, for [`dragPlane`]. */
export function gripStart(plane: CutPlane, ray: { origin: Vec3; direction: Vec3 }): number {
  const { n } = planeAxes(plane);
  return alongAxis(plane.point, n, ray.origin, unit(ray.direction)) ?? 0;
}
