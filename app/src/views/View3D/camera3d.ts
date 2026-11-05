// Camera maths for the 3D view, free of three.js so it can be tested. The
// camera orbits a target: `theta` turns it about the vertical scene axis
// (0 looks from the front, that is from job -Y towards +Y) and `phi` is its
// angle down from straight above (0 looks down on the job, pi/2 looks level,
// pi looks up from below). Scene axes follow the renderer: job x -> x,
// job y -> -z, job z -> y.

export type Vec3 = readonly [number, number, number];

export type ViewPreset = 'top' | 'bottom' | 'front' | 'back' | 'left' | 'right' | 'iso';

export type ProjectionKind = 'perspective' | 'orthographic';

export interface OrbitAngles {
  theta: number;
  phi: number;
}

/** Everything that places the camera. */
export interface OrbitView extends OrbitAngles {
  /** Distance from the target, scene units (mm). */
  radius: number;
  target: Vec3;
}

export interface CameraBasis {
  /** Unit vector from the target towards the camera. */
  back: Vec3;
  /** Unit vector pointing right on screen. */
  right: Vec3;
  /** Unit vector pointing up on screen. */
  up: Vec3;
}

export const ISO_THETA = Math.PI * 0.25;
export const ISO_PHI = Math.PI * 0.32;
/** Vertical field of view of the perspective camera, degrees. */
export const FOV_DEG = 45;

export const VIEW_PRESETS: Readonly<Record<ViewPreset, OrbitAngles>> = {
  top: { theta: 0, phi: 0 },
  bottom: { theta: 0, phi: Math.PI },
  front: { theta: 0, phi: Math.PI / 2 },
  back: { theta: Math.PI, phi: Math.PI / 2 },
  left: { theta: -Math.PI / 2, phi: Math.PI / 2 },
  right: { theta: Math.PI / 2, phi: Math.PI / 2 },
  iso: { theta: ISO_THETA, phi: ISO_PHI },
};

/** Keeps `phi` inside straight up and straight down. */
export function clampPhi(phi: number): number {
  return Math.min(Math.PI, Math.max(0, phi));
}

/**
 * The camera's axes for an orbit angle. Right comes from `theta` alone, so
 * the basis stays well defined looking straight down or straight up.
 */
export function cameraBasis(theta: number, phi: number): CameraBasis {
  const sinPhi = Math.sin(phi);
  const cosPhi = Math.cos(phi);
  const back: Vec3 = [sinPhi * Math.sin(theta), cosPhi, sinPhi * Math.cos(theta)];
  const right: Vec3 = [Math.cos(theta), 0, -Math.sin(theta)];
  // up = back x right
  const up: Vec3 = [
    back[1] * right[2] - back[2] * right[1],
    back[2] * right[0] - back[0] * right[2],
    back[0] * right[1] - back[1] * right[0],
  ];
  return { back, right, up };
}

/** Where the camera sits for an orbit view. */
export function cameraPosition(view: OrbitView): Vec3 {
  const { back } = cameraBasis(view.theta, view.phi);
  const [x, y, z] = view.target;
  return [x + back[0] * view.radius, y + back[1] * view.radius, z + back[2] * view.radius];
}

/**
 * Half the height and width an orthographic camera shows so that, at the
 * target, it frames the same area as the perspective camera would.
 */
export function orthoHalfSize(
  radius: number,
  aspect: number,
  fovDeg = FOV_DEG,
): { halfWidth: number; halfHeight: number } {
  const halfHeight = radius * Math.tan(((fovDeg / 2) * Math.PI) / 180);
  return { halfWidth: halfHeight * Math.max(aspect, 1e-6), halfHeight };
}

/** An axis aligned box in scene units. */
export interface Box3 {
  min: Vec3;
  max: Vec3;
}

/**
 * A box in job space (x, y, z) as the scene holds it: scene (x, z, -y)
 * with the material top at scene height 0, so a job whose Z zero is at the
 * bottom (material top at job Z `surfaceZ`) sits `surfaceZ` lower.
 */
export function sceneBoxOf(
  box: { min: readonly [number, number, number]; max: readonly [number, number, number] },
  surfaceZ: number,
): Box3 {
  return {
    min: [box.min[0], box.min[2] - surfaceZ, -box.max[1]],
    max: [box.max[0], box.max[2] - surfaceZ, -box.min[1]],
  };
}

/** Centre and radius of the sphere around a box. */
export function boundingSphere(box: Box3): { centre: Vec3; radius: number } {
  const centre: Vec3 = [
    (box.min[0] + box.max[0]) / 2,
    (box.min[1] + box.max[1]) / 2,
    (box.min[2] + box.max[2]) / 2,
  ];
  const radius =
    Math.hypot(box.max[0] - box.min[0], box.max[1] - box.min[1], box.max[2] - box.min[2]) / 2;
  return { centre, radius };
}

/**
 * Orbit distance at which a sphere of `sphereRadius` fits the view with a
 * `margin` (1.1 leaves 10 percent). A perspective camera must see the sphere
 * inside both its vertical and horizontal field of view; an orthographic one
 * shows `orthoHalfSize(radius)`, which must cover the sphere both ways.
 */
export function fitRadius(
  sphereRadius: number,
  aspect: number,
  projection: ProjectionKind,
  margin = 1.1,
  fovDeg = FOV_DEG,
): number {
  const r = Math.max(sphereRadius, 1e-6) * margin;
  const tanHalfV = Math.tan(((fovDeg / 2) * Math.PI) / 180);
  const safeAspect = Math.max(aspect, 1e-6);
  const tanHalfH = tanHalfV * safeAspect;
  if (projection === 'orthographic') {
    return Math.max(r / tanHalfV, r / tanHalfH);
  }
  const halfV = Math.atan(tanHalfV);
  const halfH = Math.atan(tanHalfH);
  return r / Math.sin(Math.min(halfV, halfH));
}

/** The view that frames `box` from the given angles. */
export function fitView(
  box: Box3,
  angles: OrbitAngles,
  aspect: number,
  projection: ProjectionKind,
): OrbitView {
  const sphere = boundingSphere(box);
  return {
    theta: angles.theta,
    phi: angles.phi,
    radius: fitRadius(sphere.radius, aspect, projection),
    target: sphere.centre,
  };
}

/** `angle` moved by whole turns to lie within half a turn of `near`. */
export function nearestTurn(angle: number, near: number): number {
  const turn = Math.PI * 2;
  return angle - turn * Math.round((angle - near) / turn);
}

/** Smooth start and stop for t in 0..1. */
export function easeInOut(t: number): number {
  const c = Math.min(1, Math.max(0, t));
  return c * c * (3 - 2 * c);
}

/**
 * The view part way (t in 0..1) from `from` to `to`, turning the short way
 * round and scaling the distance geometrically so zooms feel even.
 */
export function blendView(from: OrbitView, to: OrbitView, t: number): OrbitView {
  const k = easeInOut(t);
  const theta = nearestTurn(to.theta, from.theta);
  const lerp = (a: number, b: number) => a + (b - a) * k;
  return {
    theta: lerp(from.theta, theta),
    phi: lerp(from.phi, to.phi),
    radius: from.radius * Math.pow(to.radius / from.radius, k),
    target: [
      lerp(from.target[0], to.target[0]),
      lerp(from.target[1], to.target[1]),
      lerp(from.target[2], to.target[2]),
    ],
  };
}
