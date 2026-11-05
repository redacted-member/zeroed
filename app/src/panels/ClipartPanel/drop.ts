// Dragging a clip from the Clipart tab onto a view. The drag runs on
// pointer events rather than HTML5 drag and drop, which the desktop shell's
// native file drop handler can swallow inside the web view. Each view
// registers its surface with a function that turns a screen point into the
// job point under it; the clip's centre lands there.

import { useEffect, type RefObject } from 'react';

import type { Point2 } from '../../views/View2D/camera';

/** Turns a screen point (client pixels) into the job point under it, or
 * null when nothing there can take a clip. */
export type DropResolver = (clientX: number, clientY: number) => Point2 | null;

/** How far the pointer moves, in pixels, before a press becomes a drag. */
export const DRAG_THRESHOLD = 4;

const targets = new Map<Element, DropResolver>();

/** Makes `element` a place clips can be dropped; answers the undo. */
export function registerDropTarget(element: Element, resolve: DropResolver): () => void {
  targets.set(element, resolve);
  return () => {
    if (targets.get(element) === resolve) {
      targets.delete(element);
    }
  };
}

/** Registers the element in `ref` as a drop target while mounted. The
 * resolver is read through a ref so it may change every render. */
export function useDropTarget(
  ref: RefObject<Element | null>,
  resolve: RefObject<DropResolver>,
): void {
  useEffect(() => {
    const element = ref.current;
    if (element === null) {
      return undefined;
    }
    return registerDropTarget(element, (x, y) => resolve.current(x, y));
  }, [ref, resolve]);
}

/** The resolver of the drop target holding `element` (itself or its
 * nearest registered ancestor), or null. */
export function dropTargetOf(
  element: Element | null,
  registry: ReadonlyMap<Element, DropResolver> = targets,
): DropResolver | null {
  for (let at = element; at !== null; at = at.parentElement) {
    const found = registry.get(at);
    if (found !== undefined) {
      return found;
    }
  }
  return null;
}

/** Whether a press that went down at `from` has moved far enough to be a
 * drag at `to`. */
export function isDrag(from: Point2, to: Point2, threshold: number = DRAG_THRESHOLD): boolean {
  return Math.hypot(to.x - from.x, to.y - from.y) >= threshold;
}

/** The job point a clip let go at a screen point lands on, or null when no
 * view is under it. */
export function dropPointAt(
  clientX: number,
  clientY: number,
  hit: (x: number, y: number) => Element | null = (x, y) => document.elementFromPoint(x, y),
): Point2 | null {
  const resolve = dropTargetOf(hit(clientX, clientY));
  return resolve === null ? null : resolve(clientX, clientY);
}

/**
 * Something that takes a clip dropped on it itself (a furniture panel's
 * face in the Assembly View): true when it took the clip at that screen
 * point, so it does not also land in the job.
 */
export type FaceDropHandler = (clientX: number, clientY: number, clip: string) => boolean;

const faceTargets = new Map<Element, FaceDropHandler>();

/** Makes `element` take clips on faces; answers the undo. */
export function registerFaceDrop(element: Element, handle: FaceDropHandler): () => void {
  faceTargets.set(element, handle);
  return () => {
    if (faceTargets.get(element) === handle) {
      faceTargets.delete(element);
    }
  };
}

/** Whether a face under the screen point took the clip `clip` (its path). */
export function dropOnFace(
  clientX: number,
  clientY: number,
  clip: string,
  hit: (x: number, y: number) => Element | null = (x, y) => document.elementFromPoint(x, y),
  registry: ReadonlyMap<Element, FaceDropHandler> = faceTargets,
): boolean {
  for (let at = hit(clientX, clientY); at !== null; at = at.parentElement) {
    const handle = registry.get(at);
    if (handle !== undefined) {
      return handle(clientX, clientY, clip);
    }
  }
  return false;
}
