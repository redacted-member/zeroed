// Seams of a furniture design in the Assembly View: which seam a pick ray
// passes nearest (hover and click), and the overall dimension lines drawn
// round the design. Pure maths in job space (X, Y, Z up), mm.

import type {
  ConnectionRow,
  DesignRow,
  FurnitureBox,
  FurnitureValues,
  Vec3,
} from '../../bridge/furniture';

/** A pick reaches a seam this close, mm (the kernel's furniture.at uses the same). */
export const SEAM_REACH = 8;

function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function along(a: Vec3, d: Vec3, t: number): Vec3 {
  return [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t];
}

function length(a: Vec3): number {
  return Math.sqrt(dot(a, a));
}

/**
 * The shortest distance between a ray (origin, unit direction, forward
 * only) and the segment from `p` to `q`, and how far along the ray that is.
 */
export function raySegmentDistance(
  origin: Vec3,
  direction: Vec3,
  p: Vec3,
  q: Vec3,
): { distance: number; at: number } {
  const u = direction;
  const v = sub(q, p);
  const w = sub(origin, p);
  const a = dot(u, u);
  const b = dot(u, v);
  const c = dot(v, v);
  const d = dot(u, w);
  const e = dot(v, w);
  const den = a * c - b * b;
  // Parameters along the ray (s >= 0) and the segment (0 <= t <= 1).
  let s = 0;
  let t = 0;
  if (c < 1e-12) {
    // The segment is a point.
    s = Math.max(0, -d / a);
    t = 0;
  } else {
    s = den < 1e-12 ? 0 : (b * e - c * d) / den;
    if (s < 0) {
      s = 0;
    }
    t = (b * s + e) / c;
    if (t < 0 || t > 1) {
      t = Math.min(1, Math.max(0, t));
      s = Math.max(0, (b * t - d) / a);
    }
  }
  const onRay = along(origin, u, s);
  const onSeg = along(p, v, t);
  return { distance: length(sub(onRay, onSeg)), at: s };
}

/**
 * The connection whose seam a ray passes nearest, within `reach` mm; the
 * nearer to the eye wins between seams equally close (within 1 mm).
 */
export function nearestSeam(
  origin: Vec3,
  direction: Vec3,
  connections: readonly ConnectionRow[],
  reach: number = SEAM_REACH,
): ConnectionRow | null {
  let best: ConnectionRow | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;
  let bestAt = Number.POSITIVE_INFINITY;
  for (const c of connections) {
    const { distance, at } = raySegmentDistance(origin, direction, c.seam.from, c.seam.to);
    if (distance > reach) {
      continue;
    }
    const clearly = distance < bestDistance - 1;
    const tie = Math.abs(distance - bestDistance) <= 1 && at < bestAt;
    if (clearly || tie) {
      best = c;
      bestDistance = distance;
      bestAt = at;
    }
  }
  return best;
}

/** The box round every panel, or null for none. */
export function overallBox(boxes: readonly FurnitureBox[]): FurnitureBox | null {
  if (boxes.length === 0) {
    return null;
  }
  const min: Vec3 = [Infinity, Infinity, Infinity];
  const max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (const b of boxes) {
    for (let i = 0; i < 3; i += 1) {
      min[i] = Math.min(min[i] ?? Infinity, b.min[i] ?? Infinity);
      max[i] = Math.max(max[i] ?? -Infinity, b.max[i] ?? -Infinity);
    }
  }
  return { min, max };
}

export type DimensionAxis = 'width' | 'depth' | 'height';

/** A dimension line: its ends, the extension lines from the box, and its length, mm. */
export interface DimensionLine {
  axis: DimensionAxis;
  from: Vec3;
  to: Vec3;
  /** Short lines from the box's corners out to the dimension line's ends. */
  extensions: Array<[Vec3, Vec3]>;
  length: number;
}

/**
 * The overall width, depth and height lines, `gap` mm clear of the box:
 * width along the front bottom edge, depth along the right bottom edge
 * (both just in front of or beside the floor line), height up the front
 * right corner.
 */
export function dimensionLines(box: FurnitureBox, gap: number): DimensionLine[] {
  const [x0, y0, z0] = box.min;
  const [x1, y1, z1] = box.max;
  return [
    {
      axis: 'width',
      from: [x0, y0 - gap, z0],
      to: [x1, y0 - gap, z0],
      extensions: [
        [
          [x0, y0, z0],
          [x0, y0 - gap * 1.2, z0],
        ],
        [
          [x1, y0, z0],
          [x1, y0 - gap * 1.2, z0],
        ],
      ],
      length: x1 - x0,
    },
    {
      axis: 'depth',
      from: [x1 + gap, y0, z0],
      to: [x1 + gap, y1, z0],
      extensions: [
        [
          [x1, y0, z0],
          [x1 + gap * 1.2, y0, z0],
        ],
        [
          [x1, y1, z0],
          [x1 + gap * 1.2, y1, z0],
        ],
      ],
      length: y1 - y0,
    },
    {
      axis: 'height',
      from: [x1 + gap, y0 - gap, z0],
      to: [x1 + gap, y0 - gap, z1],
      extensions: [
        [
          [x1, y0, z1],
          [x1 + gap * 1.2, y0 - gap * 1.2, z1],
        ],
      ],
      length: z1 - z0,
    },
  ];
}

