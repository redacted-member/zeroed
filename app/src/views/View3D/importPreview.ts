// What the 3D view draws while the model import flow is open: the model's
// triangles coloured by step (undercuts, the part below the zero plane,
// segments), and a see-through plane (the zero plane, or the segmenting
// plane). Scene axes follow the renderer (x -> x, job y -> -z, job z -> y)
// placed as the model view places the relief: the zero plane down in the
// material with the top at the material top (scene y zero), or for a model
// taller than the material, the zero plane at the material bottom.

import type { CutPlane, MeshPreview, Vec3 } from '../../bridge/meshImport';
import type { JobSetup } from '../../bridge/types';
import { jobBounds } from '../View2D/camera';

type Rgb = readonly [number, number, number];

const BASE: Rgb = [0.8, 0.74, 0.64];
const UNDERCUT: Rgb = [0.42, 0.16, 0.55];
const LAYOUT_UNDERCUT: Rgb = [0.93, 0.45, 0.66];
const BELOW: Rgb = [0.42, 0.42, 0.44];
const SEGMENT_COLOURS: readonly Rgb[] = [
  [0.86, 0.62, 0.36],
  [0.42, 0.66, 0.86],
  [0.55, 0.8, 0.45],
  [0.88, 0.5, 0.5],
  [0.72, 0.6, 0.88],
  [0.9, 0.82, 0.4],
  [0.45, 0.8, 0.78],
  [0.8, 0.55, 0.75],
];

export interface ImportPreviewOptions {
  step: 'orientate' | 'position' | 'segment';
  undercuts: boolean;
  /** Position step: the zero plane, mm above the model bottom. */
  zeroPlane: number | null;
  discardBelow: boolean;
  /** Segment step: segments laid out. */
  layout: boolean;
  /** Segment step: the chosen tree node id. */
  selected: string;
  /** Segment step, edit mode: the plane of the next cut. */
  plane: CutPlane | null;
  /** Material thickness, mm: a model taller than it stands out above the top. */
  thickness: number;
  /** The material rectangle, job mm: minX, minY, maxX, maxY; null for none. */
  material: readonly [number, number, number, number] | null;
  /** A rotary job: its axis and blank radius, for a model laid on the axis. */
  rotary?: { axis: 'x' | 'y'; radius: number } | null;
}

export interface ImportScene {
  /** Nine floats per triangle, scene space. */
  positions: Float32Array;
  /** Three floats per vertex. */
  colours: Float32Array;
  /** Two triangles of the plane to show, scene space, or null. */
  plane: Float32Array | null;
  /** Line segments outlining the material block, scene space, or null. */
  outline: Float32Array | null;
  /** Draw only the triangle edges. */
  wireframe?: boolean;
  /** Rotary: line segments of the cylinder round the model (blue) and the
   * rotation axis (red), scene space. */
  guides?: { cylinder: Float32Array; axis: Float32Array };
  /** 3D Segmenting, edit mode: how far the scene lowers the model (job Z
   * is model Z less this) and how long the plane's handles are. */
  planeHandle?: { top: number; size: number };
}

/** Whether a segment shows while tree node `selected` is chosen. */
export function segmentShown(leaves: readonly string[], selected: string): boolean {
  return leaves.some((leaf) => leaf.startsWith(selected) || selected.startsWith(leaf));
}

function colourFor(
  k: number,
  preview: MeshPreview,
  options: ImportPreviewOptions,
  bottom: number,
): Rgb | null {
  const undercut = ((preview.flags[k] ?? 0) & 1) === 1;
  if (options.step === 'segment') {
    const index = preview.segment[k] ?? 0;
    const segment = preview.segments[index];
    if (
      !options.layout &&
      segment !== undefined &&
      !segmentShown(segment.leaves, options.selected)
    ) {
      return null;
    }
    if (options.layout && options.undercuts && undercut) {
      return LAYOUT_UNDERCUT;
    }
    return SEGMENT_COLOURS[index % SEGMENT_COLOURS.length] ?? BASE;
  }
  if (options.step === 'position' && options.zeroPlane !== null && options.discardBelow) {
    const z = Math.max(
      preview.xyz[k * 9 + 2] ?? 0,
      preview.xyz[k * 9 + 5] ?? 0,
      preview.xyz[k * 9 + 8] ?? 0,
    );
    if (z < bottom + options.zeroPlane - 1e-6) {
      return BELOW;
    }
  }
  if (options.undercuts && undercut) {
    return UNDERCUT;
  }
  return BASE;
}

