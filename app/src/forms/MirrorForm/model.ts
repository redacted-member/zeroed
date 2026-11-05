// Pure state for Mirror Selection: which flip, and whether to keep a copy.
// Flips work on the selection box, or on a single rotated object's own
// rotated box with Use rotated bounds; the material flips turn the
// selection about the centre lines of the material, and a line vector
// picked last (Shift click) can be the mirror.

import type { Bbox, Matrix, Path, Point, SelectionFrame } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import {
  bboxCentre,
  inFrame,
  mirrorAcrossLine,
  mirrorMatrix,
  type MirrorMode,
} from '../../lib/transformMath';
import { scaleAbout } from '../../lib/geometry';

export interface MirrorFormState {
  copy: boolean;
  /** Flip a single rotated object about its own rotated box. */
  rotatedBounds: boolean;
}

export type MirrorAction = MirrorMode | 'materialHorizontal' | 'materialVertical';

export interface MirrorButton {
  mode: MirrorAction;
  /** The full name: the tooltip. */
  label: string;
  /** The small label under the icon. */
  short: string;
  /** The shell/ToolIcons id. */
  icon: string;
}

const mirror = strings.transform.mirror;

export const MIRROR_BUTTONS: readonly MirrorButton[] = [
  { mode: 'horizontal', label: mirror.horizontal, short: mirror.short.horizontal, icon: 'mirror' },
  {
    mode: 'vertical',
    label: mirror.vertical,
    short: mirror.short.vertical,
    icon: 'mirrorVertical',
  },
  { mode: 'left', label: mirror.left, short: mirror.short.left, icon: 'mirrorLeft' },
  { mode: 'right', label: mirror.right, short: mirror.short.right, icon: 'mirrorRight' },
  { mode: 'top', label: mirror.top, short: mirror.short.top, icon: 'mirrorTop' },
  { mode: 'bottom', label: mirror.bottom, short: mirror.short.bottom, icon: 'mirrorBottom' },
  {
    mode: 'materialHorizontal',
    label: mirror.materialHorizontal,
    short: mirror.short.materialHorizontal,
    icon: 'mirrorMaterialX',
  },
  {
    mode: 'materialVertical',
    label: mirror.materialVertical,
    short: mirror.short.materialVertical,
    icon: 'mirrorMaterialY',
  },
];

export function initialMirrorState(): MirrorFormState {
  return { copy: false, rotatedBounds: true };
}

/**
 * Matrix for a flip of the selection, or null without a selection. `frame`
 * is the selection's rotated frame (used with rotated bounds), `material`
 * the job box.
 */
export function mirrorMatrixFor(
  bbox: Bbox | null,
  mode: MirrorAction,
  options: { frame?: SelectionFrame | null; rotatedBounds?: boolean; material?: Bbox } = {},
): Matrix | null {
  if (bbox === null) {
    return null;
  }
  if (mode === 'materialHorizontal' || mode === 'materialVertical') {
    const centre = bboxCentre(options.material ?? bbox);
    return mode === 'materialHorizontal'
      ? scaleAbout(-1, 1, centre.x, centre.y)
      : scaleAbout(1, -1, centre.x, centre.y);
  }
  const frame = options.frame ?? null;
  if (options.rotatedBounds === true && frame !== null && frame.angle !== 0) {
    return inFrame(frame, mirrorMatrix(frame.extent, mode));
  }
  return mirrorMatrix(bbox, mode);
}

/** The two ends of a path that is one straight line, the mirror line, or null. */
export function mirrorLine(path: Path): [Point, Point] | null {
  const [sub, ...rest] = path.subpaths;
  if (sub === undefined || rest.length > 0 || sub.closed) {
    return null;
  }
  const [segment, ...more] = sub.segments;
  if (segment === undefined || more.length > 0 || segment.kind !== 'line') {
    return null;
  }
  return [sub.start, segment.to];
}

/** Reflection across a mirror line. */
export function mirrorAcross(line: [Point, Point]): Matrix {
  return mirrorAcrossLine(line[0], line[1]);
}
