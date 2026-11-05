import { describe, expect, it } from 'vitest';

import { HoverProbe } from './hoverProbe';

describe('HoverProbe', () => {
  it('asks once at a time and ends on the newest place', async () => {
    const asked: number[] = [];
    const probe = new HoverProbe(async (at) => {
      asked.push(at.x);
      await Promise.resolve();
      return at.x > 5 ? 'pointer' : 'crosshair';
    }, 'default');
    expect(probe.cursor).toBe('default');
    probe.move({ x: 1, y: 0 });
    probe.move({ x: 2, y: 0 });
    probe.move({ x: 9, y: 0 });
    await probe.settle();
    expect(asked).toEqual([1, 9]);
    expect(probe.cursor).toBe('pointer');
    probe.reset();
    expect(probe.cursor).toBe('default');
  });

  it('falls back when a probe fails', async () => {
    const probe = new HoverProbe(() => Promise.reject(new Error('no')), 'crosshair');
    probe.move({ x: 0, y: 0 });
    await probe.settle();
    expect(probe.cursor).toBe('crosshair');
  });
});
