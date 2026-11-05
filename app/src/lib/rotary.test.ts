import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB, type JobSetup } from '../bridge/types';
import { angleDegrees, normaliseRotary, rotaryOf, unwrappedSize, wrapPoint } from './rotary';

const rotaryJob: JobSetup = {
  ...DEFAULT_JOB,
  kind: 'rotary',
  rotary: { diameter: 40, axis: 'x', length: 200, blank: 'cylinder', zZero: 'surface' },
};

describe('rotary jobs', () => {
  it('fills in settings an older kernel leaves out', () => {
    expect(rotaryOf(DEFAULT_JOB)).toEqual({
      diameter: 100,
      axis: 'x',
      length: 300,
      blank: 'cylinder',
      zZero: 'centre',
    });
  });

  it('lays the job out on the unwrapped surface like the kernel', () => {
    const job = normaliseRotary(rotaryJob);
    expect(job.width).toBe(200);
    expect(job.height).toBeCloseTo(Math.PI * 40, 9);
    expect(job.thickness).toBe(20);
    expect(job.zZero).toBe('top');
    expect(normaliseRotary(DEFAULT_JOB)).toBe(DEFAULT_JOB);
    const along = unwrappedSize({ ...rotaryOf(rotaryJob), axis: 'y' });
    expect(along.height).toBe(200);
  });

  it('turns a distance around the blank into degrees', () => {
    expect(angleDegrees(Math.PI * 40, 40)).toBeCloseTo(360, 9);
    expect(angleDegrees((Math.PI * 40) / 4, 40)).toBeCloseTo(90, 9);
    expect(angleDegrees(10, 0)).toBe(0);
  });

  it('wraps a design the way it reads flat, not mirrored', () => {
    // Flat, job y runs to scene -z and job x to scene x. Just past the top of
    // the blank the wrapped design must run the same way.
    const along = wrapPoint('x', 40, 0, 1, 20);
    expect(along.z).toBeLessThan(0);
    const across = wrapPoint('y', 40, 0, 1, 20);
    expect(across.x).toBeGreaterThan(0);
  });

  it('wraps flat points onto the cylinder with the top at scene zero', () => {
    const top = wrapPoint('x', 40, 15, 0, 20);
    expect(top.x).toBe(15);
    expect(top.y).toBeCloseTo(0, 9);
    expect(top.z).toBeCloseTo(0, 9);
    // A quarter turn round sits level with the axis, one radius to the side.
    const side = wrapPoint('x', 40, 15, (Math.PI * 40) / 4, 20);
    expect(side.y).toBeCloseTo(-20, 9);
    expect(side.z).toBeCloseTo(-20, 9);
    // Cutting 5 mm deep moves the point 5 mm towards the axis.
    const cut = wrapPoint('y', 40, 30, 0, 15);
    expect(cut).toEqual({ x: 0, y: -5, z: -30 });
  });
});
