// Draw controller for forms whose shape is set by one drag (centre to
// radius, corner to corner): the handlers receive job points and the form
// decides what the drag means. Handlers are read through refs so the
// controller never needs rebuilding.

import { useEffect, useMemo, useRef } from 'react';

import type { Point } from '../bridge/document';
import type { DrawController, DrawPointer } from '../state/drawStore';
import type { QuickEntry } from './quickKeys';

export interface ShapeDragHandlers {
  /** Pointer moved while dragging from `from`. */
  onDrag(from: Point, to: Point, pointer: DrawPointer): void;
  /** Released after a drag: create the shape. */
  onCommit(from: Point, to: Point, pointer: DrawPointer): void;
  /** Released without dragging: place the shape at the point. */
  onClick(point: Point, pointer: DrawPointer): void;
  /** Enter pressed while the view has focus. */
  onEnter(): void;
  /**
   * Quick Keys typed while dragging from `from` (the pointer is at
   * `cursor`): create the shape they describe and return true, or return
   * false when the entry does not fit the tool.
   */
  onQuick?(from: Point, cursor: Point, entry: QuickEntry): boolean;
}

const DRAG_MIN_MM = 0.01;

/** What a pointer or key event of the drag asks the form to do. */
export type DragStep =
  | { kind: 'drag'; from: Point; to: Point; pointer: DrawPointer }
  | { kind: 'commit'; from: Point; to: Point; pointer: DrawPointer }
  | { kind: 'click'; point: Point; pointer: DrawPointer }
  | { kind: 'enter' }
  | null;

/**
 * The drag in progress. Enter during a drag finishes the shape there, and
 * the release that follows makes nothing more.
 */
export class ShapeDrag {
  private start: Point | null = null;
  private dragged = false;
  private last: Point | null = null;
  private pointer: DrawPointer | null = null;

  down(p: DrawPointer): void {
    this.start = { x: p.x, y: p.y };
    this.dragged = false;
    this.last = { x: p.x, y: p.y };
    this.pointer = p;
  }

  move(p: DrawPointer): DragStep {
    const start = this.start;
    if (start === null) {
      return null;
    }
    this.last = { x: p.x, y: p.y };
    this.pointer = p;
    if (this.dragged || Math.hypot(p.x - start.x, p.y - start.y) > DRAG_MIN_MM) {
      this.dragged = true;
      return { kind: 'drag', from: start, to: { x: p.x, y: p.y }, pointer: p };
    }
    return null;
  }

  up(p: DrawPointer): DragStep {
    const start = this.start;
    const dragged = this.dragged;
    this.reset();
    if (start === null) {
      return null;
    }
    return dragged
      ? { kind: 'commit', from: start, to: { x: p.x, y: p.y }, pointer: p }
      : { kind: 'click', point: { x: p.x, y: p.y }, pointer: p };
  }

  enter(): DragStep {
    const { start, dragged, last, pointer } = this;
    this.reset();
    if (start !== null && dragged && last !== null && pointer !== null) {
      return { kind: 'commit', from: start, to: last, pointer };
    }
    return { kind: 'enter' };
  }

  /** Where Quick Keys typed now would start from and point at, or null. */
  quick(): { from: Point; cursor: Point } | null {
    return this.start === null ? null : { from: this.start, cursor: this.last ?? this.start };
  }

  reset(): void {
    this.start = null;
    this.dragged = false;
    this.last = null;
    this.pointer = null;
  }
}

export function useShapeDrag(handlers: ShapeDragHandlers): DrawController {
  const handlersRef = useRef(handlers);

  useEffect(() => {
    handlersRef.current = handlers;
  }, [handlers]);

  const takesQuickKeys = handlers.onQuick !== undefined;

  return useMemo<DrawController>(() => {
    const drag = new ShapeDrag();
    const run = (step: DragStep) => {
      if (step === null) {
        return;
      }
      const h = handlersRef.current;
      if (step.kind === 'drag') {
        h.onDrag(step.from, step.to, step.pointer);
      } else if (step.kind === 'commit') {
        h.onCommit(step.from, step.to, step.pointer);
      } else if (step.kind === 'click') {
        h.onClick(step.point, step.pointer);
      } else {
        h.onEnter();
      }
    };
    const controller: DrawController = {
      pointerDown: (p) => drag.down(p),
      pointerMove: (p) => run(drag.move(p)),
      pointerUp: (p) => run(drag.up(p)),
      keyDown: (event) => {
        if (event.key !== 'Enter') {
          return false;
        }
        run(drag.enter());
        return true;
      },
      cancel: () => drag.reset(),
    };
    if (takesQuickKeys) {
      controller.quickEntry = (entry) => {
        const at = drag.quick();
        const onQuick = handlersRef.current.onQuick;
        if (at === null || onQuick === undefined) {
          return false;
        }
        const applied = onQuick(at.from, at.cursor, entry);
        if (applied) {
          // The shape is made; the release that follows must not make another.
          drag.reset();
        }
        return applied;
      };
    }
    return controller;
  }, [takesQuickKeys]);
}
