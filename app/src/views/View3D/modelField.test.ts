import { describe, expect, it } from 'vitest';

import {
  codedHeights,
  floatHeights,
  HEIGHT_NAN,
  heightReader,
  type Heights,
} from '../../bridge/heights';
import type { Composite } from '../../bridge/model';
import { DEFAULT_JOB, type JobSetup } from '../../bridge/types';
import { flatComposite, modelField, planeDepth, withUnderside } from './modelField';

/** Every cell's height in millimetres. */
function mm(heights: Heights | 'flat' | undefined): number[] {
  if (heights === undefined || heights === 'flat') {
    return [];
  }
  const read = heightReader(heights);
  return Array.from({ length: heights.data.length }, (_, i) => read(i));
}

function composite(values: number[]): Composite {
  return {
    width: values.length,
    height: 1,
    cellSize: 1,
    originX: 0.5,
    originY: 0.5,
    min: Math.min(...values),
    max: Math.max(...values),
    revision: 1,
    sections: [],
    z: floatHeights(Float32Array.from(values)),
  };
}

describe('model field', () => {
  it('puts the highest point at the material top', () => {
    const field = modelField(composite([0, 2, 5]), DEFAULT_JOB);
    expect(mm(field.z)).toEqual([-5, -3, 0]);
    expect(field.top).toBe(0);
    expect(field.bottom).toBe(-DEFAULT_JOB.thickness);
  });

  it('follows the job when Z zero is on the bed', () => {
    const job = { ...DEFAULT_JOB, zZero: 'bottom' as const };
    const field = modelField(composite([0, 4]), job);
    expect(mm(field.z)).toEqual([job.thickness - 4, job.thickness]);
  });

  it('keeps a model taller than the material whole, standing out above the top', () => {
    const field = modelField(composite([0, 40]), DEFAULT_JOB);
    expect(mm(field.z)).toEqual([-DEFAULT_JOB.thickness, 40 - DEFAULT_JOB.thickness]);
  });

  it('answers the scene height of the zero plane', () => {
    const c = composite([0, 2, 5]);
    const field = modelField(c, DEFAULT_JOB);
    const y = -planeDepth(c.max ?? 0, 0, DEFAULT_JOB);
    // Model height 5 is the peak, which sits at the material top (scene 0).
    expect(y + 5).toBeCloseTo((mm(field.z)[2] ?? 0) - field.top);
    expect(planeDepth(0, 0, DEFAULT_JOB)).toBe(0);
  });

  it('lowers the model by the Material Setup gap, never under the material bottom', () => {
    const c = composite([0, 2, 5]);
    const job = { ...DEFAULT_JOB, machine: { ...DEFAULT_JOB.machine, modelGap: 3 } };
    expect(mm(modelField(c, job).z)[2]).toBeCloseTo(-3);
    expect(planeDepth(5, 0, job)).toBeCloseTo(8);
    const deep = { ...job, machine: { ...job.machine, modelGap: 100 } };
    expect(planeDepth(5, 0, deep)).toBeCloseTo(DEFAULT_JOB.thickness);
  });

  it('keeps u16 codes as they came, moving only their base', () => {
    const codes = Uint16Array.from([0, 4, 10]);
    const c = { ...composite([0, 2, 5]), z: codedHeights(codes, { base: 0, step: 0.5 }) };
    const field = modelField(c, DEFAULT_JOB);
    expect(field.z.data).toBe(codes);
    expect(field.z.code).toEqual({ base: -5, step: 0.5 });
    expect(mm(field.z)).toEqual([-5, -3, 0]);
  });

  it('puts cells without model on the zero plane, still as codes', () => {
    const codes = Uint16Array.from([0, HEIGHT_NAN, 10]);
    const c = { ...composite([0, 0, 5]), z: codedHeights(codes, { base: 0, step: 0.5 }) };
    const field = modelField(c, DEFAULT_JOB);
    expect(field.z.code).not.toBeNull();
    expect(mm(field.z)).toEqual([-5, -5, 0]);
    // The composite itself is left alone.
    expect(codes[1]).toBe(HEIGHT_NAN);
    // Without a code for zero the heights become floats.
    const high = { ...c, z: codedHeights(Uint16Array.from([0, HEIGHT_NAN]), { base: 1, step: 1 }) };
    const lifted = modelField(high, DEFAULT_JOB, 2);
    expect(lifted.z.code).toBeNull();
    expect(mm(lifted.z)).toEqual([-1, -2]);
  });
});

describe('multi sided view', () => {
  const job: JobSetup = { ...DEFAULT_JOB, thickness: 20, zZero: 'top' };
  const composite = (z: number[], max: number): Composite =>
    ({
      width: z.length,
      height: 1,
      cellSize: 1,
      originX: 0,
      originY: 0,
      revision: 1,
      min: 0,
      max,
      z: floatHeights(new Float32Array(z)),
    }) as Composite;

  it('puts both sides on one modelling plane, set by the gap above the whole model', () => {
    const two: JobSetup = {
      ...job,
      kind: 'doubleSided',
      machine: { ...job.machine, modelGap: 2 },
    };
    // Top side model 5 high: its peak 2 under the top face, the plane 7 down.
    expect(planeDepth(5, 4, two, 'top')).toBe(7);
    // Seen from the bottom face the same plane is 20 - 7 = 13 down.
    expect(planeDepth(4, 5, two, 'bottom')).toBe(13);
    // A single sided job ignores the side and the other peak.
    expect(planeDepth(5, 4, job, 'bottom')).toBe(5);
  });

  it('hangs the other side under the shared plane, so the sides never meet', () => {
    const top = modelField(composite([5, 5, 0], 5), job, 7);
    const under = withUnderside(top, composite([0, 4, Number.NaN], 4), 7);
    // The plane is at -7; the other side's model reaches 4 below it.
    expect(mm(under.floor)).toEqual([-7, -11, -7]);
    expect(mm(under.z)).toEqual([-2, -2, -7]);
  });

  it('cuts through only where a side dips below the plane', () => {
    // This side dips 2 below the plane at the first cell.
    const top = modelField(composite([-2, 3], 3), job, 3);
    const under = withUnderside(top, composite([3, 0], 3), 3);
    expect(Number.isNaN(mm(under.z)[0])).toBe(false);
    const deep = withUnderside(top, composite([1, 0], 1), 3);
    expect(Number.isNaN(mm(deep.z)[0])).toBe(true);
    expect(Number.isNaN(mm(deep.floor)[0])).toBe(true);
  });

  it('shows a side with no model of its own as flat', () => {
    const flat = flatComposite(composite([3, 1], 3));
    expect(mm(flat.z)).toEqual([0, 0]);
    expect(flat.max).toBe(0);
  });

  it('leaves a field on another grid alone', () => {
    const top = modelField(composite([1, 1], 1), job);
    expect(withUnderside(top, composite([1], 1), 1)).toBe(top);
  });
});
