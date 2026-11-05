import { describe, expect, it } from 'vitest';

import { parseQuickEntry, type QuickEntry } from './quickKeys';
import {
  boxCorner,
  quickBox,
  quickCircleRadius,
  quickRadial,
  quickRotation,
  quickScale,
} from './quickShapes';

const mm = (v: number) => v;
function entry(text: string): QuickEntry {
  return parseQuickEntry(text) ?? { values: [], tags: {} };
}

describe('quick shapes', () => {
  const drag = { width: 30, height: 12 };

  it('sizes rectangles and ellipses', () => {
    expect(quickBox(entry('20'), drag, mm, true)).toEqual({ width: 20, height: 20 });
    expect(quickBox(entry('40,10'), drag, mm, true)).toEqual({ width: 40, height: 10 });
    expect(quickBox(entry('50x'), drag, mm, true)).toEqual({ width: 50, height: 12 });
    expect(quickBox(entry('8y'), drag, mm, true)).toEqual({ width: 30, height: 8 });
    expect(quickBox(entry('3r50x'), drag, mm, true)).toEqual({ width: 50, height: 12, radius: 3 });
    expect(quickBox(entry('3r50x'), drag, mm, false)).toBeNull();
    expect(quickBox(entry('9r10y'), drag, mm, true)).toBeNull();
    expect(quickBox(entry('40w10h'), drag, mm, true)).toEqual({ width: 40, height: 10 });
    expect(quickBox(entry('25h'), drag, mm, true)).toEqual({ width: 30, height: 25 });
    expect(quickBox(entry('2r40w'), drag, mm, true)).toEqual({ width: 40, height: 12, radius: 2 });
  });

  it('draws boxes towards the cursor', () => {
    expect(boxCorner({ x: 0, y: 0 }, { x: -1, y: 1 }, 10, 5)).toEqual({ x: -10, y: 5 });
  });

  it('reads circle radius and diameter', () => {
    expect(quickCircleRadius(entry('12'), mm)).toBe(12);
    expect(quickCircleRadius(entry('30d'), mm)).toBe(15);
    expect(quickCircleRadius(entry('12r'), mm)).toBe(12);
    expect(quickCircleRadius(entry('1,2'), mm)).toBeNull();
  });

  it('reads polygon sides and star points', () => {
    expect(quickRadial(entry('20'), mm, 's')).toEqual({ radius: 20 });
    expect(quickRadial(entry('8s40d'), mm, 's')).toEqual({ radius: 20, count: 8 });
    expect(quickRadial(entry('6p50i10r'), mm, 'p')).toEqual({
      radius: 10,
      count: 6,
      innerPercent: 50,
    });
    expect(quickRadial(entry('6p'), mm, 'p')).toBeNull();
    expect(quickRadial(entry('2s10r'), mm, 's')).toBeNull();
    expect(quickRadial(entry('5i10r'), mm, 's')).toBeNull();
  });

  it('rotates and scales', () => {
    expect(quickRotation(entry('45'))).toBe(45);
    expect(quickRotation(entry('45d'))).toBeNull();
    const size = { width: 20, height: 10 };
    expect(quickScale(entry('40'), 'both', size, mm)).toEqual({ sx: 2, sy: 2 });
    expect(quickScale(entry('40'), 'x', size, mm)).toEqual({ sx: 2, sy: 1 });
    expect(quickScale(entry('5'), 'y', size, mm)).toEqual({ sx: 1, sy: 0.5 });
    expect(quickScale(entry('10,20'), 'both', size, mm)).toEqual({ sx: 0.5, sy: 2 });
    expect(quickScale(entry('1.5s'), 'both', size, mm)).toEqual({ sx: 1.5, sy: 1.5 });
  });
});
