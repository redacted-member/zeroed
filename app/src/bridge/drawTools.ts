// Shapes of the Drawing tab edit tool commands: drag snipping, Select
// Duplicates, freehand sketching and the Measure tool's vector pick.
// Lengths are millimetres.

import type { Path, Point } from './document';

export interface SnipReply {
  /** Vectors that lost a span. */
  ids: number[];
  /** Vectors cut away completely. */
  deleted: number[];
  revision: number;
}

export interface DuplicatesReply {
  /** The copies now selected; the first drawn of each stack is kept out. */
  ids: number[];
  revision: number;
}

export interface FreehandArgs {
  points: Point[];
  /** Averaging passes before the curve fit. */
  smoothing: number;
  /** How far the fitted curve may stray from the smoothed trail. */
  tolerance: number;
  closed: boolean;
}

export interface MeasuredSegment {
  kind: 'line' | 'arc' | 'cubic';
  length: number;
  start: Point;
  end: Point;
  radius?: number;
  centre?: Point;
  sweepDeg?: number;
}

export interface MeasureHit {
  id: number;
  /** Nearest point on the vector. */
  point: Point;
  distance: number;
  /** Length of the whole vector. */
  length: number;
  closed: boolean;
  width: number;
  height: number;
  /** Area enclosed, square millimetres (0 when open). */
  area: number;
  /** Number of spans (segments) of the vector. */
  spans: number;
  /** The vector's geometry, for highlighting it. */
  path: Path;
  segment: MeasuredSegment;
}
