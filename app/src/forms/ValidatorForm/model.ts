// Pure helpers for the Vector Validator form: the problems it lists and the
// box the view zooms to when one is shown.

import type { Bbox, EntitySummary, Path, Point, ValidationIssue } from '../../bridge/document';
import type { OverlayTone } from '../../state/drawStore';

/** Half the size of the box shown around a problem that has a location. */
export const ISSUE_ZOOM_MM = 10;

/** The box to zoom to for an issue: its location, else its object's box. */
export function issueBounds(
  issue: ValidationIssue,
  entities: readonly EntitySummary[],
): Bbox | null {
  if (issue.x !== undefined && issue.y !== undefined) {
    return [
      issue.x - ISSUE_ZOOM_MM,
      issue.y - ISSUE_ZOOM_MM,
      issue.x + ISSUE_ZOOM_MM,
      issue.y + ISSUE_ZOOM_MM,
    ];
  }
  const entity = entities.find((e) => e.id === issue.id);
  return entity === undefined ? null : entity.bbox;
}

/** A stable key for a row of the issue list. */
export function issueKey(issue: ValidationIssue, index: number): string {
  return `${issue.id}-${issue.kind}-${index}`;
}

/** The four kinds of mark the legend explains. */
export type MarkKind = 'open' | 'intersection' | 'duplicate' | 'zeroLength';

export const MARK_KINDS: readonly MarkKind[] = ['open', 'intersection', 'duplicate', 'zeroLength'];

/** Which mark an issue gets. */
export function markKind(issue: ValidationIssue): MarkKind {
  switch (issue.kind) {
    case 'open':
      return 'open';
    case 'selfIntersecting':
    case 'intersection':
      return 'intersection';
    case 'zeroLengthSegment':
      return 'zeroLength';
    default:
      return 'duplicate';
  }
}

/** The colour of each mark. */
export const MARK_TONE: Record<MarkKind, OverlayTone> = {
  open: 'first',
  intersection: 'accent',
  duplicate: 'second',
  zeroLength: 'first',
};

/** The places marked for an issue: its marks, else its one place. */
export function markPoints(issue: ValidationIssue): Point[] {
  if (issue.marks !== undefined && issue.marks.length > 0) {
    return issue.marks;
  }
  return issue.x !== undefined && issue.y !== undefined ? [{ x: issue.x, y: issue.y }] : [];
}

function polygon(points: Point[]): Path {
  const [first, ...rest] = points;
  return {
    subpaths: [
      {
        start: first ?? { x: 0, y: 0 },
        closed: true,
        segments: rest.map((to) => ({ kind: 'line' as const, to })),
      },
    ],
  };
}

/** A mark `r` millimetres across its middle: a square for an open end, a
 * cross for a crossing, a diamond for a duplicate, a triangle for a zero
 * length span. */
export function markShape(kind: MarkKind, at: Point, r: number): Path {
  const { x, y } = at;
  switch (kind) {
    case 'open':
      return polygon([
        { x: x - r, y: y - r },
        { x: x + r, y: y - r },
        { x: x + r, y: y + r },
        { x: x - r, y: y + r },
      ]);
    case 'intersection':
      return {
        subpaths: [
          {
            start: { x: x - r, y: y - r },
            closed: false,
            segments: [{ kind: 'line', to: { x: x + r, y: y + r } }],
          },
          {
            start: { x: x - r, y: y + r },
            closed: false,
            segments: [{ kind: 'line', to: { x: x + r, y: y - r } }],
          },
        ],
      };
    case 'duplicate':
      return polygon([
        { x, y: y - r },
        { x: x + r, y },
        { x, y: y + r },
        { x: x - r, y },
      ]);
    default:
      return polygon([
        { x: x - r, y: y - r },
        { x: x + r, y: y - r },
        { x, y: y + r },
      ]);
  }
}

/** True when some issue is a zero length span, which the fix button takes out. */
export function hasZeroLength(issues: readonly ValidationIssue[]): boolean {
  return issues.some((issue) => issue.kind === 'zeroLengthSegment');
}
