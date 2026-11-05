import { describe, expect, it } from 'vitest';

import type { ToolpathSummary } from '../../bridge/cam';
import { machineTiming, scaleValue, timeRows } from './model';

const row = (id: number, visible: boolean, seconds: number) =>
  ({ id, name: `T${id}`, visible, estimatedSeconds: seconds }) as unknown as ToolpathSummary;

describe('machining time estimate dialog model', () => {
  it('lists the visible toolpaths and totals them', () => {
    const { rows, total } = timeRows([row(1, true, 60), row(2, false, 30), row(3, true, 15)]);
    expect(rows.map((r) => r.id)).toEqual([1, 3]);
    expect(total).toBe(75);
  });

  it('accepts a scale factor from 0.1 to 10', () => {
    expect(scaleValue('1.25').value).toBe(1.25);
    expect(scaleValue('0').error).toBeDefined();
    expect(scaleValue('11').error).toBeDefined();
    expect(scaleValue('x').value).toBeNull();
  });

  it('reads the active machine rapid rate and accelerations', () => {
    const machines = {
      active: 'Shop',
      list: [
        { name: 'Old', units: 'mm' as const, bedX: 1, bedY: 1, bedZ: 1, posts: [] },
        {
          name: 'Shop',
          units: 'mm' as const,
          bedX: 1,
          bedY: 1,
          bedZ: 1,
          posts: [],
          rapidRate: 5000,
          acceleration: [500, 500, 0] as [number, number, number],
        },
      ],
    };
    expect(machineTiming(machines)).toEqual({ name: 'Shop', rapidRate: 5000, accelerated: false });
    expect(machineTiming({ ...machines, active: 'Old' })).toEqual({
      name: 'Old',
      rapidRate: 3000,
      accelerated: false,
    });
    expect(machineTiming({ ...machines, active: 'None' })).toBeNull();
  });
});
