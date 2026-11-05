// The 3D Segmenting plane's handles in the 3D view: where they show after
// each frame, and drags that move or tilt the plane in the import flow.

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MutableRefObject,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from 'react';

import type { CutPlane, Vec3 } from '../../bridge/meshImport';
import { useImportStore } from '../../state/importStore';
import { dragPlane, gripStart, handlePoints, type PlaneGrip } from './planeHandles';
import type { View3DRenderer } from './renderer';

/** A handle where it shows on the canvas, CSS pixels. */
export interface ShownPlaneHandles {
  centre: { x: number; y: number };
  move: { x: number; y: number };
  tiltU: { x: number; y: number };
  tiltV: { x: number; y: number };
}

export interface PlaneHandlesControl {
  shown: ShownPlaneHandles | null;
  active: PlaneGrip | null;
  /** The model's lowering and the handles' length, or none to hide them. */
  setFrame: (frame: { top: number; size: number } | null) => void;
  onDown: (grip: PlaneGrip, event: ReactPointerEvent<SVGElement>) => void;
  onMove: (event: ReactPointerEvent<SVGElement>) => void;
  onUp: (event: ReactPointerEvent<SVGElement>) => void;
}

interface Drag {
  grip: PlaneGrip;
  pointerId: number;
  start: CutPlane;
  from: number;
}

export function usePlaneHandles(
  rendererRef: RefObject<View3DRenderer | null>,
  canvasRef: RefObject<HTMLCanvasElement | null>,
  layoutRef: MutableRefObject<(() => void) | null>,
  scheduleDraw: () => void,
): PlaneHandlesControl {
  const frameRef = useRef<{ top: number; size: number } | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const [shown, setShown] = useState<ShownPlaneHandles | null>(null);
  const [active, setActive] = useState<PlaneGrip | null>(null);

  const layout = useCallback(() => {
    const renderer = rendererRef.current;
    const frame = frameRef.current;
    const plane = useImportStore.getState().view.plane;
    if (renderer === null || frame === null || plane === null) {
      setShown((old) => (old === null ? old : null));
      return;
    }
    const lowered = (p: Vec3): Vec3 => [p[0], p[1], p[2] - frame.top];
    const points = handlePoints(plane, frame.size);
    const project = (p: Vec3) => renderer.projectJob(lowered(p));
    const [centre, move, tiltU, tiltV] = [
      project(points.centre),
      project(points.move),
      project(points.tiltU),
      project(points.tiltV),
    ];
    if (centre === null || move === null || tiltU === null || tiltV === null) {
      setShown(null);
      return;
    }
    setShown((old) =>
      old !== null &&
      [old.centre, old.move, old.tiltU, old.tiltV].every(
        (p, i) =>
          p.x === [centre, move, tiltU, tiltV][i]?.x && p.y === [centre, move, tiltU, tiltV][i]?.y,
      )
        ? old
        : { centre, move, tiltU, tiltV },
    );
  }, [rendererRef]);
  useEffect(() => {
    layoutRef.current = layout;
    return () => {
      layoutRef.current = null;
    };
  }, [layout, layoutRef]);

  const setFrame = useCallback(
    (frame: { top: number; size: number } | null) => {
      if (dragRef.current === null) {
        frameRef.current = frame;
      }
      scheduleDraw();
    },
    [scheduleDraw],
  );

  /** The pointer's ray in the model's own frame (Z not lowered). */
  const ray = (event: ReactPointerEvent<SVGElement>) => {
    const canvas = canvasRef.current;
    const renderer = rendererRef.current;
    const frame = frameRef.current;
    if (canvas === null || renderer === null || frame === null) {
      return null;
    }
    const rect = canvas.getBoundingClientRect();
    const x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    const y = -(((event.clientY - rect.top) / rect.height) * 2 - 1);
    const { origin, direction } = renderer.jobRay(x, y);
    return {
      origin: [origin[0], origin[1], origin[2] + frame.top] as Vec3,
      direction: [direction[0], direction[1], direction[2]] as Vec3,
    };
  };

  const onDown = (grip: PlaneGrip, event: ReactPointerEvent<SVGElement>) => {
    const plane = useImportStore.getState().view.plane;
    const r = ray(event);
    if (event.button !== 0 || plane === null || r === null) {
      return;
    }
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { grip, pointerId: event.pointerId, start: plane, from: gripStart(plane, r) };
    setActive(grip);
  };

  const onMove = (event: ReactPointerEvent<SVGElement>) => {
    const drag = dragRef.current;
    const r = ray(event);
    if (drag === null || drag.pointerId !== event.pointerId || r === null) {
      return;
    }
    const plane = dragPlane(drag.start, drag.grip, drag.from, r);
    if (plane !== null) {
      useImportStore.getState().setView({ plane });
      scheduleDraw();
    }
  };

  const onUp = (event: ReactPointerEvent<SVGElement>) => {
    if (dragRef.current?.pointerId === event.pointerId) {
      event.currentTarget.releasePointerCapture(event.pointerId);
      dragRef.current = null;
      setActive(null);
    }
  };

  return { shown, active, setFrame, onDown, onMove, onUp };
}
