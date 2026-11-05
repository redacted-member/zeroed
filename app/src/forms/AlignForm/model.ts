// Pure state for the Alignment Tools form: the align modes and Space
// Selection.

import { strings } from '../../lib/formStrings';
import type { Bbox } from '../../bridge/document';
import { translation } from '../../lib/geometry';
import {
  bboxHeight,
  bboxWidth,
  type AlignMode,
  type Placed,
  type PlacedMatrix,
} from '../../lib/transformMath';

export interface AlignFormState {
  /** Space the other objects inside the last selected one. */
  insideLast: boolean;
}

export interface AlignButton {
  mode: AlignMode;
  /** The full name: the tooltip. */
  label: string;
  /** The small label under the icon. */
  short: string;
  /** The shell/ToolIcons id, shared with the Align Objects group and the Align menu. */
  icon: string;
}

const text = strings.transform.align;

/** Align to Selection: onto the last selected object,
 * the top and bottom onto the outside of its edge. */
export const ALIGN_BUTTONS: readonly AlignButton[] = [
  { mode: 'centre', label: text.centre, short: text.short.centre, icon: 'alignCenter' },
  { mode: 'vCentre', label: text.vCentre, short: text.short.vCentre, icon: 'alignVCenter' },
  { mode: 'hCentre', label: text.hCentre, short: text.short.hCentre, icon: 'alignHCenter' },
  { mode: 'left', label: text.left, short: text.short.left, icon: 'alignLeft' },
  { mode: 'right', label: text.right, short: text.short.right, icon: 'alignRight' },
  {
    mode: 'outsideTop',
    label: text.outsideTop,
    short: text.short.outsideTop,
    icon: 'alignOutsideTop',
  },
  {
    mode: 'outsideBottom',
    label: text.outsideBottom,
    short: text.short.outsideBottom,
    icon: 'alignOutsideBottom',
  },
];

export function initialAlignState(): AlignFormState {
  return { insideLast: false };
}

/** The Align to Material buttons: center both ways, or along X or Y only. */
export const MATERIAL_BUTTONS: readonly AlignButton[] = [
  {
    mode: 'centre',
    label: text.materialCentre,
    short: text.short.materialCentre,
    icon: 'alignMaterial',
  },
  { mode: 'hCentre', label: text.materialX, short: text.short.materialX, icon: 'alignMaterialX' },
  { mode: 'vCentre', label: text.materialY, short: text.short.materialY, icon: 'alignMaterialY' },
];

/** The Space Selection buttons. */
export const SPACE_BUTTONS: ReadonlyArray<{
  horizontal: boolean;
  label: string;
  short: string;
  icon: string;
}> = [
  {
    horizontal: true,
    label: text.spaceHorizontal,
    short: text.short.spaceHorizontal,
    icon: 'spaceHorizontal',
  },
  {
    horizontal: false,
    label: text.spaceVertical,
    short: text.short.spaceVertical,
    icon: 'spaceVertical',
  },
];

/**
 * Space Selection inside the last object: the others share the bounds of
 * `inside` with equal gaps, including the gaps at its two edges.
 */
export function spaceInside(
  items: readonly Placed[],
  inside: Bbox,
  horizontal: boolean,
): PlacedMatrix[] {
  const size = (b: Bbox) => (horizontal ? bboxWidth(b) : bboxHeight(b));
  const low = (b: Bbox) => (horizontal ? b[0] : b[1]);
  const sorted = [...items].sort((a, b) => low(a.bbox) - low(b.bbox));
  const total = sorted.reduce((sum, item) => sum + size(item.bbox), 0);
  const gap = (size(inside) - total) / (sorted.length + 1);
  let cursor = low(inside) + gap;
  const moves = new Map<number, number>();
  for (const item of sorted) {
    moves.set(item.id, cursor - low(item.bbox));
    cursor += size(item.bbox) + gap;
  }
  return items.map((item) => {
    const d = moves.get(item.id) ?? 0;
    return { id: item.id, matrix: horizontal ? translation(d, 0) : translation(0, d) };
  });
}

/**
 * Space Selection in selection order (Ctrl): the first and last selected
 * stay put and the centres of the rest are spread evenly between them, in
 * the order they were picked.
 */
export function spaceInOrder(items: readonly Placed[], horizontal: boolean): PlacedMatrix[] {
  const centre = (b: Bbox) => (horizontal ? (b[0] + b[2]) / 2 : (b[1] + b[3]) / 2);
  const first = items[0];
  const last = items[items.length - 1];
  if (first === undefined || last === undefined || items.length < 3) {
    return items.map((item) => ({ id: item.id, matrix: translation(0, 0) }));
  }
  const start = centre(first.bbox);
  const step = (centre(last.bbox) - start) / (items.length - 1);
  return items.map((item, index) => {
    const d = start + step * index - centre(item.bbox);
    return { id: item.id, matrix: horizontal ? translation(d, 0) : translation(0, d) };
  });
}
