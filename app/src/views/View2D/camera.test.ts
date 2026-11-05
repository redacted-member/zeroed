import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB, type JobSetup } from '../../bridge/types';
import {
  datumOffset,
  fitCamera,
  gridSpacing,
  jobBounds,
  jobToScreen,
  panBy,
  screenToJob,
  zoomAt,
  type Camera2D,
  type Viewport,
} from './camera';

const viewport: Viewport = { width: 800, height: 600 };
const camera: Camera2D = { centreX: 0, centreY: 0, pxPerMm: 2 };

describe('2D camera', () => {
  it('maps the camera centre to the middle of the viewport', () => {
    expect(jobToScreen(camera, viewport, { x: 0, y: 0 })).toEqual({ x: 400, y: 300 });
  });

  it('points Y up', () => {
    expect(jobToScreen(camera, viewport, { x: 0, y: 10 }).y).toBeLessThan(300);
  });

  it('round trips screen and job coordinates', () => {
    const point = screenToJob(camera, viewport, { x: 123, y: 456 });
    const back = jobToScreen(camera, viewport, point);
    expect(back.x).toBeCloseTo(123, 9);
    expect(back.y).toBeCloseTo(456, 9);
  });

  it('keeps the point under the pointer still while zooming', () => {
    const pointer = { x: 610, y: 120 };
    const before = screenToJob(camera, viewport, pointer);
    const after = screenToJob(zoomAt(camera, viewport, pointer, 1.25), viewport, pointer);
    expect(after.x).toBeCloseTo(before.x, 9);
    expect(after.y).toBeCloseTo(before.y, 9);
  });

  it('pans by whole pixels in job units', () => {
    const panned = panBy(camera, 20, -10);
    expect(panned.centreX).toBeCloseTo(-10, 9);
    expect(panned.centreY).toBeCloseTo(-5, 9);
  });

  it('fits the job into the viewport', () => {
    const fitted = fitCamera(jobBounds(DEFAULT_JOB), viewport, 20);
    const corner = jobToScreen(fitted, viewport, { x: DEFAULT_JOB.width, y: DEFAULT_JOB.height });
    expect(corner.x).toBeLessThanOrEqual(viewport.width);
    expect(corner.y).toBeGreaterThanOrEqual(0);
    expect(fitted.centreX).toBeCloseTo(DEFAULT_JOB.width / 2, 9);
    expect(fitted.centreY).toBeCloseTo(DEFAULT_JOB.height / 2, 9);
  });

  it('picks a grid step that stays readable', () => {
    expect(gridSpacing(20).minor).toBe(0.5);
    expect(gridSpacing(2).minor).toBe(5);
    expect(gridSpacing(0.05).minor).toBe(200);
    expect(gridSpacing(2).major).toBe(50);
  });
});

describe('job bounds', () => {
  it('places the datum at the origin of job space', () => {
    const bounds = jobBounds(DEFAULT_JOB);
    expect(bounds).toEqual({ minX: 0, minY: 0, maxX: 300, maxY: 200 });
  });

  it('moves the rectangle when the datum is the centre', () => {
    const job: JobSetup = { ...DEFAULT_JOB, xyOrigin: 'centre' };
    expect(jobBounds(job)).toEqual({ minX: -150, minY: -100, maxX: 150, maxY: 100 });
  });

  it('adds the datum offset', () => {
    const job: JobSetup = { ...DEFAULT_JOB, xyOrigin: 'topRight', offsetX: 10, offsetY: -5 };
    expect(datumOffset(job)).toEqual({ x: 310, y: 195 });
    expect(jobBounds(job)).toEqual({ minX: -310, minY: -195, maxX: -10, maxY: 5 });
  });
});
