import { describe, expect, it } from 'vitest';

import { selectionHandles } from './renderer';

describe('selectionHandles', () => {
  const camera = { centreX: 0, centreY: 0, pxPerMm: 1 };
  const viewport = { width: 200, height: 200 };
  const frame = { angle: 0, extent: [-20, -10, 20, 10] as [number, number, number, number] };

  it('puts eight scale handles on the box and a rotate handle past each corner', () => {
    const handles = selectionHandles(frame, camera, viewport);
    expect(handles.filter((h) => h.id !== 'rotate')).toHaveLength(8);
    const rotate = handles.filter((h) => h.id === 'rotate');
    expect(rotate).toHaveLength(4);
    const nw = handles.find((h) => h.id === 'nw');
    const first = rotate[0];
    // Further from the middle of the box than the corner it belongs to.
    const centre = { x: 100, y: 100 };
    const far = (p: { x: number; y: number }) => Math.hypot(p.x - centre.x, p.y - centre.y);
    expect(first !== undefined && nw !== undefined && far(first) > far(nw) + 10).toBe(true);
  });

  it('puts the rotational anchor first when it is shown', () => {
    const handles = selectionHandles(frame, camera, viewport, { x: 5, y: 5 });
    expect(handles).toHaveLength(13);
    expect(handles[0]).toEqual({ id: 'pivot', x: 105, y: 95 });
  });
});
