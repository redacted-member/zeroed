// The faces of an Assembly View piece, read from its relief: the region of
// the face under the pointer (lit while a mate waits for a face) and the
// flat faces a dragged piece snaps by (the outside of its part, pocket
// floors, and the walls of slots, steps and holes). Everything is in the
// piece's own frame: X and Y from the lower left corner of its part box, Z
// up from the underside, millimetres, as the kernel's piece frame.

import type { PieceFaceHit, PieceRow } from '../../bridge/assembly';
import type { Heightfield } from '../../bridge/cam';
import { floorReader, heightReader } from '../../bridge/heights';
import { apply4, pieceMatrix, type Box } from './pieceGizmo';
import type { Vec3 } from './rotateRings';

/** How far a lit face stands off its surface, mm, so it draws over it. */
const LIFT = 0.05;
/** A step in the surface counts as a wall when it is at least this tall, mm. */
const STEP = 0.2;
/** A level or wall counts when it covers at least this share of the cells. */
const PLANE_SHARE = 0.02;
/** Most inner planes kept along each axis. */
const MAX_PLANES = 12;
/** Faces snap when their normals are within this of parallel (cosine). */
const PARALLEL = Math.cos((2 * Math.PI) / 180);

/** A relief read cell by cell in the piece's frame. */
interface Cells {
  width: number;
  height: number;
  cell: number;
  /** Material top of cell `i` above the underside of the box, NaN for none. */
  top: (i: number) => number;
  /** Material underside of cell `i` above the underside of the box. */
  under: (i: number) => number;
}

type Relief = Pick<Heightfield, 'width' | 'height' | 'cellSize' | 'bottom' | 'z' | 'floor'>;

function cellsOf(field: Relief): Cells {
  const z = heightReader(field.z);
  const floor = floorReader(field);
  const { bottom } = field;
  return {
    width: field.width,
    height: field.height,
    cell: field.cellSize,
    top: (i) => z(i) - bottom,
    under: (i) => {
      const f = floor === null ? bottom : floor(i);
      return (Number.isNaN(f) ? bottom : f) - bottom;
    },
  };
}

function isMaterial(c: Cells, i: number): boolean {
  return !Number.isNaN(c.top(i)) && c.top(i) > c.under(i);
}

/** Whether cell (x, y) holds material at height `z`. */
function solidAt(c: Cells, x: number, y: number, z: number): boolean {
  if (x < 0 || y < 0 || x >= c.width || y >= c.height) {
    return false;
  }
  const i = y * c.width + x;
  return isMaterial(c, i) && c.under(i) <= z + 1e-6 && z <= c.top(i) + 1e-6;
}

/** Two triangles for the quad a, b, c, d (in order round it). */
function pushQuad(out: number[], a: Vec3, b: Vec3, c: Vec3, d: Vec3): void {
  out.push(...a, ...b, ...c, ...a, ...c, ...d);
}

/**
 * The triangles of the face a mate would take at `face` (a point on the
 * piece and its outward normal, as the kernel found it): on the top or
 * underside, the flat stretch at that height joined to the point; on a
 * wall, the wall joined to the point along its length, each column from
 * where the material starts to where it ends. Empty when nothing is there.
 */
