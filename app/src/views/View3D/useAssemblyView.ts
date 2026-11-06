// The Assembly View in the 3D view: shows the pieces while its form is
// open, picks a piece on a click (or, while a mate waits for one, a face of
// it), and moves or turns the picked one with the arrows, rings and ball
// drawn over it. A drag shows its placement at once (the other pieces pale),
// snaps a face of the piece against a face of a piece near it when snapping
// is on, and places the piece in the kernel, one undo step, when the pointer
// goes up. A click on an arrow or ring opens a box to type a distance or
// angle. Snapping lays a flat face of the dragged piece (its sides, pocket
// floors, slot walls) against or in line with one of a piece near it.
// While a mate waits for a face, the face under the pointer is lit. While
// the pieces are exploded they show moved apart and the handles hide.
// A component being placed (Place in the add list, or dragged from the
// Component Tree onto the view) shows as a ghost standing on the piece or
// the ground under the pointer; a click (or letting go of the drag) lands
// it there and Esc cancels.

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MutableRefObject,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from 'react';

import { pieceKey, pieceRef, type PieceRow, type Triple } from '../../bridge/assembly';
import type { Heightfield } from '../../bridge/cam';
import { toDisplay, toMillimetres } from '../../lib/units';
import { useAssemblyFitStore } from '../../state/assemblyFitStore';
import {
  componentPieceKey,
  isShown,
  placingGhost,
  shownPieces,
  useAssemblyStore,
} from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { useFurnitureStore } from '../../state/furnitureStore';
import type { GizmoField, GizmoTarget, ShownGizmo } from '../PieceGizmo/PieceGizmo';
import type { ScenePiece } from './assemblyLayer';
import { explodedPieces, explodeMoves } from './explode';
import { coveredSheets } from './furnitureSeams';
import {
  boxPlanes,
  faceRegion,
  faceSnap,
  piecePlanes,
  type LocalPlane,
  type SnapPiece,
} from './pieceFaces';
import {
  alongAxis,
  arrowLength,
  movedAlong,
  movedByScene,
  pieceBox,
  pieceLocalSceneMatrix,
  pieceSceneMatrix,
  piecesBox,
  ringsOf,
  SNAP_PIXELS,
  tidy,
  toPieceLocal,
  turnedBy,
} from './pieceGizmo';
import { landingPoint, registerPieceDrop } from './pieceDrop';
import type { View3DRenderer } from './renderer';
import {
  angleBetween,
  axisVector,
  onRingPlane,
  RING_AXES,
  ringAngle,
  ringPoints,
  type RingAxis,
  type Vec3,
} from './rotateRings';

/** Where each axis sits in a triple. */
const AXIS_INDEX: Readonly<Record<RingAxis, 0 | 1 | 2>> = { x: 0, y: 1, z: 2 };

/** A press that moves less than this is a click, CSS pixels. */
const CLICK_SLOP = 3;
/** The ball's radius, CSS pixels. */
const BALL_RADIUS = 11;

interface Drag {
  target: GizmoTarget;
  pointerId: number;
  piece: PieceRow;
  /** Where along the arrow's axis the press was, mm. */
  startAlong: number | null;
  /** Where round the ring the press was, radians. */
  startAngle: number | null;
  screen: { x: number; y: number };
  moved: boolean;
}

export interface AssemblyViewControl {
  /** Whether the Assembly View is showing. */
  active: boolean;
  /** Whether a component waits for a click to land it. */
  placing: boolean;
  shown: ShownGizmo | null;
  dragging: GizmoTarget | null;
  field: GizmoField | null;
  /** Picks the piece under a click (normalised device coordinates). */
  pickAt: (ndcX: number, ndcY: number) => void;
  /** The pointer moved over the view (null: it left); lights a face while a mate waits. */
  hoverAt: (ndcX: number | null, ndcY?: number) => void;
  onDown: (target: GizmoTarget, event: ReactPointerEvent<SVGElement>) => void;
  onMove: (event: ReactPointerEvent<SVGElement>) => void;
  onUp: (event: ReactPointerEvent<SVGElement>) => void;
  onValue: (kind: 'move' | 'turn', axis: RingAxis, value: number) => void;
  closeField: () => void;
}

