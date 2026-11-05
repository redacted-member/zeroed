// Pure state for the 3D Segmenting form: the segment tree (ids "r", then a
// digit per level, 0 above a cut and 1 below), merges of leaves, per
// segment layout turns, and the segmenting plane fields. Mirrors how the
// kernel reads a plan (model/import/segment.rs).

import {
  DEFAULT_SEGMENT_LAYOUT,
  type CutPlane,
  type SegmentLayout,
  type SegmentNode,
  type SegmentPlan,
  type Vec3,
} from '../../bridge/meshImport';
import type { Units } from '../../bridge/types';
import { modalDialogOpen } from '../../lib/dialogKeys';
import { fieldError, lengthText, lengthValue, numberValue } from '../../lib/formFields';
import type { PopupEntry } from '../../views/PopupMenu/model';

export const ROOT = 'r';

/** The node with `id`, or null. */
export function nodeAt(root: SegmentNode, id: string): SegmentNode | null {
  if (!id.startsWith(ROOT)) {
    return null;
  }
  let node: SegmentNode | undefined = root;
  for (const digit of id.slice(1)) {
    node = node?.children?.[Number(digit)];
    if (node === undefined) {
      return null;
    }
  }
  return node ?? null;
}

function hasChildren(node: SegmentNode | null): boolean {
  return (node?.children?.length ?? 0) === 2 && node?.cut !== null && node?.cut !== undefined;
}

/** A copy of the tree with the node at `id` replaced. */
function replaceAt(root: SegmentNode, id: string, next: SegmentNode): SegmentNode {
  if (id === ROOT) {
    return next;
  }
  const digits = id.slice(1);
  const index = Number(digits[0]);
  const children = [...(root.children ?? [])];
  const child = children[index];
  if (child === undefined) {
    return root;
  }
  children[index] = replaceAt(child, ROOT + digits.slice(1), next);
  return { ...root, children };
}

/** Leaf ids in tree order. */
export function leafIds(root: SegmentNode, id = ROOT): string[] {
  if (!hasChildren(root)) {
    return [id];
  }
  return (root.children ?? []).flatMap((child, k) => leafIds(child, `${id}${k}`));
}

/** The merge group holding leaf `id`, or just the leaf. */
export function groupOf(plan: SegmentPlan, id: string): string[] {
  return plan.merges.find((group) => group.includes(id)) ?? [id];
}

export interface SegmentEntry {
  /** Its first leaf. */
  id: string;
  leaves: string[];
}

/** The segments the kernel makes of `plan`, in tree order. */
export function segmentsOf(plan: SegmentPlan): SegmentEntry[] {
  const leaves = new Set(leafIds(plan.root));
  const out: SegmentEntry[] = [];
  const done = new Set<string>();
  for (const leaf of leafIds(plan.root)) {
    if (done.has(leaf)) {
      continue;
    }
    const group = groupOf(plan, leaf).filter((id) => leaves.has(id));
    group.forEach((id) => done.add(id));
    const ordered = [...leafIds(plan.root)].filter((id) => group.includes(id));
    out.push({ id: ordered[0] ?? leaf, leaves: ordered });
  }
  return out;
}

/** The segment (its first leaf id) that tree node `id` belongs to, or null. */
export function segmentOfNode(plan: SegmentPlan, id: string): string | null {
  return segmentsOf(plan).find((s) => s.leaves.includes(id))?.id ?? null;
}

/** Cuts leaf `id` in two along `cut`; merged leaves are not cut. */
export function split(plan: SegmentPlan, id: string, cut: CutPlane): SegmentPlan {
  const node = nodeAt(plan.root, id);
  if (node === null || hasChildren(node) || groupOf(plan, id).length > 1) {
    return plan;
  }
  const layouts = { ...plan.layouts };
  delete layouts[id];
  return {
    ...plan,
    root: replaceAt(plan.root, id, { cut, children: [{}, {}] }),
    layouts,
  };
}

/** Whether `id` names a node below `parent`. */
function below(id: string, parent: string): boolean {
  return id.length > parent.length && id.startsWith(parent);
}

/** Undoes the cuts under node `id`, forgetting merges and turns inside it. */
export function deleteChildren(plan: SegmentPlan, id: string): SegmentPlan {
  const node = nodeAt(plan.root, id);
  if (node === null || !hasChildren(node)) {
    return plan;
  }
  const merges = plan.merges
    .map((group) => group.filter((leaf) => !below(leaf, id)))
    .filter((group) => group.length > 1);
  const layouts = Object.fromEntries(
    Object.entries(plan.layouts).filter(([key]) => !below(key, id)),
  );
  return { ...plan, root: replaceAt(plan.root, id, {}), merges, layouts };
}

/** Merges the segments holding leaves `a` and `b` into one. */
export function merge(plan: SegmentPlan, a: string, b: string): SegmentPlan {
  const leaves = new Set(leafIds(plan.root));
  if (!leaves.has(a) || !leaves.has(b)) {
    return plan;
  }
  const ga = groupOf(plan, a);
  const gb = groupOf(plan, b);
  if (ga.includes(b)) {
    return plan;
  }
  const joined = leafIds(plan.root).filter((id) => ga.includes(id) || gb.includes(id));
  const merges = [...plan.merges.filter((g) => g !== ga && g !== gb), joined];
  return { ...plan, merges };
}

