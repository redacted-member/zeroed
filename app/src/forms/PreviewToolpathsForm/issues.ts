// Pure parts of the preview checks: the collisions the animation met and
// the areas the finished preview is off the design, merged into one list
// with the worst first, the box the 3D view frames for one and the point
// the animation runs to.

import type { Collision, IssueKind, Severity, SimCheck, SurfaceIssue } from '../../bridge/cam';

/** One row of the issues list. */
export interface PreviewIssue {
  /** Stable key for the list. */
  key: string;
  kind: IssueKind;
  severity: Severity;
  toolpathId: number;
  moveIndex: number;
  /** Where, job space: the tool tip, or an area's worst point. */
  x: number;
  y: number;
  z: number;
  /** How far off the design, mm (areas only). */
  depth: number | null;
  /** Area, square mm (areas only). */
  area: number | null;
}

/** The kinds the legend shows, in order. */
export const LEGEND_KINDS = ['gouge', 'leftover', 'undercut'] as const;

/** Default check tolerance, mm. */
export const DEFAULT_TOLERANCE = 0.1;
/** Most and least tolerance the field takes, mm. */
export const TOLERANCE_MIN = 0.01;
export const TOLERANCE_MAX = 5;

const RANK: Readonly<Record<Severity, number>> = { high: 0, medium: 1, low: 2 };

/** The collisions and the check's areas as one list, the most severe first
 * (collisions before areas of the same severity, each in the order found). */
export function previewIssues(
  collisions: readonly Collision[],
  check: Pick<SimCheck, 'issues'> | null,
): PreviewIssue[] {
  const met: PreviewIssue[] = collisions.map((c, k) => ({
    key: `c${k}-${c.toolpathId}-${c.moveIndex}-${c.kind}`,
    kind: c.kind,
    severity: c.severity,
    toolpathId: c.toolpathId,
    moveIndex: c.moveIndex,
    x: c.x,
    y: c.y,
    z: c.z,
    depth: null,
    area: null,
  }));
  const areas: PreviewIssue[] = (check?.issues ?? []).map((s: SurfaceIssue, k) => ({
    key: `s${k}-${s.toolpathId}-${s.kind}`,
    kind: s.kind,
    severity: s.severity,
    toolpathId: s.toolpathId,
    moveIndex: s.moveIndex,
    x: s.x,
    y: s.y,
    z: s.z,
    depth: s.depth,
    area: s.area,
  }));
  const all = [...met, ...areas];
  return all
    .map((issue, order) => ({ issue, order }))
    .sort((a, b) => RANK[a.issue.severity] - RANK[b.issue.severity] || a.order - b.order)
    .map((entry) => entry.issue);
}

/** How many issues there are of each severity. */
export function severityCounts(issues: readonly PreviewIssue[]): Record<Severity, number> {
  const counts: Record<Severity, number> = { high: 0, medium: 0, low: 0 };
  for (const issue of issues) {
    counts[issue.severity] += 1;
  }
  return counts;
}

/** Smallest side of the box framed round an issue, mm. */
const FRAME_MIN = 20;

/**
 * The job space box the 3D view frames round an issue: a few tool widths
 * (`toolRadius`) or the area's size round the point, from the point down to
 * the material top. On a rotary job shown wrapped, `wrapped` is the issue's
 * scene point on the blank (scene x, y, z as the renderer lays them out:
 * job x, z and -y), and the box is as wide every way round it, since the
 * surface there faces out from the axis.
 */
export function issueBox(
  issue: Pick<PreviewIssue, 'x' | 'y' | 'z' | 'area'>,
  toolRadius: number,
  surfaceZ: number,
  wrapped: readonly [number, number, number] | null = null,
): { min: [number, number, number]; max: [number, number, number] } {
  const across = Math.max(FRAME_MIN, toolRadius * 10, Math.sqrt(issue.area ?? 0) * 2);
  const half = across / 2;
  if (wrapped !== null) {
    // Scene height is job Z less the material top's.
    const [x, y, z] = [wrapped[0], -wrapped[2], wrapped[1] + surfaceZ];
    return { min: [x - half, y - half, z - half], max: [x + half, y + half, z + half] };
  }
  return {
    min: [issue.x - half, issue.y - half, Math.min(issue.z, surfaceZ) - half / 2],
    max: [issue.x + half, issue.y + half, Math.max(issue.z, surfaceZ)],
  };
}

/** Where the animation stops for an issue: the point on its move nearest it. */
export function seekArgs(issue: Pick<PreviewIssue, 'toolpathId' | 'moveIndex' | 'x' | 'y' | 'z'>): {
  toMove: { toolpathId: number; moveIndex: number; x: number; y: number; z: number };
} {
  return {
    toMove: {
      toolpathId: issue.toolpathId,
      moveIndex: issue.moveIndex,
      x: issue.x,
      y: issue.y,
      z: issue.z,
    },
  };
}

/** A tolerance typed into the field, clamped to what the check takes, or
 * null when it does not read as a number. */
export function toleranceValue(mm: number): number | null {
  if (!Number.isFinite(mm)) {
    return null;
  }
  return Math.min(Math.max(mm, TOLERANCE_MIN), TOLERANCE_MAX);
}

/** Whether a check map fits the heightfield shown, cell for cell. */
export function checkFits(
  check: Pick<SimCheck, 'width' | 'height' | 'map'> | null,
  field: { width: number; height: number } | null,
): boolean {
  return (
    check !== null &&
    field !== null &&
    check.width === field.width &&
    check.height === field.height &&
    check.map.length === field.width * field.height
  );
}
