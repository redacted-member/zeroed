// Pure helpers for the Edit Tabs panel: how near a click must be, when a
// press and release is a drag, and the markers drawn at the tabs.

import type { Path, Point } from '../../bridge/document';

/** Pixels a click may be from a tab or a vector. */
export const REACH_PX = 8;
/** Pixels a press must move before its release is a drag. */
export const DRAG_PX = 4;
/** Marker size in pixels. */
const MARKER_PX = 9;

/** The click reach in millimetres at a zoom of `pxPerMm`. */
export function reachMm(pxPerMm: number): number {
  return REACH_PX / Math.max(pxPerMm, 1e-9);
}

/** Whether a press at `a` and release at `b` (job millimetres) is a drag. */
export function isDrag(a: Point, b: Point, pxPerMm: number): boolean {
  return Math.hypot(b.x - a.x, b.y - a.y) * pxPerMm >= DRAG_PX;
}

/** A small square centred on each tab, the same size on screen at any zoom. */
export function tabMarkers(positions: readonly Point[], pxPerMm: number): Path[] {
  const h = MARKER_PX / 2 / Math.max(pxPerMm, 1e-9);
  return positions.map((p) => ({
    subpaths: [
      {
        start: { x: p.x - h, y: p.y - h },
        segments: [
          { kind: 'line', to: { x: p.x + h, y: p.y - h } },
          { kind: 'line', to: { x: p.x + h, y: p.y + h } },
          { kind: 'line', to: { x: p.x - h, y: p.y + h } },
          { kind: 'line', to: { x: p.x - h, y: p.y - h } },
        ],
        closed: true,
      },
    ],
  }));
}
