import { describe, expect, it } from 'vitest';

import type { Collision, SurfaceIssue } from '../../bridge/cam';
import {
  checkFits,
  issueBox,
  previewIssues,
  seekArgs,
  severityCounts,
  toleranceValue,
  TOLERANCE_MAX,
  TOLERANCE_MIN,
} from './issues';

const rapid: Collision = {
  toolpathId: 3,
  moveIndex: 7,
  x: 10,
  y: 20,
  z: -2,
  kind: 'rapid',
  severity: 'high',
};
const plunge: Collision = { ...rapid, moveIndex: 2, kind: 'plunge', severity: 'medium' };

const area = (kind: SurfaceIssue['kind'], severity: SurfaceIssue['severity']): SurfaceIssue => ({
  kind,
  severity,
  toolpathId: 4,
  moveIndex: 12,
  x: 50,
  y: 30,
  z: -3,
  depth: 0.6,
  area: 36,
});

describe('preview issues', () => {
  it('lists collisions and check areas together, the most severe first', () => {
    const issues = previewIssues([plunge, rapid], {
      issues: [area('leftover', 'low'), area('gouge', 'high')],
    });
    expect(issues.map((i) => i.kind)).toEqual(['rapid', 'gouge', 'plunge', 'leftover']);
    expect(new Set(issues.map((i) => i.key)).size).toBe(4);
    // A collision has no depth or area; an area has both.
    expect(issues[0]?.depth).toBeNull();
    expect(issues[1]?.area).toBe(36);
    expect(severityCounts(issues)).toEqual({ high: 2, medium: 1, low: 1 });
    expect(previewIssues([], null)).toEqual([]);
  });

  it('frames a few tool widths round an issue, or its whole area', () => {
    const small = issueBox({ x: 10, y: 20, z: -2, area: null }, 3, 0);
    expect(small.min[0]).toBeCloseTo(-5);
    expect(small.max[1]).toBeCloseTo(35);
    expect(small.max[2]).toBe(0);
    expect(small.min[2]).toBeLessThan(-2);
    // A big area frames all of it; nothing smaller than the least box.
    const big = issueBox({ x: 0, y: 0, z: -3, area: 400 }, 1, 0);
    expect(big.max[0] - big.min[0]).toBeCloseTo(40);
    const tiny = issueBox({ x: 0, y: 0, z: 0, area: null }, 0, 0);
    expect(tiny.max[0] - tiny.min[0]).toBeCloseTo(20);
  });

  it('frames a wrapped rotary issue round its place on the blank', () => {
    // Scene (x, y, z) is job (x, z, -y): a point on the side of the blank.
    const box = issueBox({ x: 10, y: 60, z: -2, area: null }, 3, 0, [10, -18, -12]);
    const centre = [0, 1, 2].map((k) => ((box.min[k] ?? 0) + (box.max[k] ?? 0)) / 2);
    expect(centre[0]).toBeCloseTo(10);
    expect(centre[1]).toBeCloseTo(12);
    expect(centre[2]).toBeCloseTo(-18);
    // As wide every way, not a flat slab at the unwrapped position.
    expect(box.max[2] - box.min[2]).toBeCloseTo(30);
    expect(box.max[1] - box.min[1]).toBeCloseTo(30);
    // With Z zero at the bottom the box is in job Z: scene height plus the top.
    const raised = issueBox({ x: 10, y: 60, z: -2, area: null }, 3, 40, [10, -18, -12]);
    expect(((raised.min[2] ?? 0) + (raised.max[2] ?? 0)) / 2).toBeCloseTo(22);
  });

  it('runs the animation to the move and point of an issue', () => {
    expect(seekArgs(rapid)).toEqual({
      toMove: { toolpathId: 3, moveIndex: 7, x: 10, y: 20, z: -2 },
    });
  });

  it('keeps the tolerance within what the check takes', () => {
    expect(toleranceValue(0.2)).toBe(0.2);
    expect(toleranceValue(0)).toBe(TOLERANCE_MIN);
    expect(toleranceValue(99)).toBe(TOLERANCE_MAX);
    expect(toleranceValue(Number.NaN)).toBeNull();
  });

  it('paints a check only on the heightfield it was made for', () => {
    const check = { width: 3, height: 2, map: new Uint8Array(6) };
    expect(checkFits(check, { width: 3, height: 2 })).toBe(true);
    expect(checkFits(check, { width: 2, height: 3 })).toBe(false);
    expect(checkFits(null, { width: 3, height: 2 })).toBe(false);
    expect(checkFits(check, null)).toBe(false);
  });
});
