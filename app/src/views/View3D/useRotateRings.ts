// The rotation rings of the Orientate Model step in the 3D view: where
// they show, and the drags and typed angles that turn the model. The view
// lays them out after each frame it draws; the import flow's store holds
// the turns and redraws the preview.

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MutableRefObject,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from 'react';

import { DEFAULT_SEGMENT_LAYOUT } from '../../bridge/meshImport';
import { useDocumentStore } from '../../state/documentStore';
import { useImportStore } from '../../state/importStore';
import type { RingTarget, ShownRing } from '../RotateRings/RotateRings';
import type { View3DRenderer } from './renderer';
import {
  angleBetween,
  axisOffsetAfter,
  axisVector,
  freeTurn,
  onRingPlane,
  RING_AXES,
  ringAngle,
  ringFrame,
  ringPoints,
  turnedAbout,
  turnKey,
  type RingAxis,
  type RingFrame,
  type Turns,
} from './rotateRings';

/** The ball's radius as a share of the rings'. */
const BALL_SHARE = 0.14;
/** Smallest ball, CSS pixels. */
const MIN_BALL = 9;
/** A press that moves less than this is a click, CSS pixels. */
const CLICK_SLOP = 3;
/** Free turn per pixel dragged on the ball, radians. */
const BALL_PER_PIXEL = 0.01;
/** The axis cube's half side as a share of the ball's radius. */
const CUBE_SHARE = 0.45;

interface Drag {
  target: RingTarget;
  pointerId: number;
  start: Turns;
  /** Where the press started on the ring, radians round it. */
  startAngle: number | null;
  screen: { x: number; y: number };
  moved: boolean;
  /** Where the axis cube drag was last, CSS pixels. */
  last: { x: number; y: number };
}

export interface RingsOverlay {
  rings: ShownRing[];
  ball: { x: number; y: number; r: number } | null;
  cube: { x: number; y: number; r: number } | null;
}

/** Whether the axis cube shows: Rotation Axis Movement on a full 3D model
 * laid on a rotary job's axis. */
function axisCubeShown(): boolean {
  const flow = useImportStore.getState();
  return flow.axisMove && flow.view.ringSegment === null && flow.preview?.cylinder !== undefined;
}

export interface RotateRingsControl {
  shown: RingsOverlay | null;
  active: RingTarget | null;
  field: { axis: RingAxis; degrees: number; x: number; y: number } | null;
  /** Gives the rings the model's scene positions, or none. */
  setModel: (positions: ArrayLike<number> | null) => void;
  onDown: (target: RingTarget, event: ReactPointerEvent<SVGElement>) => void;
  onMove: (event: ReactPointerEvent<SVGElement>) => void;
  onUp: (event: ReactPointerEvent<SVGElement>) => void;
  onAngle: (axis: RingAxis, degrees: number) => void;
  closeField: () => void;
}

/** The turns the rings change: the model's (Orientate Model), or one
 * segment's layout turns (3D Segmenting, layout mode, O). */
function turnsNow(): Turns {
  const flow = useImportStore.getState();
  const segment = flow.view.ringSegment;
  if (segment !== null) {
    const layout = flow.plan.layouts[segment];
    return { rotX: layout?.rotX ?? 0, rotY: layout?.rotY ?? 0, rotZ: layout?.rotZ ?? 0 };
  }
  const { rotX, rotY, rotZ } = flow.orient;
  return { rotX, rotY, rotZ };
}

function setTurns(patch: Partial<Turns>): void {
  const flow = useImportStore.getState();
  const segment = flow.view.ringSegment;
  if (segment === null) {
    flow.setOrient(patch);
    return;
  }
  const plan = flow.plan;
  flow.setPlan({
    ...plan,
    layouts: {
      ...plan.layouts,
      [segment]: { ...DEFAULT_SEGMENT_LAYOUT, ...plan.layouts[segment], ...patch },
    },
  });
}