export function faceRegion(field: Relief, face: PieceFaceHit): Float32Array {
  const c = cellsOf(field);
  const { width, height, cell } = c;
  const [px, py, pz] = face.point;
  const [nx, ny, nz] = face.normal;
  const out: number[] = [];
  const clampX = (v: number) => Math.min(Math.max(Math.floor(v / cell), 0), width - 1);
  const clampY = (v: number) => Math.min(Math.max(Math.floor(v / cell), 0), height - 1);
  const sx = clampX(px);
  const sy = clampY(py);

  if (Math.abs(nz) > 0.7) {
    const up = nz > 0;
    const level = (i: number) => (up ? c.top(i) : c.under(i));
    const tolerance = Math.max(cell * 0.75, 0.05);
    const flat = (i: number) => isMaterial(c, i) && Math.abs(level(i) - pz) <= tolerance;
    // The nearest cell of that face to the point, then the cells joined to it.
    let start = -1;
    for (let r = 0; r <= 2 && start < 0; r += 1) {
      for (let dy = -r; dy <= r && start < 0; dy += 1) {
        for (let dx = -r; dx <= r; dx += 1) {
          const x = sx + dx;
          const y = sy + dy;
          if (x >= 0 && y >= 0 && x < width && y < height && flat(y * width + x)) {
            start = y * width + x;
            break;
          }
        }
      }
    }
    if (start < 0) {
      return new Float32Array(0);
    }
    const lit = new Uint8Array(width * height);
    const queue = [start];
    lit[start] = 1;
    while (queue.length > 0) {
      const i = queue.pop() ?? 0;
      const x = i % width;
      const y = (i - x) / width;
      const visit = (j: number) => {
        if (lit[j] === 0 && flat(j)) {
          lit[j] = 1;
          queue.push(j);
        }
      };
      if (x > 0) {
        visit(i - 1);
      }
      if (x + 1 < width) {
        visit(i + 1);
      }
      if (y > 0) {
        visit(i - width);
      }
      if (y + 1 < height) {
        visit(i + width);
      }
    }
    const z = pz + (up ? LIFT : -LIFT);
    // One quad per run of lit cells along a row.
    for (let y = 0; y < height; y += 1) {
      let x = 0;
      while (x < width) {
        if (lit[y * width + x] === 0) {
          x += 1;
          continue;
        }
        const from = x;
        while (x < width && lit[y * width + x] === 1) {
          x += 1;
        }
        const a: Vec3 = [from * cell, y * cell, z];
        const b: Vec3 = [x * cell, y * cell, z];
        const d: Vec3 = [x * cell, (y + 1) * cell, z];
        const e: Vec3 = [from * cell, (y + 1) * cell, z];
        if (up) {
          pushQuad(out, a, b, d, e);
        } else {
          pushQuad(out, a, e, d, b);
        }
      }
    }
    return new Float32Array(out);
  }

  // A wall: the cells where the material at the point's height ends in the
  // normal's direction, near the wall's plane and joined to the point.
  const length = Math.hypot(nx, ny);
  if (length < 1e-9) {
    return new Float32Array(0);
  }
  const n: [number, number] = [nx / length, ny / length];
  const plane = n[0] * px + n[1] * py;
  const step: [number, number] = [Math.round(n[0]), Math.round(n[1])];
  if (step[0] === 0 && step[1] === 0) {
    return new Float32Array(0);
  }
  const reach = cell * 1.5;
  const onWall = (x: number, y: number) => {
    if (!solidAt(c, x, y, pz) || solidAt(c, x + step[0], y + step[1], pz)) {
      return false;
    }
    const cx = (x + 0.5) * cell;
    const cy = (y + 0.5) * cell;
    return Math.abs(n[0] * cx + n[1] * cy + cell / 2 - plane) <= reach;
  };
  let start: [number, number] | null = null;
  let best = Infinity;
  for (let dy = -3; dy <= 3; dy += 1) {
    for (let dx = -3; dx <= 3; dx += 1) {
      const x = sx + dx;
      const y = sy + dy;
      if (onWall(x, y)) {
        const d = Math.hypot((x + 0.5) * cell - px, (y + 0.5) * cell - py);
        if (d < best) {
          best = d;
          start = [x, y];
        }
      }
    }
  }
  if (start === null) {
    return new Float32Array(0);
  }
  const seen = new Set<number>([start[1] * width + start[0]]);
  const queue: Array<[number, number]> = [start];
  const t: [number, number] = [-n[1], n[0]];
  const at = plane + LIFT;
  while (queue.length > 0) {
    const [x, y] = queue.pop() ?? [0, 0];
    const i = y * width + x;
    // The column's wall: where the material steps down (or up) next door.
    const nextX = x + step[0];
    const nextY = y + step[1];
    const inside = nextX >= 0 && nextY >= 0 && nextX < width && nextY < height;
    const j = nextY * width + nextX;
    let low = c.under(i);
    let high = c.top(i);
    if (inside && isMaterial(c, j)) {
      if (c.top(j) < pz) {
        low = Math.max(low, c.top(j));
      } else {
        high = Math.min(high, c.under(j));
      }
    }
    const along = t[0] * (x + 0.5) * cell + t[1] * (y + 0.5) * cell;
    const corner = (s: number, z: number): Vec3 => [n[0] * at + t[0] * s, n[1] * at + t[1] * s, z];
    const s0 = along - cell / 2;
    const s1 = along + cell / 2;
    pushQuad(out, corner(s0, low), corner(s1, low), corner(s1, high), corner(s0, high));
    for (let dy = -1; dy <= 1; dy += 1) {
      for (let dx = -1; dx <= 1; dx += 1) {
        const x2 = x + dx;
        const y2 = y + dy;
        const k = y2 * width + x2;
        if ((dx !== 0 || dy !== 0) && !seen.has(k) && onWall(x2, y2)) {
          seen.add(k);
          queue.push([x2, y2]);
        }
      }
    }
  }
  return new Float32Array(out);
}

