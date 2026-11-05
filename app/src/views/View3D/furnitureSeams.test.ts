import { describe, expect, it } from 'vitest';

import type { ConnectionRow, Vec3 } from '../../bridge/furniture';
import {
  dimensionGap,
  dimensionLines,
  draggedSize,
  midpoint,
  nearestSeam,
  overallBox,
  placedBox,
  placedSeam,
  raySegmentDistance,
  sizeArrows,
  coveredSheets,
  previewShown,
  showsPreview,
} from './furnitureSeams';

function seam(key: string, from: Vec3, to: Vec3): ConnectionRow {
  return {
    key,
    a: 'a',
    b: 'b',
    type: 'tee',
    class: 'shelf',
    joint: 'dado',
    params: {},
    source: 'style',
    choices: ['dado'],
    seam: { from, to },
  };
}

describe('raySegmentDistance', () => {
  it('measures the gap to a segment crossing the ray at right angles', () => {
    // Ray down from above along -Z at x = 0, y = 5; segment along X at y = 0, z = 0.
    const hit = raySegmentDistance([0, 5, 100], [0, 0, -1], [-50, 0, 0], [50, 0, 0]);
    expect(hit.distance).toBeCloseTo(5, 9);
    expect(hit.at).toBeCloseTo(100, 9);
  });

  it('measures to the nearest end when the ray passes beyond the segment', () => {
    // Ray at x = 60 passes 10 past the end at x = 50 (and 3 off in Y).
    const hit = raySegmentDistance([60, 3, 100], [0, 0, -1], [-50, 0, 0], [50, 0, 0]);
    expect(hit.distance).toBeCloseTo(Math.hypot(10, 3), 9);
  });

  it('never looks behind the ray start', () => {
    // The segment is behind the eye: the nearest ray point is its start.
    const hit = raySegmentDistance([0, 0, 10], [0, 0, 1], [-5, 0, 0], [5, 0, 0]);
    expect(hit.at).toBe(0);
    expect(hit.distance).toBeCloseTo(10, 9);
  });

  it('handles a ray parallel to the segment', () => {
    const hit = raySegmentDistance([0, 4, -100], [1, 0, 0], [0, 0, -100], [10, 0, -100]);
    expect(hit.distance).toBeCloseTo(4, 9);
  });
});

describe('nearestSeam', () => {
  const down: Vec3 = [0, 0, -1];
  const a = seam('left~shelf#0', [0, 0, 500], [0, 300, 500]);
  const b = seam('right~shelf#0', [800, 0, 500], [800, 300, 500]);

  it('picks the seam within reach and nothing past it', () => {
    expect(nearestSeam([7.99, 100, 1000], down, [a, b])?.key).toBe('left~shelf#0');
    expect(nearestSeam([8.01, 100, 1000], down, [a, b])).toBeNull();
    expect(nearestSeam([795, 100, 1000], down, [a, b])?.key).toBe('right~shelf#0');
  });

  it('prefers the clearly nearer seam', () => {
    const near = seam('n', [3, 0, 500], [3, 300, 500]);
    expect(nearestSeam([0, 100, 1000], down, [near, a])?.key).toBe('left~shelf#0');
  });

  it('takes the one nearer the eye between seams equally close', () => {
    const low = seam('low', [0.5, 0, 100], [0.5, 300, 100]);
    expect(nearestSeam([0, 100, 1000], down, [low, a])?.key).toBe('left~shelf#0');
    expect(nearestSeam([0, 100, 1000], down, [a, low])?.key).toBe('left~shelf#0');
  });
});

describe('overallBox and dimension lines', () => {
  const box = overallBox([
    { min: [0, 0, 0], max: [18, 300, 1800] },
    { min: [782, 0, 0], max: [800, 300, 1800] },
    { min: [18, 6, 80], max: [782, 300, 98] },
  ]);

  it('takes the box round every panel', () => {
    expect(box).toEqual({ min: [0, 0, 0], max: [800, 300, 1800] });
    expect(overallBox([])).toBeNull();
  });

  it('measures width, depth and height outside the box', () => {
    if (box === null) {
      throw new Error('no box');
    }
    const lines = dimensionLines(box, 50);
    expect(lines.map((l) => [l.axis, l.length])).toEqual([
      ['width', 800],
      ['depth', 300],
      ['height', 1800],
    ]);
    expect(lines[0]?.from).toEqual([0, -50, 0]);
    expect(lines[0]?.to).toEqual([800, -50, 0]);
    expect(lines[1]?.from).toEqual([850, 0, 0]);
    expect(lines[2]?.to).toEqual([850, -50, 1800]);
  });

  it('keeps the gap a tenth of the largest side between 20 and 120 mm', () => {
    expect(dimensionGap({ min: [0, 0, 0], max: [800, 300, 1000] })).toBe(100);
    expect(dimensionGap({ min: [0, 0, 0], max: [100, 100, 100] })).toBe(20);
    expect(dimensionGap({ min: [0, 0, 0], max: [800, 300, 1800] })).toBe(120);
  });

  it('finds a midpoint', () => {
    expect(midpoint([0, 0, 0], [10, 20, 30])).toEqual([5, 10, 15]);
  });
});

