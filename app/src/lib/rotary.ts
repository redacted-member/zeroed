// Rotary jobs: the blank settings with defaults for older kernels, the flat
// layout of the unwrapped surface and the wrap from flat job space onto the
// cylinder. The kernel owns these rules (doc/job.rs and post/rotary.rs);
// these helpers mirror them for the forms and the 3D view.

import type { JobSetup, RotaryAxis, RotaryBlank, RotaryZZero } from '../bridge/types';

/** Rotary settings with every field present. */
export interface RotaryBlankSetup {
  diameter: number;
  axis: RotaryAxis;
  length: number;
  blank: RotaryBlank;
  zZero: RotaryZZero;
}

export const DEFAULT_ROTARY_LENGTH = 300;

export function isRotary(job: JobSetup): boolean {
  return job.kind === 'rotary';
}

/** The job's rotary settings, with defaults for fields an older kernel omits. */
export function rotaryOf(job: JobSetup): RotaryBlankSetup {
  return {
    diameter: job.rotary.diameter,
    axis: job.rotary.axis,
    length: job.rotary.length ?? DEFAULT_ROTARY_LENGTH,
    blank: job.rotary.blank ?? 'cylinder',
    zZero: job.rotary.zZero ?? 'centre',
  };
}

/** Length around the blank: pi times the diameter. */
export function circumference(diameter: number): number {
  return Math.PI * diameter;
}

/** Width and height of the unwrapped surface for a blank. */
export function unwrappedSize(rotary: RotaryBlankSetup): { width: number; height: number } {
  const around = circumference(rotary.diameter);
  return rotary.axis === 'x'
    ? { width: rotary.length, height: around }
    : { width: around, height: rotary.length };
}

/**
 * The job with its flat size, thickness and Z zero derived from the blank, as
 * the kernel stores a rotary job. Flat jobs come back unchanged.
 */
export function normaliseRotary(job: JobSetup): JobSetup {
  if (!isRotary(job)) {
    return job;
  }
  const rotary = rotaryOf(job);
  const { width, height } = unwrappedSize(rotary);
  return {
    ...job,
    width,
    height,
    thickness: rotary.diameter / 2,
    zZero: rotary.zZero === 'centre' ? 'bottom' : 'top',
    rotary,
  };
}

/** Angle in degrees for a distance around a blank; one circumference is 360. */
export function angleDegrees(distance: number, diameter: number): number {
  if (!(diameter > 0)) {
    return 0;
  }
  return (distance / circumference(diameter)) * 360;
}

/** Scene position of a wrapped point, before any scene offset. */
export interface WrappedPoint {
  x: number;
  y: number;
  z: number;
}

/**
 * Wraps a flat job point onto the cylinder in scene axes (x -> x, job y ->
 * -z, job z -> y). `radial` is the distance from the axis. The axis runs
 * along scene x (blank along X) or scene z (blank along Y) at scene height
 * `-radius`, so the top of the uncut blank sits at scene y zero, like the
 * top of a flat block. Angle zero is straight up, and the distance round
 * runs the way the flat layout does (job y -> -z, job x -> x), so a design
 * reads the same wrapped as flat.
 */
export function wrapPoint(
  axis: RotaryAxis,
  diameter: number,
  along: number,
  around: number,
  radial: number,
): WrappedPoint {
  const radius = diameter / 2;
  const theta = radius > 0 ? around / radius : 0;
  const up = radial * Math.cos(theta) - radius;
  const side = radial * Math.sin(theta);
  return axis === 'x' ? { x: along, y: up, z: -side } : { x: side, y: up, z: -along };
}