function cross(a: Vec3, b: Vec3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

function unit(a: Vec3): Vec3 {
  const l = Math.hypot(a[0], a[1], a[2]);
  return l > 0 ? [a[0] / l, a[1] / l, a[2] / l] : a;
}

/** The normal of a cut plane, as the kernel turns it (about X, then Y). */
export function planeNormal(plane: CutPlane): Vec3 {
  const ax = (plane.angleX * Math.PI) / 180;
  const ay = (plane.angleY * Math.PI) / 180;
  // (0, 0, 1) turned about X, then about Y.
  const n: Vec3 = [0, -Math.sin(ax), Math.cos(ax)];
  return [
    n[0] * Math.cos(ay) + n[2] * Math.sin(ay),
    n[1],
    -n[0] * Math.sin(ay) + n[2] * Math.cos(ay),
  ];
}

/** A square of half side `r` on the plane through `point` with `normal`, job space. */
function square(point: Vec3, normal: Vec3, r: number): Vec3[] {
  const helper: Vec3 = Math.abs(normal[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0];
  const u = unit(cross(normal, helper));
  const v = unit(cross(normal, u));
  const at = (a: number, b: number): Vec3 => [
    point[0] + (u[0] * a + v[0] * b) * r,
    point[1] + (u[1] * a + v[1] * b) * r,
    point[2] + (u[2] * a + v[2] * b) * r,
  ];
  const [p, q, s, t] = [at(-1, -1), at(1, -1), at(1, 1), at(-1, 1)];
  return [p, q, s, p, s, t];
}

/** The twelve edges of the material block, scene space (top at y zero). */
export function blockOutline(
  material: readonly [number, number, number, number],
  thickness: number,
): Float32Array {
  const [x0, y0, x1, y1] = material;
  const corners = [
    [x0, y0],
    [x1, y0],
    [x1, y1],
    [x0, y1],
  ] as const;
  const out: number[] = [];
  for (let k = 0; k < 4; k += 1) {
    const [ax, ay] = corners[k] ?? [0, 0];
    const [bx, by] = corners[(k + 1) % 4] ?? [0, 0];
    for (const level of [0, -thickness]) {
      out.push(ax, level, -ay, bx, level, -by);
    }
    out.push(ax, 0, -ay, ax, -thickness, -ay);
  }
  return new Float32Array(out);
}

/** Scene position of a point of a model laid on a rotary axis: `along`,
 * `side` and `up` round the axis, which sits `radius` below the top of the
 * blank, as the wrapped model shows it. */
export function rotaryToScene(
  axis: 'x' | 'y',
  radius: number,
  along: number,
  side: number,
  up: number,
): [number, number, number] {
  return axis === 'x' ? [along, up - radius, side] : [side, up - radius, -along];
}

/** A model laid on a rotary axis, with the cylinder round it and the axis. */
function rotaryScene(
  preview: MeshPreview,
  cylinder: NonNullable<MeshPreview['cylinder']>,
  rotary: { axis: 'x' | 'y'; radius: number },
): ImportScene {
  const count = Math.floor(preview.xyz.length / 9);
  const positions = new Float32Array(count * 9);
  const colours = new Float32Array(count * 9);
  let lo = Infinity;
  let hi = -Infinity;
  for (let v = 0; v < count * 3; v += 1) {
    const along = preview.xyz[v * 3] ?? 0;
    lo = Math.min(lo, along);
    hi = Math.max(hi, along);
    const p = rotaryToScene(
      rotary.axis,
      rotary.radius,
      along,
      preview.xyz[v * 3 + 1] ?? 0,
      preview.xyz[v * 3 + 2] ?? 0,
    );
    positions.set(p, v * 3);
    colours.set(BASE, v * 3);
  }
  if (count === 0) {
    lo = 0;
    hi = 0;
  }
  const r = cylinder.diameter / 2;
  const at = (along: number, angle: number) =>
    rotaryToScene(rotary.axis, rotary.radius, along, r * Math.sin(angle), r * Math.cos(angle));
  const ring: number[] = [];
  const steps = 48;
  for (const along of [lo, hi]) {
    for (let k = 0; k < steps; k += 1) {
      ring.push(
        ...at(along, (k / steps) * Math.PI * 2),
        ...at(along, ((k + 1) / steps) * Math.PI * 2),
      );
    }
  }
  for (let k = 0; k < 8; k += 1) {
    const angle = (k / 8) * Math.PI * 2;
    ring.push(...at(lo, angle), ...at(hi, angle));
  }
  const axisLine = [
    ...rotaryToScene(rotary.axis, rotary.radius, lo, 0, 0),
    ...rotaryToScene(rotary.axis, rotary.radius, hi, 0, 0),
  ];
  return {
    positions,
    colours,
    plane: null,
    outline: null,
    guides: { cylinder: new Float32Array(ring), axis: new Float32Array(axisLine) },
  };
}

/** Builds the scene data for a preview. */
export function importScene(preview: MeshPreview, options: ImportPreviewOptions): ImportScene {
  const rotary = options.rotary ?? null;
  if (preview.cylinder !== undefined && rotary !== null) {
    return rotaryScene(preview, preview.cylinder, rotary);
  }
  const count = Math.floor(preview.xyz.length / 9);
  let lo = Infinity;
  let hi = -Infinity;
  const box = [Infinity, Infinity, -Infinity, -Infinity];
  for (let i = 0; i < count * 3; i += 1) {
    const x = preview.xyz[i * 3] ?? 0;
    const y = preview.xyz[i * 3 + 1] ?? 0;
    const z = preview.xyz[i * 3 + 2] ?? 0;
    lo = Math.min(lo, z);
    hi = Math.max(hi, z);
    box[0] = Math.min(box[0] ?? x, x);
    box[1] = Math.min(box[1] ?? y, y);
    box[2] = Math.max(box[2] ?? x, x);
    box[3] = Math.max(box[3] ?? y, y);
  }
  if (count === 0) {
    lo = 0;
    hi = 0;
  }
  // The zero plane is at the bottom of the model, or where Position puts it.
  const zero =
    lo + (options.step === 'position' && options.zeroPlane !== null ? options.zeroPlane : 0);
  const top = zero + Math.min(hi - zero, options.thickness);
  const positions: number[] = [];
  const colours: number[] = [];
  const toScene = (x: number, y: number, z: number) => positions.push(x, z - top, -y);
  for (let k = 0; k < count; k += 1) {
    const colour = colourFor(k, preview, options, lo);
    if (colour === null) {
      continue;
    }
    for (let v = 0; v < 3; v += 1) {
      const at = k * 9 + v * 3;
      toScene(preview.xyz[at] ?? 0, preview.xyz[at + 1] ?? 0, preview.xyz[at + 2] ?? 0);
      colours.push(colour[0], colour[1], colour[2]);
    }
  }
  let plane: Float32Array | null = null;
  const [minX = 0, minY = 0, maxX = 0, maxY = 0] = box;
  const r = Math.max(maxX - minX, maxY - minY, hi - lo) * 0.6 + 1;
  const mid: Vec3 = [(minX + maxX) / 2, (minY + maxY) / 2, 0];
  let corners: Vec3[] | null = null;
  if (options.step === 'position' && options.zeroPlane !== null && count > 0) {
    corners = square([mid[0], mid[1], lo + options.zeroPlane], [0, 0, 1], r);
  } else if (options.step === 'segment' && !options.layout && options.plane !== null) {
    corners = square(options.plane.point, planeNormal(options.plane), r);
  }
  if (corners !== null) {
    plane = new Float32Array(corners.flatMap(([x, y, z]) => [x, z - top, -y]));
  }
  return {
    positions: new Float32Array(positions),
    colours: new Float32Array(colours),
    plane,
    outline: options.material === null ? null : blockOutline(options.material, options.thickness),
    ...(options.step === 'segment' && !options.layout && options.plane !== null
      ? { planeHandle: { top, size: r * 0.5 } }
      : {}),
  };
}

/** The job's material rectangle in job space. */
export function materialRect(job: JobSetup): [number, number, number, number] {
  const b = jobBounds(job);
  return [b.minX, b.minY, b.maxX, b.maxY];
}

/** The scene positions of the triangles of segment `id` (layout mode, where
 * every triangle shows in order). */
export function segmentPositions(
  positions: Float32Array,
  preview: MeshPreview,
  id: string,
): Float32Array {
  const index = preview.segments.findIndex((s) => s.id === id);
  const out: number[] = [];
  const count = Math.floor(positions.length / 9);
  for (let k = 0; k < count; k += 1) {
    if (preview.segment[k] === index) {
      for (let i = 0; i < 9; i += 1) {
        out.push(positions[k * 9 + i] ?? 0);
      }
    }
  }
  return new Float32Array(out);
}