describe('size arrows', () => {
  const box = { min: [0, 0, 0] as Vec3, max: [800, 300, 1800] as Vec3 };

  it('stand out of the right, back and top faces', () => {
    expect(sizeArrows(box, 100)).toEqual([
      { axis: 'width', base: [800, 150, 900], tip: [900, 150, 900] },
      { axis: 'depth', base: [400, 300, 900], tip: [400, 400, 900] },
      { axis: 'height', base: [400, 150, 1800], tip: [400, 150, 1900] },
    ]);
  });

  it('turns a drag into a size, rounded to the step', () => {
    // 30 px at 2 mm a pixel adds 60 mm.
    expect(draggedSize(800, 30, 2, 1)).toBe(860);
    // 1/16 in steps of 1.5875 mm: 801 mm is 504.57 steps, so 505.
    expect(draggedSize(800, 1, 1, 1.5875)).toBeCloseTo(505 * 1.5875, 9);
    expect(draggedSize(800, -10.4, 1, 1)).toBe(790);
  });

  it('keeps the size within the bounds, the bounds themselves allowed', () => {
    expect(draggedSize(800, -600, 1, 1, 250, 2400)).toBe(250);
    expect(draggedSize(800, -550, 1, 1, 250, 2400)).toBe(250);
    expect(draggedSize(800, -549, 1, 1, 250, 2400)).toBe(251);
    expect(draggedSize(800, 5000, 1, 1, 250, 2400)).toBe(2400);
  });
});

describe('placing a design at its origin', () => {
  it('moves the box and seams by the origin', () => {
    const o: Vec3 = [1000, -50, 0];
    expect(placedBox([{ min: [0, 0, 0], max: [800, 300, 1800] }], o)).toEqual({
      min: [1000, -50, 0],
      max: [1800, 250, 1800],
    });
    expect(placedSeam({ from: [18, 0, 98], to: [18, 300, 98] }, o)).toEqual({
      from: [1018, -50, 98],
      to: [1018, 250, 98],
    });
    expect(placedBox([])).toBeNull();
  });
});

describe('showsPreview', () => {
  const made = { design: 3, values: { width: 800 }, made: { width: 800 } };

  it('shows the preview before Create and while a change is on its way', () => {
    expect(showsPreview(null, false)).toBe(false);
    expect(showsPreview({ design: null, values: { width: 800 }, made: null }, false)).toBe(true);
    expect(showsPreview({ ...made, values: { width: 801 } }, false)).toBe(true);
    expect(showsPreview({ ...made, values: { width: 800, back: 'none' } }, false)).toBe(true);
    expect(showsPreview(made, true)).toBe(true);
  });

  it('shows the made pieces once a made design has settled', () => {
    expect(showsPreview(made, false)).toBe(false);
    expect(showsPreview(made, false, false)).toBe(false);
  });

  it('keeps the preview while models on its faces have no carving toolpaths', () => {
    expect(showsPreview(made, false, true)).toBe(true);
  });
});

describe('coveredSheets', () => {
  const made = {
    left: { vector: 1, sheet: 4, piece: 1 },
    right: { vector: 2, sheet: 4, piece: 2 },
    back: { vector: 3, sheet: 2, piece: 1 },
  };
  const settled = {
    session: { design: 3, values: { width: 800 }, made: { width: 800 } },
    busy: false,
    designs: [{ id: 3, made, models: 0, carving: 0 }],
  };

  it('names the made sheets while the preview stands in for them', () => {
    expect(coveredSheets({ ...settled, busy: true })).toBe('2,4');
    expect(previewShown({ ...settled, busy: true })).toBe(true);
  });

  it('names none once the made pieces show, or before Create', () => {
    expect(coveredSheets(settled)).toBe('');
    expect(previewShown(settled)).toBe(false);
    expect(coveredSheets({ ...settled, session: { ...settled.session, design: null } })).toBe('');
  });

  it('keeps the preview for models on faces until they are carved', () => {
    const models = { ...settled, designs: [{ id: 3, made, models: 2, carving: 0 }] };
    expect(coveredSheets(models)).toBe('2,4');
    const carved = { ...settled, designs: [{ id: 3, made, models: 2, carving: 2 }] };
    expect(coveredSheets(carved)).toBe('');
  });
});