/** Splits the merge group holding leaf `id` back into single segments. */
export function separate(plan: SegmentPlan, id: string): SegmentPlan {
  return { ...plan, merges: plan.merges.filter((group) => !group.includes(id)) };
}

export function layoutOf(plan: SegmentPlan, segment: string): SegmentLayout {
  return plan.layouts[segment] ?? DEFAULT_SEGMENT_LAYOUT;
}

function wrapQuarter(n: number): number {
  return ((n % 4) + 4) % 4;
}

/** An arrow button: a quarter turn of `segment` about an axis. */
export function turnSegment(
  plan: SegmentPlan,
  segment: string,
  axis: 'x' | 'y' | 'z',
  delta: 1 | -1,
): SegmentPlan {
  const current = layoutOf(plan, segment);
  const key = axis === 'x' ? 'quarterX' : axis === 'y' ? 'quarterY' : 'quarterZ';
  return {
    ...plan,
    layouts: {
      ...plan.layouts,
      [segment]: { ...current, [key]: wrapQuarter(current[key] + delta) },
    },
  };
}

/** Sets the free turns of `segment`, degrees. */
export function freeTurn(
  plan: SegmentPlan,
  segment: string,
  turn: Pick<SegmentLayout, 'rotX' | 'rotY' | 'rotZ'>,
): SegmentPlan {
  return {
    ...plan,
    layouts: { ...plan.layouts, [segment]: { ...layoutOf(plan, segment), ...turn } },
  };
}

/** The segmenting plane fields: position in job units, tilts in degrees. */
export interface PlaneFields {
  x: string;
  y: string;
  z: string;
  angleX: string;
  angleY: string;
}

/** A level plane through the middle of the model. */
export function defaultPlane(centre: { x: number; y: number }, size: Vec3): CutPlane {
  return { point: [centre.x, centre.y, size[2] / 2], angleX: 0, angleY: 0 };
}

export function planeFields(plane: CutPlane, units: Units): PlaneFields {
  const [x, y, z] = plane.point;
  return {
    x: lengthText(x, units),
    y: lengthText(y, units),
    z: lengthText(z, units),
    angleX: String(plane.angleX),
    angleY: String(plane.angleY),
  };
}

export function planeFieldErrors(
  fields: PlaneFields,
  units: Units,
): Partial<Record<keyof PlaneFields, string>> {
  const errors: Partial<Record<keyof PlaneFields, string>> = {};
  for (const key of ['x', 'y', 'z'] as const) {
    const error = fieldError(lengthValue(fields[key], units));
    if (error !== undefined) {
      errors[key] = error;
    }
  }
  for (const key of ['angleX', 'angleY'] as const) {
    const error = fieldError(numberValue(fields[key]), { min: -90, max: 90 });
    if (error !== undefined) {
      errors[key] = error;
    }
  }
  return errors;
}

/** The plane the fields describe, or null while one is not valid. */
export function planeOf(fields: PlaneFields, units: Units): CutPlane | null {
  if (Object.keys(planeFieldErrors(fields, units)).length > 0) {
    return null;
  }
  const n = (text: string) => lengthValue(text, units) ?? 0;
  return {
    point: [n(fields.x), n(fields.y), n(fields.z)],
    angleX: numberValue(fields.angleX) ?? 0,
    angleY: numberValue(fields.angleY) ?? 0,
  };
}

export type TreeAction = { kind: 'deleteChildren' } | { kind: 'separate' };

/** The right click menu of a tree node. */
export function treeMenu(
  plan: SegmentPlan,
  id: string,
  labels: { deleteChildren: string; separate: string },
): PopupEntry<TreeAction>[] {
  return [
    {
      id: 'deleteChildren',
      label: labels.deleteChildren,
      icon: 'segmentDeleteChildren',
      disabled: !hasChildren(nodeAt(plan.root, id)),
      action: { kind: 'deleteChildren' },
    },
    {
      id: 'separate',
      label: labels.separate,
      icon: 'segmentSeparate',
      disabled: groupOf(plan, id).length < 2,
      action: { kind: 'separate' },
    },
  ];
}

/** Whether a tree node has children, for the tree view. */
export function isCut(plan: SegmentPlan, id: string): boolean {
  return hasChildren(nodeAt(plan.root, id));
}

/** The parts of a key down the free rotation toggle reads. */
export interface FreeRotationKey {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  target: EventTarget | null;
}

/**
 * Whether a key down toggles the free rotation fields while laying out:
 * O, not typed into a field, and not while a modal dialog is open.
 */
export function togglesFreeRotation(event: FreeRotationKey): boolean {
  const target = event.target as { tagName?: unknown; isContentEditable?: unknown } | null;
  const typing =
    target !== null && (target.tagName === 'INPUT' || target.isContentEditable === true);
  return (
    event.key.toLowerCase() === 'o' &&
    !typing &&
    !event.metaKey &&
    !event.ctrlKey &&
    !modalDialogOpen()
  );
}
