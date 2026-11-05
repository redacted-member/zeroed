// The Joint Creator's clicks in the 2D view: what the pointer is over (the
// edge or the place on a face that a click would pick, or why it cannot),
// asked of the kernel one question at a time, and the click itself.

import { useEffect, useMemo, useRef, useState, type MutableRefObject } from 'react';

import { api } from '../../bridge/formApi';
import type { Point } from '../../bridge/document';
import type { PickedEdge } from '../../bridge/joints';
import { useDrawController } from '../../lib/useDrawController';
import type { DrawController } from '../../state/drawStore';
import { useViewStore } from '../../state/viewStore';
import { kindInfo, stage, type JointFormState, type PickedB } from './model';

/** What a click at the pointer would pick. */
export type Hover =
  | { at: Point; pick: 'a'; edge: PickedEdge }
  | { at: Point; pick: 'b'; b: PickedB }
  | { at: Point; pick: 'none'; reason: string };

export interface PickHandlers {
  /** A click picked the first part's edge. */
  pickA: (edge: PickedEdge) => void;
  /** A click picked part B. */
  pickB: (b: PickedB) => void;
  /** Esc: step back; answers whether there was anything to step back from. */
  back: () => boolean;
}

function tolerance(): number {
  const view = useViewStore.getState();
  return view.camera.pxPerMm > 0 ? view.snapRadiusPx / view.camera.pxPerMm : 1;
}

/** Asks the kernel what a click at `at` would pick in the panel's state. */
async function probe(state: JointFormState, at: Point): Promise<Hover | null> {
  const now = stage(state);
  if (now === 'box' || now === 'ready') {
    return null;
  }
  if (now === 'pickA' || kindInfo(state.kind).target === 'edge') {
    const reply = await api.joints.pickEdge(at.x, at.y, tolerance());
    if (reply.edge === null) {
      return { at, pick: 'none', reason: reply.reason ?? '' };
    }
    if (now === 'pickA') {
      return { at, pick: 'a', edge: reply.edge };
    }
    if (state.a !== null && reply.edge.vector === state.a.vector) {
      return { at, pick: 'none', reason: 'samePart' };
    }
    return { at, pick: 'b', b: { kind: 'edge', edge: reply.edge } };
  }
  const reply = await api.joints.pickFace(at.x, at.y, state.a);
  if (reply.face === null) {
    return { at, pick: 'none', reason: reply.reason ?? '' };
  }
  return { at, pick: 'b', b: { kind: 'face', ...reply.face } };
}

/**
 * Registers the panel's draw controller and answers what the pointer is
 * over. `stateRef` is the panel's live state.
 */
export function usePicks(
  stateRef: MutableRefObject<JointFormState>,
  handlers: PickHandlers,
): { hover: Hover | null; clearHover: () => void } {
  const [hover, setHover] = useState<Hover | null>(null);
  const handlersRef = useRef(handlers);
  useEffect(() => {
    handlersRef.current = handlers;
  }, [handlers]);

  // One question in flight; a newer pointer position waits and replaces
  // any older one waiting.
  const queue = useRef<{ busy: boolean; next: Point | null; live: boolean }>({
    busy: false,
    next: null,
    live: true,
  });
  useEffect(() => {
    const q = queue.current;
    q.live = true;
    return () => {
      q.live = false;
    };
  }, []);

  const controller = useMemo<DrawController>(() => {
    const ask = (at: Point) => {
      const q = queue.current;
      if (q.busy) {
        q.next = at;
        return;
      }
      q.busy = true;
      void probe(stateRef.current, at)
        .then((found) => {
          if (q.live) {
            setHover(found);
          }
        })
        .catch(() => undefined)
        .finally(() => {
          q.busy = false;
          const next = q.next;
          q.next = null;
          if (next !== null && q.live) {
            ask(next);
          }
        });
    };
    return {
      cursor: 'crosshair',
      pointerMove: (p) => ask({ x: p.rawX, y: p.rawY }),
      pointerDown: (p) => {
        const at = { x: p.rawX, y: p.rawY };
        void probe(stateRef.current, at).then((found) => {
          if (!queue.current.live || found === null) {
            return;
          }
          if (found.pick === 'a') {
            handlersRef.current.pickA(found.edge);
            setHover(null);
          } else if (found.pick === 'b') {
            handlersRef.current.pickB(found.b);
            setHover(null);
          } else {
            setHover(found);
          }
        });
      },
      pointerUp: () => undefined,
      keyDown: (event) => {
        if (event.key !== 'Escape') {
          return false;
        }
        setHover(null);
        return handlersRef.current.back();
      },
      cancel: () => setHover(null),
    };
  }, [stateRef]);
  useDrawController(controller);

  return { hover, clearHover: () => setHover(null) };
}
