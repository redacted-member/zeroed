import { describe, expect, it } from 'vitest';

import { isDrag, reachMm, tabMarkers } from './model';

describe('Edit Tabs panel helpers', () => {
  it('keeps the click reach the same on screen', () => {
    expect(reachMm(2)).toBe(4);
    expect(reachMm(8)).toBe(1);
  });

  it('tells a drag from a click', () => {
    expect(isDrag({ x: 0, y: 0 }, { x: 1, y: 0 }, 2)).toBe(false);
    expect(isDrag({ x: 0, y: 0 }, { x: 3, y: 0 }, 2)).toBe(true);
  });

  it('draws a closed square round each tab', () => {
    const [marker] = tabMarkers([{ x: 10, y: 20 }], 1);
    expect(marker?.subpaths[0]?.closed).toBe(true);
    expect(marker?.subpaths[0]?.start).toEqual({ x: 5.5, y: 15.5 });
  });
});