/** Each relief's flat faces, worked out once. */
const planesOfRelief = new WeakMap<Heightfield, LocalPlane[]>();

function planesOf(relief: Heightfield | undefined, size: Triple): LocalPlane[] {
  if (relief === undefined) {
    return boxPlanes(size);
  }
  let planes = planesOfRelief.get(relief);
  if (planes === undefined) {
    planes = piecePlanes(relief);
    planesOfRelief.set(relief, planes);
  }
  return planes.length > 0 ? planes : boxPlanes(size);
}

function snapPiece(piece: PieceRow, relief: Heightfield | undefined): SnapPiece {
  return { piece, box: pieceBox(piece), planes: planesOf(relief, piece.size) };
}

/** A piece where the view shows it: moved out as far as the pieces are exploded. */
function shownAt(piece: PieceRow): PieceRow {
  const { offsets, explode } = useAssemblyFitStore.getState();
  const moves = explodeMoves(offsets, explode, useAssemblyStore.getState().explodeSteps);
  return explodedPieces([piece], moves, 1)[0] ?? piece;
}

function toSceneBox(box: { min: Vec3; max: Vec3 }): { min: Vec3; max: Vec3 } {
  return {
    min: [box.min[0], box.min[2], -box.max[1]],
    max: [box.max[0], box.max[2], -box.min[1]],
  };
}