/** The gap between a design and its dimension lines: a tenth of its largest side, 20 to 120 mm. */
export function dimensionGap(box: FurnitureBox): number {
  const largest = Math.max(
    box.max[0] - box.min[0],
    box.max[1] - box.min[1],
    box.max[2] - box.min[2],
  );
  return Math.min(120, Math.max(20, largest / 10));
}

/** The middle of a line. */
export function midpoint(a: Vec3, b: Vec3): Vec3 {
  return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2];
}

/** A drag arrow on an outer face: it changes the size along its axis. */
export interface SizeArrow {
  axis: DimensionAxis;
  /** The middle of the face it stands on, and its tip, job space mm. */
  base: Vec3;
  tip: Vec3;
}

/**
 * The arrows that drag the overall size: out of the right face (width),
 * the back face (depth: the front stays at Y = 0) and the top (height),
 * each `length` mm long.
 */
export function sizeArrows(box: FurnitureBox, length: number): SizeArrow[] {
  const [x0, y0, z0] = box.min;
  const [x1, y1, z1] = box.max;
  const mx = (x0 + x1) / 2;
  const my = (y0 + y1) / 2;
  const mz = (z0 + z1) / 2;
  return [
    { axis: 'width', base: [x1, my, mz], tip: [x1 + length, my, mz] },
    { axis: 'depth', base: [mx, y1, mz], tip: [mx, y1 + length, mz] },
    { axis: 'height', base: [mx, my, z1], tip: [mx, my, z1 + length] },
  ];
}

/**
 * The value a size arrow dragged `alongPx` screen pixels (along its own
 * direction on screen) gives: `start` plus the pixels at `mmPerPx`,
 * rounded to `step` and kept within min and max.
 */
export function draggedSize(
  start: number,
  alongPx: number,
  mmPerPx: number,
  step: number,
  min?: number,
  max?: number,
): number {
  const raw = start + alongPx * mmPerPx;
  const stepped = Math.round(raw / step) * step;
  const low = min ?? Number.NEGATIVE_INFINITY;
  const high = max ?? Number.POSITIVE_INFINITY;
  return Math.min(high, Math.max(low, stepped));
}

/** A point moved by `by`. */
export function shifted(p: Vec3, by: Vec3): Vec3 {
  return [p[0] + by[0], p[1] + by[1], p[2] + by[2]];
}

/** A seam in the Assembly View: furniture space moved to the design's origin. */
export function placedSeam(seam: { from: Vec3; to: Vec3 }, origin: Vec3): { from: Vec3; to: Vec3 } {
  return { from: shifted(seam.from, origin), to: shifted(seam.to, origin) };
}

/** The box round a preview's panels where the Assembly View shows them, or null. */
export function placedBox(
  boxes: readonly FurnitureBox[],
  origin: Vec3 = [0, 0, 0],
): FurnitureBox | null {
  const box = overallBox(boxes);
  return box === null ? null : { min: shifted(box.min, origin), max: shifted(box.max, origin) };
}

/**
 * Whether the designer draws its own preview panels: before Create, while
 * a change is on its way (values not yet made, or a request out), and
 * while models on its faces have no carving toolpaths yet (`uncarved`: the
 * pieces cannot show them). A made, settled design shows as its pieces,
 * the parts its toolpaths cut.
 */
export function showsPreview(
  session: { design: number | null; values: FurnitureValues; made: FurnitureValues | null } | null,
  busy: boolean,
  uncarved = false,
): boolean {
  if (session === null) {
    return false;
  }
  if (session.design === null || session.made === null || busy || uncarved) {
    return true;
  }
  const keys = new Set([...Object.keys(session.values), ...Object.keys(session.made)]);
  for (const key of keys) {
    if (session.values[key] !== session.made[key]) {
      return true;
    }
  }
  return false;
}

/** What of the furniture store decides whether the preview shows. */
export interface PreviewState {
  session: { design: number | null; values: FurnitureValues; made: FurnitureValues | null } | null;
  busy: boolean;
  designs: readonly Pick<DesignRow, 'id' | 'made' | 'models' | 'carving'>[];
}

/** Whether the designer's preview shows (see [`showsPreview`]), from the store. */
export function previewShown(s: PreviewState): boolean {
  const row = s.designs.find((d) => d.id === s.session?.design);
  const uncarved = row !== undefined && (row.models ?? 0) > 0 && (row.carving ?? 0) === 0;
  return showsPreview(s.session, s.busy, uncarved);
}

/**
 * The sheets whose pieces the preview stands in for while it shows over a
 * made design, as "id,id" (a string, so a store selector stays stable);
 * empty when the made pieces show.
 */
export function coveredSheets(s: PreviewState): string {
  const design = s.session?.design ?? null;
  if (design === null || !previewShown(s)) {
    return '';
  }
  const row = s.designs.find((d) => d.id === design);
  const sheets = new Set(Object.values(row?.made ?? {}).map((m) => m.sheet));
  return [...sheets].sort((a, b) => a - b).join(',');
}
