import { describe, expect, it } from 'vitest';

import { alignmentCandidates, findAlignment, type AlignmentSource } from './smartSnap';

const square = (id: number, x: number, y: number, size: number): AlignmentSource => ({
  id,
  layer: 1,
  bbox: [x, y, x + size, y + size],
  runs: [new Float32Array([x, y, x + size, y, x + size, y + size, x, y + size])],
});

describe('smart snapping', () => {
  it('collects corners, centre and nodes of nearby objects only', () => {
    const points = alignmentCandidates(
      [square(1, 0, 0, 10), square(2, 500, 500, 10)],
      { x: 20, y: 5 },
      50,
    );
    expect(points).toContainEqual({ x: 5, y: 5 });
    expect(points).toContainEqual({ x: 10, y: 10 });
    expect(points.some((p) => p.x >= 500)).toBe(false);
  });

  it('skips excluded objects and uses only the start of long flattened runs', () => {
    const curve: AlignmentSource = {
      id: 3,
      layer: 1,
      bbox: [0, 0, 100, 100],
      runs: [new Float32Array(Array.from({ length: 200 }, (_, i) => i / 2))],
    };
    expect(alignmentCandidates([curve], { x: 0, y: 0 }, 10)).toHaveLength(6);
    expect(alignmentCandidates([curve], { x: 0, y: 0 }, 10, (s) => s.id === 3)).toHaveLength(0);
  });

  it('snaps one axis to the closest aligned point and reports a line', () => {
    const snap = findAlignment(
      { x: 40.3, y: 80 },
      [
        { x: 40, y: 0 },
        { x: 41, y: 10 },
      ],
      0.5,
    );
    expect(snap?.point).toEqual({ x: 40, y: 80 });
    expect(snap?.lines).toEqual([{ axis: 'x', from: { x: 40, y: 0 }, to: { x: 40, y: 80 } }]);
  });

  it('snaps both axes to different objects at once', () => {
    const snap = findAlignment(
      { x: 9.8, y: 30.2 },
      [
        { x: 10, y: 0 },
        { x: 60, y: 30 },
      ],
      0.5,
    );
    expect(snap?.point).toEqual({ x: 10, y: 30 });
    expect(snap?.lines.map((l) => l.axis)).toEqual(['x', 'y']);
  });

  it('prefers the nearest source when offsets tie', () => {
    const snap = findAlignment(
      { x: 5, y: 50 },
      [
        { x: 5, y: 0 },
        { x: 5, y: 45 },
      ],
      1,
    );
    expect(snap?.lines[0]?.from).toEqual({ x: 5, y: 45 });
  });

  it('answers null when nothing aligns or the tolerance is zero', () => {
    expect(findAlignment({ x: 0, y: 0 }, [{ x: 5, y: 5 }], 1)).toBeNull();
    expect(findAlignment({ x: 5, y: 0 }, [{ x: 5, y: 5 }], 0)).toBeNull();
  });
});
