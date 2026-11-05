// Editing a drawn shape again with its creation form: while Draw Circle (and
// the other shape forms) is open, clicking an existing shape of that kind
// selects it (Shift adds more) and the form shows its values; Apply redraws
// the selected shapes from the fields. The shape is recognised from its
// geometry, since vectors keep no record of the tool that made them.

import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';

import { api } from '../bridge/api';
import type { EntityJson, Path, Point } from '../bridge/document';
import type { Units } from '../bridge/types';
import type { DrawPointer } from '../state/drawStore';
import { useViewStore } from '../state/viewStore';
import { useDocumentStore } from '../state/documentStore';
import { arcFromBulge } from './geometry';

const TOL = 1e-6;

export interface CircleShape {
  centre: Point;
  radius: number;
}

/** The circle a path draws: one closed loop of arcs sharing a centre and radius. */
export function recogniseCircle(path: Path): CircleShape | null {
  const [sub, ...rest] = path.subpaths;
  if (sub === undefined || rest.length > 0 || !sub.closed || sub.segments.length === 0) {
    return null;
  }
  let from = sub.start;
  let found: CircleShape | null = null;
  let sweep = 0;
  for (const segment of sub.segments) {
    if (segment.kind !== 'arc') {
      return null;
    }
    const arc = arcFromBulge(from, segment.to, segment.bulge);
    if (arc === null) {
      return null;
    }
    const scale = Math.max(1, arc.radius) * 1e-6;
    if (
      found !== null &&
      (Math.abs(arc.radius - found.radius) > scale ||
        Math.hypot(arc.centre.x - found.centre.x, arc.centre.y - found.centre.y) > scale)
    ) {
      return null;
    }
    found = found ?? { centre: arc.centre, radius: arc.radius };
    sweep += arc.sweep;
    from = segment.to;
  }
  const closes = Math.hypot(from.x - sub.start.x, from.y - sub.start.y) <= TOL;
  return found !== null && closes && Math.abs(Math.abs(sweep) - 2 * Math.PI) < 1e-6 ? found : null;
}

type Box = [number, number, number, number];

/** The one closed subpath of a path, or null. */
function onlyLoop(path: Path) {
  const [sub, ...rest] = path.subpaths;
  return sub !== undefined && rest.length === 0 && sub.closed && sub.segments.length > 0
    ? sub
    : null;
}

/** The corners of a closed loop of straight spans, without the repeated start. */
function lineLoop(path: Path): Point[] | null {
  const sub = onlyLoop(path);
  if (sub === null || sub.segments.some((s) => s.kind !== 'line')) {
    return null;
  }
  const points = [sub.start, ...sub.segments.map((s) => s.to)];
  const last = points[points.length - 1];
  if (last !== undefined && Math.hypot(last.x - sub.start.x, last.y - sub.start.y) <= TOL) {
    points.pop();
  }
  return points;
}