export function useAssemblyView(
  rendererRef: RefObject<View3DRenderer | null>,
  canvasRef: RefObject<HTMLCanvasElement | null>,
  layoutRef: MutableRefObject<(() => void) | null>,
  scheduleDraw: () => void,
  rendererEpoch: number,
): AssemblyViewControl {
  const active = useAssemblyStore((s) => s.active);
  const pieces = useAssemblyStore((s) => s.pieces);
  const reliefs = useAssemblyStore((s) => s.reliefs);
  const selected = useAssemblyStore((s) => s.selected);
  const isolated = useAssemblyStore((s) => s.isolated);
  const live = useAssemblyStore((s) => s.live);
  const hover = useAssemblyStore((s) => s.hover);
  const picking = useAssemblyStore((s) => s.mate.step !== null);
  const placing = useAssemblyStore((s) => s.placing);
  const ghosts = useAssemblyStore((s) => s.ghosts);
  const units = useDocumentStore((s) => s.job.units);
  const explode = useAssemblyFitStore((s) => s.explode);
  const offsets = useAssemblyFitStore((s) => s.offsets);
  const steps = useAssemblyStore((s) => s.explodeSteps);
  const furnitureSheets = useFurnitureStore(coveredSheets);
  const moves = useMemo(() => explodeMoves(offsets, explode, steps), [explode, offsets, steps]);
  const dragRef = useRef<Drag | null>(null);
  const framedRef = useRef(false);
  const [shown, setShown] = useState<ShownGizmo | null>(null);
  const [dragging, setDragging] = useState<GizmoTarget | null>(null);
  const [field, setField] = useState<GizmoField | null>(null);

  // The pieces in the scene, framed once when the view opens.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    if (!active) {
      renderer.setAssembly(null, null);
      framedRef.current = false;
      scheduleDraw();
      return;
    }
    // While the Furniture Designer's preview stands in for a made design,
    // that design's pieces stay out, so the two do not show in one place.
    const covered = new Set(furnitureSheets.split(',').filter(Boolean).map(Number));
    const placed = explodedPieces(shownPieces(pieces, live), moves, 1).filter(
      (p) => (p.component !== undefined && p.component !== null) || !covered.has(p.sheet),
    );
    const scene: ScenePiece[] = [];
    // Hidden pieces (and all but the isolated one) stay out of the scene,
    // so they neither show nor pick.
    for (const piece of placed.filter((p) => isShown(p, isolated))) {
      const key = pieceKey(piece);
      const relief = reliefs[key];
      if (relief !== undefined) {
        scene.push({
          key,
          field: relief,
          matrix: pieceSceneMatrix(piece, relief),
        });
      }
    }
    // Framed on where the pieces rest (apart while exploded), so a drag
    // does not refit the view.
    const box = piecesBox(explodedPieces(pieces, moves, 1));
    renderer.setAssembly(scene, box === null ? null : toSceneBox(box));
    if (!framedRef.current && box !== null) {
      framedRef.current = true;
      renderer.setView(renderer.fitted(renderer.view()));
    }
    scheduleDraw();
  }, [
    active,
    furnitureSheets,
    isolated,
    live,
    moves,
    pieces,
    reliefs,
    rendererEpoch,
    rendererRef,
    scheduleDraw,
  ]);

  useEffect(() => {
    rendererRef.current?.setAssemblyHighlight(selected, live !== null);
    scheduleDraw();
  }, [live, rendererRef, scheduleDraw, selected]);

  // The face under the pointer while a mate waits for one.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    const piece = hover === null ? undefined : pieces.find((p) => pieceKey(p) === hover.key);
    const relief = hover === null ? undefined : reliefs[hover.key];
    if (!active || !picking || hover === null || piece === undefined || relief === undefined) {
      renderer.setAssemblyFace(null, null);
    } else {
      renderer.setAssemblyFace(
        faceRegion(relief, hover.face),
        pieceLocalSceneMatrix(explodedPieces([piece], moves, 1)[0] ?? piece),
      );
    }
    scheduleDraw();
  }, [active, hover, moves, pieces, picking, reliefs, rendererRef, scheduleDraw]);

  // The ghost of a component being placed, where it would land.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    const ghost = active ? placingGhost(placing, pieces, reliefs, ghosts) : null;
    renderer.setAssemblyGhost(
      ghost === null ? null : { field: ghost.field, matrix: pieceSceneMatrix(ghost, ghost.field) },
    );
    scheduleDraw();
  }, [active, ghosts, pieces, placing, reliefs, rendererRef, scheduleDraw]);

  // Esc cancels placing a component.
  const waiting = active && placing !== null;
  useEffect(() => {
    if (!waiting) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        useAssemblyStore.getState().cancelPlacing();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [waiting]);

  /** Where a component being placed lands under a canvas point: the point and the piece it is on. */
  const landingAt = useCallback(
    (ndcX: number, ndcY: number) => {
      const renderer = rendererRef.current;
      const store = useAssemblyStore.getState();
      const component = store.placing?.component;
      if (renderer === null) {
        return null;
      }
      const skip = component === undefined ? null : componentPieceKey(component);
      const refOf = (key: string) => {
        const row = store.pieces.find((p) => pieceKey(p) === key);
        return row === undefined ? null : pieceRef(row);
      };
      return landingPoint(renderer, ndcX, ndcY, skip, refOf);
    },
    [rendererRef],
  );

  // Components dragged from the Component Tree land where they are let go.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!active || canvas === null) {
      return undefined;
    }
    return registerPieceDrop(canvas, (x, y) => {
      const rect = canvas.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) {
        return null;
      }
      return landingAt(
        ((x - rect.left) / rect.width) * 2 - 1,
        -(((y - rect.top) / rect.height) * 2 - 1),
      );
    });
  }, [active, canvasRef, landingAt]);

  // Where the handles show, after each frame the view draws.
  const layout = useCallback(() => {
    const renderer = rendererRef.current;
    const store = useAssemblyStore.getState();
    const piece = shownPieces(store.pieces, store.live).find((p) => pieceKey(p) === store.selected);
    // The handles place the assembled pieces, so they hide while exploded;
    // a grounded piece stays put and a hidden one does not show.
    const apart = useAssemblyFitStore.getState().explode > 0;
    const fixed = piece === undefined || piece.grounded === true || !isShown(piece, store.isolated);
    if (renderer === null || !store.active || piece === undefined || apart || fixed) {
      setShown((old) => (old === null ? old : null));
      return;
    }
    const centre: Vec3 = [piece.position[0], piece.position[1], piece.position[2]];
    const middle = renderer.projectJob(centre);
    if (middle === null) {
      setShown((old) => (old === null ? old : null));
      return;
    }
    const length = arrowLength(piece.size);
    const arrows = RING_AXES.flatMap((axis) => {
      const tip = renderer.projectJob(movedAlong(piece.position, axis, length));
      return tip === null ? [] : [{ axis, x1: middle.x, y1: middle.y, x2: tip.x, y2: tip.y }];
    });
    const frame = ringsOf(piece);
    const rings = RING_AXES.flatMap((axis) => {
      const points = ringPoints(frame, axis).map((p) => renderer.projectJob(p));
      if (points.some((p) => p === null)) {
        return [];
      }
      const path = points
        .map((p, i) => `${i === 0 ? 'M' : 'L'}${(p?.x ?? 0).toFixed(1)} ${(p?.y ?? 0).toFixed(1)}`)
        .join(' ');
      return [{ axis, path }];
    });
    const next: ShownGizmo = { arrows, rings, ball: { ...middle, r: BALL_RADIUS } };
    setShown((old) => (old !== null && JSON.stringify(old) === JSON.stringify(next) ? old : next));
  }, [rendererRef]);
  useEffect(() => {
    layoutRef.current = layout;
    return () => {
      layoutRef.current = null;
    };
  }, [layout, layoutRef]);
  useEffect(() => {
    scheduleDraw();
  }, [explode, isolated, pieces, selected, scheduleDraw]);

  const pointerRay = (event: ReactPointerEvent<SVGElement>) => {
    const canvas = canvasRef.current;
    const renderer = rendererRef.current;
    if (canvas === null || renderer === null) {
      return null;
    }
    const rect = canvas.getBoundingClientRect();
    const x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    const y = -(((event.clientY - rect.top) / rect.height) * 2 - 1);
    return renderer.jobRay(x, y);
  };

  const screenOf = (event: ReactPointerEvent<SVGElement>) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    return { x: event.clientX - (rect?.left ?? 0), y: event.clientY - (rect?.top ?? 0) };
  };

  const alongAt = (piece: PieceRow, axis: RingAxis, event: ReactPointerEvent<SVGElement>) => {
    const ray = pointerRay(event);
    return ray === null
      ? null
      : alongAxis(
          [piece.position[0], piece.position[1], piece.position[2]],
          axisVector(axis),
          ray.origin,
          ray.direction,
        );
  };

  const angleAt = (piece: PieceRow, axis: RingAxis, event: ReactPointerEvent<SVGElement>) => {
    const ray = pointerRay(event);
    const frame = ringsOf(piece);
    const hit = ray === null ? null : onRingPlane(frame, axis, ray.origin, ray.direction);
    return hit === null ? null : ringAngle(frame, axis, hit);
  };

  /** Millimetres a few pixels cover at the piece, or null off screen. */
  const snapTolerance = (piece: PieceRow): number | null => {
    const renderer = rendererRef.current;
    const middle = renderer?.projectJob([piece.position[0], piece.position[1], piece.position[2]]);
    if (renderer === null || renderer === undefined || middle === null || middle === undefined) {
      return null;
    }
    const length = arrowLength(piece.size);
    let pixels = 0;
    for (const axis of RING_AXES) {
      const tip = renderer.projectJob(movedAlong(piece.position, axis, length));
      if (tip !== null) {
        pixels = Math.max(pixels, Math.hypot(tip.x - middle.x, tip.y - middle.y));
      }
    }
    return pixels > 0 ? (SNAP_PIXELS * length) / pixels : null;
  };

  const selectedPiece = (): PieceRow | undefined => {
    const store = useAssemblyStore.getState();
    return store.pieces.find((p) => pieceKey(p) === store.selected);
  };

  const onDown = (target: GizmoTarget, event: ReactPointerEvent<SVGElement>) => {
    const piece = selectedPiece();
    if (event.button !== 0 || dragRef.current !== null || piece === undefined) {
      return;
    }
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    setField(null);
    dragRef.current = {
      target,
      pointerId: event.pointerId,
      piece,
      startAlong: target.kind === 'move' ? alongAt(piece, target.axis, event) : null,
      startAngle: target.kind === 'turn' ? angleAt(piece, target.axis, event) : null,
      screen: screenOf(event),
      moved: false,
    };
    setDragging(target);
  };

  const onMove = (event: ReactPointerEvent<SVGElement>) => {
    const drag = dragRef.current;
    const renderer = rendererRef.current;
    if (drag === null || drag.pointerId !== event.pointerId || renderer === null) {
      return;
    }
    const now = screenOf(event);
    const dx = now.x - drag.screen.x;
    const dy = now.y - drag.screen.y;
    if (!drag.moved && Math.hypot(dx, dy) < CLICK_SLOP) {
      return;
    }
    drag.moved = true;
    const { piece, target } = drag;
    let position: Triple = piece.position;
    let rotation: Triple = piece.rotation;
    let shownValue: GizmoField | null = null;
    if (target.kind === 'move' && drag.startAlong !== null) {
      const along = alongAt(piece, target.axis, event);
      if (along === null) {
        return;
      }
      const distance = along - drag.startAlong;
      position = movedAlong(piece.position, target.axis, distance);
      shownValue = {
        kind: 'move',
        axis: target.axis,
        value: tidy(toDisplay(distance, units)),
        x: now.x,
        y: now.y,
        editable: false,
      };
    } else if (target.kind === 'turn' && drag.startAngle !== null) {
      const angle = angleAt(piece, target.axis, event);
      if (angle === null) {
        return;
      }
      const degrees = (angleBetween(drag.startAngle, angle) * 180) / Math.PI;
      rotation = turnedBy(piece.rotation, target.axis, degrees);
      shownValue = {
        kind: 'turn',
        axis: target.axis,
        value: tidy(degrees),
        x: now.x,
        y: now.y,
        editable: false,
      };
    } else if (target.kind === 'ball') {
      position = movedByScene(piece.position, renderer.panDelta(dx, dy));
    }
    const store = useAssemblyStore.getState();
    if (store.snap && target.kind !== 'turn') {
      const tolerance = snapTolerance(piece);
      if (tolerance !== null) {
        const key = pieceKey(piece);
        const others = store.pieces
          .filter((p) => pieceKey(p) !== key && isShown(p, store.isolated))
          .map((p) => snapPiece(p, store.reliefs[pieceKey(p)]));
        const axis = target.kind === 'move' ? AXIS_INDEX[target.axis] : null;
        const moving = snapPiece({ ...piece, position }, store.reliefs[key]);
        const shift = faceSnap(moving, others, axis, tolerance);
        position = [position[0] + shift[0], position[1] + shift[1], position[2] + shift[2]];
        if (shownValue !== null && target.kind === 'move') {
          const moved = position[AXIS_INDEX[target.axis]] - piece.position[AXIS_INDEX[target.axis]];
          shownValue = { ...shownValue, value: tidy(toDisplay(moved, units)) };
        }
      }
    }
    setField(shownValue);
    store.setLive({ key: pieceKey(piece), position, rotation });
  };

  const onUp = (event: ReactPointerEvent<SVGElement>) => {
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) {
      return;
    }
    dragRef.current = null;
    setDragging(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    const store = useAssemblyStore.getState();
    const placed = store.live;
    if (drag.moved && placed !== null && event.type === 'pointerup') {
      setField(null);
      void store.place(placed.key, placed.position, placed.rotation);
      return;
    }
    store.setLive(null);
    setField(null);
    if (!drag.moved && drag.target.kind !== 'ball') {
      const at = screenOf(event);
      setField({
        kind: drag.target.kind,
        axis: drag.target.axis,
        value: 0,
        x: at.x,
        y: at.y,
        editable: true,
      });
    }
  };

  const onValue = (kind: 'move' | 'turn', axis: RingAxis, value: number) => {
    const piece = selectedPiece();
    setField(null);
    if (piece === undefined || piece.grounded === true) {
      return;
    }
    const position =
      kind === 'move'
        ? movedAlong(piece.position, axis, toMillimetres(value, units))
        : piece.position;
    const rotation = kind === 'turn' ? turnedBy(piece.rotation, axis, value) : piece.rotation;
    void useAssemblyStore.getState().place(pieceKey(piece), position, rotation);
  };

  const pickAt = (ndcX: number, ndcY: number) => {
    const renderer = rendererRef.current;
    if (renderer === null) {
      return;
    }
    setField(null);
    const store = useAssemblyStore.getState();
    if (store.placing !== null && !store.placing.dragging) {
      // Place waits for a click: the component lands where it shows.
      const at = landingAt(ndcX, ndcY);
      if (at !== null) {
        void store.dropComponent(store.placing.component, at);
      }
      return;
    }
    const hit = renderer.pickPiece(ndcX, ndcY);
    const fit = useAssemblyFitStore.getState();
    if (fit.measure.step !== null) {
      // Measure waits for a point: the one clicked on a piece, in its frame.
      const piece = store.pieces.find((p) => pieceKey(p) === hit?.key);
      if (hit !== null && piece !== undefined) {
        const point: Vec3 = [hit.point[0], hit.point[1], hit.point[2]];
        void fit.measurePick({ ...pieceRef(piece), point: toPieceLocal(shownAt(piece), point) });
      }
      return;
    }
    if (store.mate.step === null) {
      store.select(hit?.key ?? null);
      return;
    }
    const piece = store.pieces.find((p) => pieceKey(p) === hit?.key);
    if (hit === null || piece === undefined) {
      return;
    }
    const point: Vec3 = [hit.point[0], hit.point[1], hit.point[2]];
    void store.pickFace({ ...pieceRef(piece), point: toPieceLocal(shownAt(piece), point) });
  };

  const hoverAt = (ndcX: number | null, ndcY = 0) => {
    const renderer = rendererRef.current;
    const store = useAssemblyStore.getState();
    if (store.active && store.placing !== null && !store.placing.dragging) {
      store.movePlacing(ndcX === null ? null : landingAt(ndcX, ndcY));
      return;
    }
    if (renderer === null || !store.active || store.mate.step === null || ndcX === null) {
      store.hoverFace(null);
      return;
    }
    const hit = renderer.pickPiece(ndcX, ndcY);
    const piece = store.pieces.find((p) => pieceKey(p) === hit?.key);
    if (hit === null || piece === undefined) {
      store.hoverFace(null);
      return;
    }
    const point: Vec3 = [hit.point[0], hit.point[1], hit.point[2]];
    store.hoverFace({ ...pieceRef(piece), point: toPieceLocal(shownAt(piece), point) });
  };

  return {
    active,
    placing: waiting && placing?.dragging !== true,
    shown,
    dragging,
    // A box left open when the view closed does not come back with it.
    field: active ? field : null,
    pickAt,
    hoverAt,
    onDown,
    onMove,
    onUp,
    onValue,
    closeField: () => setField(null),
  };
}
