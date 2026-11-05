import { describe, expect, it } from 'vitest';

import type { EntitySummary } from '../../bridge/document';
import {
  hasZeroLength,
  ISSUE_ZOOM_MM,
  issueBounds,
  markKind,
  markPoints,
  markShape,
} from './model';

const entity: EntitySummary = {
  id: 4,
  kind: 'path',
  layer: 1,
  bbox: [0, 0, 50, 20],
  closed: false,
  revision: 1,
};

describe('vector validator model', () => {
  it('zooms around the place a problem was found', () => {
    const box = issueBounds({ id: 4, kind: 'open', x: 5, y: 6, message: '' }, [entity]);
    expect(box).toEqual([
      5 - ISSUE_ZOOM_MM,
      6 - ISSUE_ZOOM_MM,
      5 + ISSUE_ZOOM_MM,
      6 + ISSUE_ZOOM_MM,
    ]);
  });

  it('falls back to the object box, or nothing when the object is gone', () => {
    expect(issueBounds({ id: 4, kind: 'overlappingDuplicate', message: '' }, [entity])).toEqual(
      entity.bbox,
    );
    expect(issueBounds({ id: 9, kind: 'open', message: '' }, [entity])).toBeNull();
  });

  it('marks each issue with its own shape at every place', () => {
    const open = {
      id: 1,
      kind: 'open' as const,
      message: '',
      marks: [
        { x: 0, y: 0 },
        { x: 5, y: 0 },
      ],
    };
    expect(markKind(open)).toBe('open');
    expect(markPoints(open)).toHaveLength(2);
    expect(markKind({ ...open, kind: 'intersection' })).toBe('intersection');
    expect(markKind({ ...open, kind: 'selfIntersecting' })).toBe('intersection');
    expect(markKind({ ...open, kind: 'overlappingDuplicate' })).toBe('duplicate');
    expect(markKind({ ...open, kind: 'zeroLengthSegment' })).toBe('zeroLength');
    expect(markPoints({ id: 1, kind: 'open', message: '', x: 3, y: 4 })).toEqual([{ x: 3, y: 4 }]);
    expect(markShape('intersection', { x: 0, y: 0 }, 1).subpaths).toHaveLength(2);
    expect(markShape('open', { x: 0, y: 0 }, 1).subpaths[0]?.closed).toBe(true);
    expect(hasZeroLength([open])).toBe(false);
    expect(hasZeroLength([{ ...open, kind: 'zeroLengthSegment' }])).toBe(true);
  });
});