function boxOf(points: readonly Point[]): Box {
  const xs = points.map((p) => p.x);
  const ys = points.map((p) => p.y);
  return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

function near(a: number, b: number, scale: number): boolean {
  return Math.abs(a - b) <= Math.max(1, scale) * 1e-6;
}

export interface EllipseShape {
  box: Box;
}

/** An axis aligned ellipse drawn as four quarter curves between its extremes. */
export function recogniseEllipse(path: Path): EllipseShape | null {
  const sub = onlyLoop(path);
  if (sub === null || sub.segments.length !== 4 || sub.segments.some((s) => s.kind !== 'cubic')) {
    return null;
  }
  const ends = sub.segments.map((s) => s.to);
  const box = boxOf(ends);
  const [x0, y0, x1, y1] = box;
  const size = Math.max(x1 - x0, y1 - y0);
  if (x1 - x0 <= TOL || y1 - y0 <= TOL) {
    return null;
  }
  const onEdge = (p: Point) =>
    near(p.x, x0, size) || near(p.x, x1, size) || near(p.y, y0, size) || near(p.y, y1, size);
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  const midpoint = (p: Point) =>
    (near(p.x, cx, size) && (near(p.y, y0, size) || near(p.y, y1, size))) ||
    (near(p.y, cy, size) && (near(p.x, x0, size) || near(p.x, x1, size)));
  const controls = sub.segments.flatMap((s) => (s.kind === 'cubic' ? [s.c1, s.c2] : []));
  return ends.every(midpoint) && controls.every(onEdge) ? { box } : null;
}

export interface RectangleShape {
  box: Box;
  corner: 'none' | 'external' | 'internal';
  radius: number;
}

/** An axis aligned rectangle, sharp or with equal quarter round corners. */
export function recogniseRectangle(path: Path): RectangleShape | null {
  const corners = lineLoop(path);
  if (corners !== null) {
    if (corners.length !== 4) {
      return null;
    }
    const box = boxOf(corners);
    const [x0, y0, x1, y1] = box;
    const size = Math.max(x1 - x0, y1 - y0);
    const seen = new Set<string>();
    for (const p of corners) {
      const cornerX = near(p.x, x0, size) ? 0 : near(p.x, x1, size) ? 1 : -1;
      const cornerY = near(p.y, y0, size) ? 0 : near(p.y, y1, size) ? 1 : -1;
      if (cornerX < 0 || cornerY < 0) {
        return null;
      }
      seen.add(`${String(cornerX)}${String(cornerY)}`);
    }
    return seen.size === 4 && size > TOL ? { box, corner: 'none', radius: 0 } : null;
  }
  const sub = onlyLoop(path);
  if (sub === null || sub.segments.length !== 8) {
    return null;
  }
  const quarter = Math.tan(Math.PI / 8);
  let from = sub.start;
  let radius: number | null = null;
  const centres: Point[] = [];
  for (const [index, segment] of sub.segments.entries()) {
    const wantArc = sub.segments[0]?.kind === 'arc' ? index % 2 === 0 : index % 2 === 1;
    if ((segment.kind === 'arc') !== wantArc || segment.kind === 'cubic') {
      return null;
    }
    if (segment.kind === 'arc') {
      const arc = arcFromBulge(from, segment.to, segment.bulge);
      if (arc === null || !near(Math.abs(segment.bulge), quarter, 1)) {
        return null;
      }
      if (radius !== null && !near(arc.radius, radius, radius)) {
        return null;
      }
      radius = arc.radius;
      centres.push(arc.centre);
    }
    from = segment.to;
  }
  const box = boxOf([sub.start, ...sub.segments.map((s) => s.to)]);
  const [x0, y0, x1, y1] = box;
  const size = Math.max(x1 - x0, y1 - y0);
  if (radius === null) {
    return null;
  }
  // Internal corners are centred on the box corners, external ones inside.
  const onCorner = (p: Point) =>
    (near(p.x, x0, size) || near(p.x, x1, size)) && (near(p.y, y0, size) || near(p.y, y1, size));
  const internal = centres.every(onCorner);
  return { box, corner: internal ? 'internal' : 'external', radius };
}

export interface PolygonShape {
  centre: Point;
  /** Distance from the centre to the corners. */
  radius: number;
  sides: number;
}

/** A regular polygon: straight spans of equal length, corners on one circle. */
export function recognisePolygon(path: Path): PolygonShape | null {
  const corners = lineLoop(path);
  if (corners === null || corners.length < 3) {
    return null;
  }
  const centre = {
    x: corners.reduce((sum, p) => sum + p.x, 0) / corners.length,
    y: corners.reduce((sum, p) => sum + p.y, 0) / corners.length,
  };
  const radii = corners.map((p) => Math.hypot(p.x - centre.x, p.y - centre.y));
  const radius = radii[0] ?? 0;
  const sides = corners.map((p, i) => {
    const q = corners[(i + 1) % corners.length] ?? p;
    return Math.hypot(q.x - p.x, q.y - p.y);
  });
  const side = sides[0] ?? 0;
  return radius > TOL &&
    radii.every((r) => near(r, radius, radius)) &&
    sides.every((l) => near(l, side, side))
    ? { centre, radius, sides: corners.length }
    : null;
}

export interface StarShape {
  centre: Point;
  points: number;
  outer: number;
  inner: number;
}

/** A star: corners alternating between two circles round one centre. */
export function recogniseStar(path: Path): StarShape | null {
  const corners = lineLoop(path);
  if (corners === null || corners.length < 6 || corners.length % 2 !== 0) {
    return null;
  }
  const centre = {
    x: corners.reduce((sum, p) => sum + p.x, 0) / corners.length,
    y: corners.reduce((sum, p) => sum + p.y, 0) / corners.length,
  };
  const radii = corners.map((p) => Math.hypot(p.x - centre.x, p.y - centre.y));
  const a = radii[0] ?? 0;
  const b = radii[1] ?? 0;
  const alternates = radii.every((r, i) => near(r, i % 2 === 0 ? a : b, Math.max(a, b)));
  return alternates && !near(a, b, Math.max(a, b)) && Math.min(a, b) > TOL
    ? { centre, points: corners.length / 2, outer: Math.max(a, b), inner: Math.min(a, b) }
    : null;
}

export interface EditableShape<T> {
  id: number;
  entity: Extract<EntityJson, { kind: 'path' }>;
  shape: T;
}

/**
 * The shapes the form edits: the vectors selected when it opened or picked
 * in it since (not ones it has just drawn), that `recognise` accepts.
 * `picked` answers the picked ids and a way to record a new pick.
 */
export function useEditableShapes<T>(recognise: (path: Path) => T | null): {
  shapes: EditableShape<T>[];
  pick: (id: number, add: boolean) => void;
} {
  const selected = useDocumentStore((store) => store.selection.ids);
  const [picked, setPicked] = useState<number[]>(() => selected);
  const ids = useMemo(() => selected.filter((id) => picked.includes(id)), [selected, picked]);
  const pick = useCallback(
    (id: number, add: boolean) => setPicked((current) => (add ? [...current, id] : [id])),
    [],
  );
  const revision = useDocumentStore((store) => store.revision);
  const [found, setFound] = useState<{ key: string; shapes: EditableShape<T>[] }>({
    key: '',
    shapes: [],
  });
  const key = `${ids.join(',')}@${String(revision)}`;
  useEffect(() => {
    if (ids.length === 0) {
      return;
    }
    let live = true;
    void Promise.all(ids.map((id) => api.entities.get(id).catch(() => null)))
      .then((entities) => {
        const shapes: EditableShape<T>[] = [];
        entities.forEach((entity, index) => {
          const id = ids[index];
          if (entity !== null && entity.kind === 'path' && id !== undefined) {
            const shape = recognise(entity.path);
            if (shape !== null) {
              shapes.push({ id, entity, shape });
            }
          }
        });
        if (live) {
          setFound({ key: `${ids.join(',')}@${String(revision)}`, shapes });
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [ids, revision, recognise]);
  return { shapes: ids.length === 0 || found.key !== key ? [] : found.shapes, pick };
}

/**
 * A click with a shape form open: when it lands on a shape `recognise`
 * accepts, that shape is selected (Shift adds it) and its id comes back.
 */
export async function pickShapeAt<T>(
  at: Point,
  tolerance: number,
  shift: boolean,
  recognise: (path: Path) => T | null,
): Promise<number | null> {
  const { ids } = await api.entities.hitTest({ x: at.x, y: at.y, tolerance });
  for (const id of ids) {
    const entity = await api.entities.get(id).catch(() => null);
    if (entity !== null && entity.kind === 'path' && recognise(entity.path) !== null) {
      await useDocumentStore.getState().select([id], shift ? 'add' : 'set');
      return id;
    }
  }
  return null;
}

/** Replaces the geometry of each edited shape, one kernel call each. */
export async function replaceShapes(
  shapes: ReadonlyArray<{ id: number; entity: Extract<EntityJson, { kind: 'path' }>; path: Path }>,
): Promise<void> {
  for (const { id, entity, path } of shapes) {
    await api.entities.replace(id, { ...entity, path });
  }
  await useDocumentStore.getState().refresh();
}

/** Screen pixels a click may miss a shape's outline by and still pick it. */
const PICK_PX = 6;

export interface ShapeEditorOptions<S, T> {
  recognise: (path: Path) => T | null;
  /** The fields showing a shape. */
  fill: (state: S, units: Units, shape: T) => S;
  /** The fields with only the position taken from a shape, the size kept. */
  place: (state: S, units: Units, shape: T) => S;
  /** The shape the fields describe. */
  build: (state: S, units: Units) => Path | null;
  setState: Dispatch<SetStateAction<S>>;
  stateRef: MutableRefObject<S>;
  units: Units;
  unitsRef: MutableRefObject<Units>;
}

export interface ShapeEditor {
  /** How many shapes the form is editing. */
  count: number;
  /** A click in the view: picks a shape under it, or runs `otherwise`. */
  click: (at: Point, pointer: DrawPointer, otherwise: () => void) => void;
  /** Apply: redraws the edited shapes from the fields, each at its own place when several. */
  apply: () => Promise<void>;
}

/** Edit mode for a shape creation form (see the top of this file). */
export function useShapeEditor<S, T>(options: ShapeEditorOptions<S, T>): ShapeEditor {
  const { recognise, fill, place, build, setState, stateRef, units, unitsRef } = options;
  const { shapes, pick } = useEditableShapes<T>(recognise);
  // A newly picked shape shows its values in the fields.
  const key = shapes.map((e) => `${String(e.id)}:${String(e.entity.revision)}`).join(',');
  const [shownKey, setShownKey] = useState('');
  if (key !== shownKey) {
    setShownKey(key);
    const first = shapes[0];
    if (first !== undefined) {
      setState((current) => fill(current, units, first.shape));
    }
  }
  const click = useCallback(
    (at: Point, pointer: DrawPointer, otherwise: () => void) => {
      const tolerance = PICK_PX / useViewStore.getState().camera.pxPerMm;
      void pickShapeAt(at, tolerance, pointer.shift, recognise).then((picked) => {
        if (picked === null) {
          otherwise();
        } else {
          pick(picked, pointer.shift);
        }
      });
    },
    [pick, recognise],
  );
  const apply = useCallback(async () => {
    const current = stateRef.current;
    const u = unitsRef.current;
    const edits = shapes
      .map((edit) => ({
        id: edit.id,
        entity: edit.entity,
        path: shapes.length === 1 ? build(current, u) : build(place(current, u, edit.shape), u),
      }))
      .filter((edit): edit is typeof edit & { path: Path } => edit.path !== null);
    if (edits.length > 0) {
      await replaceShapes(edits);
    }
  }, [build, place, shapes, stateRef, unitsRef]);
  return { count: shapes.length, click, apply };
}