/** A flat face of a piece in its own frame: along `axis` at `offset`, facing `sign`. */
export interface LocalPlane {
  axis: 0 | 1 | 2;
  sign: 1 | -1;
  offset: number;
}

/** The six sides of a piece's part box, for a piece whose relief is not loaded. */
export function boxPlanes(size: readonly [number, number, number]): LocalPlane[] {
  return ([0, 1, 2] as const).flatMap((axis) => [
    { axis, sign: -1 as const, offset: 0 },
    { axis, sign: 1 as const, offset: size[axis] },
  ]);
}

/** The strongest `MAX_PLANES` entries of a count map, at least `least` cells each. */
function strongest(counts: Map<string, number>, least: number): string[] {
  return [...counts.entries()]
    .filter(([, n]) => n >= least)
    .sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
    .slice(0, MAX_PLANES)
    .map(([key]) => key);
}

/**
 * The flat faces of a piece a dragged piece snaps by: the outside of its
 * material (its box sides, the highest top and lowest underside), levels
 * the top or underside keeps over a share of the cells (pocket floors,
 * rebates), and walls along X or Y where the material steps (slot and
 * dado sides, holes, notches).
 */
export function piecePlanes(field: Relief): LocalPlane[] {
  const c = cellsOf(field);
  const { width, height, cell } = c;
  let material = 0;
  let span: [number, number, number, number] | null = null;
  let high = -Infinity;
  let low = Infinity;
  const tops = new Map<string, number>();
  const unders = new Map<string, number>();
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = y * width + x;
      if (!isMaterial(c, i)) {
        continue;
      }
      material += 1;
      span =
        span === null
          ? [x, y, x + 1, y + 1]
          : [
              Math.min(span[0], x),
              Math.min(span[1], y),
              Math.max(span[2], x + 1),
              Math.max(span[3], y + 1),
            ];
      high = Math.max(high, c.top(i));
      low = Math.min(low, c.under(i));
      const t = c.top(i).toFixed(2);
      tops.set(t, (tops.get(t) ?? 0) + 1);
      const u = c.under(i).toFixed(2);
      unders.set(u, (unders.get(u) ?? 0) + 1);
    }
  }
  if (span === null) {
    return [];
  }
  const least = Math.max(4, Math.ceil(material * PLANE_SHARE));
  const planes: LocalPlane[] = [
    { axis: 0, sign: -1, offset: span[0] * cell },
    { axis: 0, sign: 1, offset: span[2] * cell },
    { axis: 1, sign: -1, offset: span[1] * cell },
    { axis: 1, sign: 1, offset: span[3] * cell },
    { axis: 2, sign: -1, offset: low },
    { axis: 2, sign: 1, offset: high },
  ];
  const known = (p: LocalPlane) =>
    planes.some(
      (q) => q.axis === p.axis && q.sign === p.sign && Math.abs(q.offset - p.offset) < 1e-3,
    );
  const add = (p: LocalPlane) => {
    if (!known(p)) {
      planes.push(p);
    }
  };
  for (const key of strongest(tops, least)) {
    add({ axis: 2, sign: 1, offset: Number(key) });
  }
  for (const key of strongest(unders, least)) {
    add({ axis: 2, sign: -1, offset: Number(key) });
  }
  // Walls: where the top steps down by at least STEP between neighbours,
  // facing the lower side; a hole or the outside counts as the bottom.
  const walls: [Map<string, number>, Map<string, number>] = [new Map(), new Map()];
  const topOr = (i: number) => (isMaterial(c, i) ? c.top(i) : -Infinity);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = y * width + x;
      const here = topOr(i);
      for (const axis of [0, 1] as const) {
        const nextX = x + (axis === 0 ? 1 : 0);
        const nextY = y + (axis === 1 ? 1 : 0);
        if (nextX >= width || nextY >= height) {
          continue;
        }
        const there = topOr(nextY * width + nextX);
        if (Math.abs(here - there) < STEP || (here === -Infinity && there === -Infinity)) {
          continue;
        }
        const at = (axis === 0 ? nextX : nextY) * cell;
        const sign = here > there ? 1 : -1;
        const key = `${sign}:${at.toFixed(3)}`;
        walls[axis].set(key, (walls[axis].get(key) ?? 0) + 1);
      }
    }
  }
  for (const axis of [0, 1] as const) {
    const across = axis === 0 ? height : width;
    for (const key of strongest(walls[axis], Math.max(3, Math.ceil(across * PLANE_SHARE)))) {
      const [sign, at] = key.split(':');
      add({ axis, sign: sign === '1' ? 1 : -1, offset: Number(at) });
    }
  }
  return planes;
}

