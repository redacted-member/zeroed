// Camera maths for the drawing view. Job space is millimetres relative to the
// job datum, with Y pointing up; screen space is CSS pixels with Y down.

import type { JobSetup, Units } from '../../bridge/types';

/** Centre of the view in job millimetres and the zoom in pixels per mm. */
export interface Camera2D {
  centreX: number;
  centreY: number;
  pxPerMm: number;
}

/** Size of the drawing surface in CSS pixels. */
export interface Viewport {
  width: number;
  height: number;
}

export interface Point2 {
  x: number;
  y: number;
}

export interface Bounds2 {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

const MIN_PX_PER_MM = 0.02;
const MAX_PX_PER_MM = 400;
const MIN_MINOR_GRID_PX = 7;
const GRID_STEPS = [0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000];

/** Position of the job datum measured from the bottom left corner, in mm. */
export function datumOffset(job: JobSetup): Point2 {
  const corner = (): Point2 => {
    switch (job.xyOrigin) {
      case 'centre':
        return { x: job.width / 2, y: job.height / 2 };
      case 'bottomRight':
        return { x: job.width, y: 0 };
      case 'topLeft':
        return { x: 0, y: job.height };
      case 'topRight':
        return { x: job.width, y: job.height };
      case 'bottomLeft':
      default:
        return { x: 0, y: 0 };
    }
  };
  const base = corner();
  return { x: base.x + job.offsetX, y: base.y + job.offsetY };
}

/** The job rectangle in job space, that is relative to the datum. */
export function jobBounds(job: JobSetup): Bounds2 {
  const datum = datumOffset(job);
  return {
    minX: 0 - datum.x,
    minY: 0 - datum.y,
    maxX: job.width - datum.x,
    maxY: job.height - datum.y,
  };
}

/** Converts a job point to a pixel position in the canvas. */
export function jobToScreen(camera: Camera2D, viewport: Viewport, point: Point2): Point2 {
  return {
    x: viewport.width / 2 + (point.x - camera.centreX) * camera.pxPerMm,
    y: viewport.height / 2 - (point.y - camera.centreY) * camera.pxPerMm,
  };
}

/** Converts a pixel position in the canvas to a job point. */
export function screenToJob(camera: Camera2D, viewport: Viewport, point: Point2): Point2 {
  return {
    x: camera.centreX + (point.x - viewport.width / 2) / camera.pxPerMm,
    y: camera.centreY - (point.y - viewport.height / 2) / camera.pxPerMm,
  };
}

/** Clamps a zoom to what the view can draw. */
export function clampZoom(pxPerMm: number): number {
  return Math.min(MAX_PX_PER_MM, Math.max(MIN_PX_PER_MM, pxPerMm));
}

/** Camera that fits the bounds into the viewport with a pixel margin. */
export function fitCamera(bounds: Bounds2, viewport: Viewport, margin = 32): Camera2D {
  const width = Math.max(bounds.maxX - bounds.minX, 1e-6);
  const height = Math.max(bounds.maxY - bounds.minY, 1e-6);
  const usableWidth = Math.max(viewport.width - margin * 2, 1);
  const usableHeight = Math.max(viewport.height - margin * 2, 1);
  return {
    centreX: (bounds.minX + bounds.maxX) / 2,
    centreY: (bounds.minY + bounds.maxY) / 2,
    pxPerMm: clampZoom(Math.min(usableWidth / width, usableHeight / height)),
  };
}

/** Zooms by a factor while keeping the job point under the pointer still. */
export function zoomAt(
  camera: Camera2D,
  viewport: Viewport,
  pointer: Point2,
  factor: number,
): Camera2D {
  const anchor = screenToJob(camera, viewport, pointer);
  const pxPerMm = clampZoom(camera.pxPerMm * factor);
  const zoomed: Camera2D = { ...camera, pxPerMm };
  const after = screenToJob(zoomed, viewport, pointer);
  return {
    centreX: zoomed.centreX + (anchor.x - after.x),
    centreY: zoomed.centreY + (anchor.y - after.y),
    pxPerMm,
  };
}

/** Pans by a pixel delta. */
export function panBy(camera: Camera2D, deltaXPx: number, deltaYPx: number): Camera2D {
  return {
    ...camera,
    centreX: camera.centreX - deltaXPx / camera.pxPerMm,
    centreY: camera.centreY + deltaYPx / camera.pxPerMm,
  };
}

/** Grid spacing in mm that keeps minor lines readable at this zoom. */
export function gridSpacing(pxPerMm: number): { minor: number; major: number } {
  const minor =
    GRID_STEPS.find((step) => step * pxPerMm >= MIN_MINOR_GRID_PX) ??
    GRID_STEPS[GRID_STEPS.length - 1] ??
    1;
  return { minor, major: minor * 10 };
}

/** Inch ruler and grid steps as [major, minor] inches, finest first. */
const INCH_STEPS: ReadonlyArray<readonly [number, number]> = [
  [1 / 8, 1 / 32],
  [1 / 4, 1 / 16],
  [1 / 2, 1 / 8],
  [1, 1 / 4],
  [2, 1 / 2],
  [5, 1],
  [10, 2],
  [20, 5],
  [50, 10],
  [100, 20],
  [200, 50],
  [500, 100],
];

const MM_PER_INCH = 25.4;

/** Ruler and grid spacing in mm at this zoom: whole inches on an inch job. */
export function unitSpacing(pxPerMm: number, units: Units): { minor: number; major: number } {
  if (units !== 'inch') {
    return gridSpacing(pxPerMm);
  }
  const step =
    INCH_STEPS.find(([, minor]) => minor * MM_PER_INCH * pxPerMm >= MIN_MINOR_GRID_PX) ??
    INCH_STEPS[INCH_STEPS.length - 1] ??
    ([1, 1 / 4] as const);
  return { major: step[0] * MM_PER_INCH, minor: step[1] * MM_PER_INCH };
}

/** Rectangle spanned by two screen points, in job space. */
export function rectBetween(a: Point2, b: Point2): Bounds2 {
  return {
    minX: Math.min(a.x, b.x),
    minY: Math.min(a.y, b.y),
    maxX: Math.max(a.x, b.x),
    maxY: Math.max(a.y, b.y),
  };
}