export function useRotateRings(
  rendererRef: RefObject<View3DRenderer | null>,
  canvasRef: RefObject<HTMLCanvasElement | null>,
  layoutRef: MutableRefObject<(() => void) | null>,
  scheduleDraw: () => void,
): RotateRingsControl {
  const frameRef = useRef<RingFrame | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const refreshing = useRef<{ busy: boolean; again: boolean }>({ busy: false, again: false });
  const [shown, setShown] = useState<RingsOverlay | null>(null);
  const [active, setActive] = useState<RingTarget | null>(null);
  const [field, setField] = useState<RotateRingsControl['field']>(null);

  const layout = useCallback(() => {
    const renderer = rendererRef.current;
    const frame = frameRef.current;
    if (renderer === null || frame === null) {
      setShown((old) => (old === null ? old : null));
      return;
    }
    const rings: ShownRing[] = [];
    for (const axis of RING_AXES) {
      const points = ringPoints(frame, axis).map((p) => renderer.projectJob(p));
      if (points.some((p) => p === null)) {
        continue;
      }
      const path = points
        .map((p, i) => `${i === 0 ? 'M' : 'L'}${(p?.x ?? 0).toFixed(1)} ${(p?.y ?? 0).toFixed(1)}`)
        .join(' ');
      rings.push({ axis, path });
    }
    const centre = renderer.projectJob(frame.centre);
    const { right } = renderer.jobCameraAxes();
    const edge = renderer.projectJob([
      frame.centre[0] + right[0] * frame.radius * BALL_SHARE,
      frame.centre[1] + right[1] * frame.radius * BALL_SHARE,
      frame.centre[2] + right[2] * frame.radius * BALL_SHARE,
    ]);
    const ball =
      centre === null || edge === null
        ? null
        : {
            x: centre.x,
            y: centre.y,
            r: Math.max(MIN_BALL, Math.hypot(edge.x - centre.x, edge.y - centre.y)),
          };
    const cube =
      ball !== null && axisCubeShown() ? { x: ball.x, y: ball.y, r: ball.r * CUBE_SHARE } : null;
    setShown((old) =>
      old !== null &&
      old.ball?.x === ball?.x &&
      old.ball?.y === ball?.y &&
      (old.cube === null) === (cube === null) &&
      old.rings.map((r) => r.path).join() === rings.map((r) => r.path).join()
        ? old
        : { rings, ball, cube },
    );
  }, [rendererRef]);
  useEffect(() => {
    layoutRef.current = layout;
    return () => {
      layoutRef.current = null;
    };
  }, [layout, layoutRef]);

  const setModel = useCallback(
    (positions: ArrayLike<number> | null) => {
      // A drag keeps the rings where they started, so they do not slide
      // about as the model turns under them.
      if (dragRef.current === null) {
        frameRef.current = positions === null ? null : ringFrame(positions);
      }
      if (positions === null) {
        setField(null);
      }
      scheduleDraw();
    },
    [scheduleDraw],
  );

  /** Redraws the preview; turns made meanwhile redraw once more after it. */
  const refresh = useCallback(() => {
    const state = refreshing.current;
    if (state.busy) {
      state.again = true;
      return;
    }
    state.busy = true;
    const run = async () => {
      do {
        state.again = false;
        await useImportStore.getState().refresh();
      } while (state.again);
      state.busy = false;
    };
    void run();
  }, []);

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

  const angleAt = (axis: RingAxis, event: ReactPointerEvent<SVGElement>): number | null => {
    const frame = frameRef.current;
    const ray = pointerRay(event);
    if (frame === null || ray === null) {
      return null;
    }
    const hit = onRingPlane(frame, axis, ray.origin, ray.direction);
    return hit === null ? null : ringAngle(frame, axis, hit);
  };

  const screenOf = (event: ReactPointerEvent<SVGElement>) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    return { x: event.clientX - (rect?.left ?? 0), y: event.clientY - (rect?.top ?? 0) };
  };

  const onDown = (target: RingTarget, event: ReactPointerEvent<SVGElement>) => {
    if (event.button !== 0 || dragRef.current !== null) {
      return;
    }
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    setField(null);
    dragRef.current = {
      target,
      pointerId: event.pointerId,
      start: turnsNow(),
      startAngle: target === 'ball' || target === 'axis' ? null : angleAt(target, event),
      screen: screenOf(event),
      moved: false,
      last: screenOf(event),
    };
    setActive(target);
  };

  const onMove = (event: ReactPointerEvent<SVGElement>) => {
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) {
      return;
    }
    const now = screenOf(event);
    const dx = now.x - drag.screen.x;
    const dy = now.y - drag.screen.y;
    if (!drag.moved && Math.hypot(dx, dy) < CLICK_SLOP) {
      return;
    }
    drag.moved = true;
    if (drag.target === 'axis') {
      // Rotation Axis Movement: the axis follows the cube across the view.
      const renderer = rendererRef.current;
      const flow = useImportStore.getState();
      const axis = useDocumentStore.getState().job.rotary.axis;
      if (renderer !== null) {
        const delta = renderer.panDelta(now.x - drag.last.x, now.y - drag.last.y);
        drag.last = now;
        flow.setRotaryFit({ offset: axisOffsetAfter(flow.rotaryFit.offset, delta, axis) });
        refresh();
      }
      return;
    }
    let turns: Turns | null = null;
    if (drag.target === 'ball') {
      const renderer = rendererRef.current;
      if (renderer !== null) {
        const { right, up } = renderer.jobCameraAxes();
        turns = freeTurn(drag.start, right, up, dx, dy, BALL_PER_PIXEL);
      }
    } else if (drag.startAngle !== null) {
      const angle = angleAt(drag.target, event);
      if (angle !== null) {
        const turn = angleBetween(drag.startAngle, angle);
        turns = turnedAbout(drag.start, axisVector(drag.target), turn);
      }
    }
    if (turns !== null) {
      setTurns(turns);
      refresh();
    }
  };

  const onUp = (event: ReactPointerEvent<SVGElement>) => {
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) {
      return;
    }
    dragRef.current = null;
    setActive(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (!drag.moved && drag.target !== 'ball' && drag.target !== 'axis') {
      const at = screenOf(event);
      setField({
        axis: drag.target,
        degrees: turnsNow()[turnKey(drag.target)],
        x: at.x,
        y: at.y,
      });
    }
    refresh();
  };

  const onAngle = (axis: RingAxis, degrees: number) => {
    setTurns({ [turnKey(axis)]: degrees });
    setField(null);
    refresh();
  };

  return {
    shown,
    active,
    field,
    setModel,
    onDown,
    onMove,
    onUp,
    onAngle,
    closeField: () => setField(null),
  };
}