/** A face plane in assembly space: unit normal `n` and `n . p = d` on it. */
interface WorldPlane {
  n: Vec3;
  d: number;
}

function worldPlanes(
  piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>,
  planes: readonly LocalPlane[],
): WorldPlane[] {
  const m = pieceMatrix(piece);
  const at = (i: number) => m[i] ?? 0;
  return planes.map((p) => {
    // The rotation's column for the axis is that local axis in assembly space.
    const col = p.axis * 4;
    const n: Vec3 = [at(col) * p.sign, at(col + 1) * p.sign, at(col + 2) * p.sign];
    const local: [number, number, number] = [
      piece.size[0] / 2,
      piece.size[1] / 2,
      piece.size[2] / 2,
    ];
    local[p.axis] = p.offset;
    const w = apply4(m, local);
    return { n, d: n[0] * w[0] + n[1] * w[1] + n[2] * w[2] };
  });
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function boxesNear(a: Box, b: Box, margin: number): boolean {
  return [0, 1, 2].every(
    (k) =>
      (a.min[k] ?? 0) <= (b.max[k] ?? 0) + margin && (b.min[k] ?? 0) <= (a.max[k] ?? 0) + margin,
  );
}

function shifted(box: Box, s: Vec3): Box {
  return {
    min: [box.min[0] + s[0], box.min[1] + s[1], box.min[2] + s[2]],
    max: [box.max[0] + s[0], box.max[1] + s[1], box.max[2] + s[2]],
  };
}

/** A piece as snapping sees it: its place, its box and its flat faces. */
export interface SnapPiece {
  piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>;
  box: Box;
  planes: readonly LocalPlane[];
}

/**
 * How far to shift a dragged piece so one of its flat faces meets a flat
 * face of a piece near it (against it, or in line with it) when that is
 * within `tolerance` mm. Along an arrow (`axis` 0, 1 or 2) the shift runs
 * along that axis; for the ball (`axis` null) it runs along the faces'
 * normal, and up to three faces facing different ways snap at once. A
 * piece is near when the boxes come within the tolerance of each other
 * after the shift.
 */
export function faceSnap(
  moving: SnapPiece,
  others: readonly SnapPiece[],
  axis: 0 | 1 | 2 | null,
  tolerance: number,
): [number, number, number] {
  const own = worldPlanes(moving.piece, moving.planes);
  const theirs = others.map((o) => ({ box: o.box, planes: worldPlanes(o.piece, o.planes) }));
  const shift: [number, number, number] = [0, 0, 0];
  const used: Vec3[] = [];
  for (let round = 0; round < (axis === null ? 3 : 1); round += 1) {
    let best: { size: number; move: Vec3; n: Vec3 } | null = null;
    for (const other of theirs) {
      for (const q of other.planes) {
        if (used.some((u) => Math.abs(dot(u, q.n)) > 0.1)) {
          continue;
        }
        const along: Vec3 | null =
          axis === null ? q.n : axis === 0 ? [1, 0, 0] : axis === 1 ? [0, 1, 0] : [0, 0, 1];
        const rate = dot(q.n, along);
        if (Math.abs(rate) < 0.5) {
          continue;
        }
        for (const p of own) {
          if (Math.abs(dot(p.n, q.n)) < PARALLEL) {
            continue;
          }
          // Where the moving face lies along q's normal, after the shift so far.
          const now = (dot(p.n, q.n) > 0 ? p.d : -p.d) + dot(q.n, shift);
          const s = (q.d - now) / rate;
          const size = Math.abs(s);
          if (size > tolerance || (best !== null && size >= best.size)) {
            continue;
          }
          const move: Vec3 = [along[0] * s, along[1] * s, along[2] * s];
          const total: Vec3 = [shift[0] + move[0], shift[1] + move[1], shift[2] + move[2]];
          if (!boxesNear(shifted(moving.box, total), other.box, tolerance)) {
            continue;
          }
          best = { size, move, n: q.n };
        }
      }
    }
    if (best === null) {
      break;
    }
    shift[0] += best.move[0];
    shift[1] += best.move[1];
    shift[2] += best.move[2];
    used.push(best.n);
  }
  return shift;
}
