// Pure state for the Distort form: which box the envelope starts round,
// picking and dragging its nodes, and the ops.distort payload.

import type { DistortArgs, DistortMode, StoredDistortion } from '../../bridge/api';
import type { Bbox, Path, Point } from '../../bridge/document';
import {
  cornerEnvelope,
  envelopeNodeAt,
  envelopeNodes,
  fromEnvelopeFrame,
  moveNode,
  toEnvelopeFrame,
  withSides,
  type Envelope,
  type EnvelopeNodeRef,
} from '../../lib/envelope';
import { rectPath } from '../../lib/geometry';

/** The box the envelope starts round: the selected vectors and the
 * selected components. Null when there is nothing or the box has no area. */
export function targetBox(selection: Bbox | null, components: Bbox | null): Bbox | null {
  const boxes = [selection, components].filter((b): b is Bbox => b !== null);
  if (boxes.length === 0) {
    return null;
  }
  const box: Bbox = [
    Math.min(...boxes.map((b) => b[0])),
    Math.min(...boxes.map((b) => b[1])),
    Math.max(...boxes.map((b) => b[2])),
    Math.max(...boxes.map((b) => b[3])),
  ];
  return box[2] - box[0] > 1e-9 && box[3] - box[1] > 1e-9 ? box : null;
}

/** The envelope a fresh distortion starts with round `box`: its four
 * corners joined by straight spans. */
export function startEnvelope(box: Bbox | null): Envelope | null {
  return box === null ? null : cornerEnvelope(box);
}

export type NodeRef = EnvelopeNodeRef;

/** The node nearest `p` within `tolerance` millimetres, if any. */
export const pickNode = envelopeNodeAt;

/** The envelope with a dragged node moved to `to`. */
export function dragNode(envelope: Envelope, node: NodeRef, to: Point): Envelope {
  return moveNode(envelope, node.edge, node.index, to);
}

/** True once any node has left its starting place. */
export function isBent(envelope: Envelope, start: Envelope): boolean {
  const same = (a: Point[] | undefined, b: Point[] | undefined) =>
    (a ?? []).length === (b ?? []).length &&
    (a ?? []).every((p, k) => {
      const q = (b ?? [])[k];
      return q !== undefined && Math.hypot(p.x - q.x, p.y - q.y) < 1e-9;
    });
  return !(
    same(envelope.bottom, start.bottom) &&
    same(envelope.top, start.top) &&
    same(envelope.left, start.left) &&
    same(envelope.right, start.right)
  );
}

/** Small squares on every node, `size` millimetres across. */
export function nodeMarkers(envelope: Envelope, size: number): Path[] {
  const half = size / 2;
  return envelopeNodes(envelope).map(({ point }) =>
    rectPath(point.x - half, point.y - half, size, size),
  );
}

/** Why the selection cannot be distorted:
 * vectors and components together, or more than one component. */
export function distortTargetProblem(
  vectors: number,
  components: number,
): 'mixed' | 'several' | null {
  if (vectors > 0 && components > 0) {
    return 'mixed';
  }
  return components > 1 ? 'several' : null;
}

/** Kernel arguments for ops.distort in Bounding Box mode; `angle` is the
 * frame of Use Rotated Bounds (the envelope is in that frame). */
export function distortArgs(
  envelope: Envelope,
  ids: number[],
  componentIds: number[],
  angle = 0,
): DistortArgs {
  const edges: NonNullable<DistortArgs['envelope']> = {
    bottom: envelope.bottom,
    top: envelope.top,
  };
  if (envelope.left !== undefined && envelope.right !== undefined) {
    edges.left = envelope.left;
    edges.right = envelope.right;
  }
  if (envelope.lines !== undefined) {
    edges.lines = envelope.lines;
  }
  const args: DistortArgs = { ids, componentIds, bbox: envelope.bbox, envelope: edges };
  if (angle !== 0) {
    args.angle = angle;
  }
  return args;
}

/** The mode the selection suggests: the last
 * two selected open vectors are the curves to bend between, else the last
 * one the curve to bend along, else the bounding box. `open` tells which
 * ids are open vectors; there must be something left to bend. */
export function modeFromSelection(
  ids: readonly number[],
  open: (id: number) => boolean,
): DistortMode {
  const n = ids.length;
  const last = ids[n - 1];
  const before = ids[n - 2];
  if (n >= 3 && last !== undefined && before !== undefined && open(last) && open(before)) {
    return 'twoCurves';
  }
  if (n >= 2 && last !== undefined && open(last)) {
    return 'curve';
  }
  return 'box';
}

/** The objects to bend and the curves of a mode, from the selection in
 * the order it was made. */
export function splitSelection(
  ids: readonly number[],
  mode: DistortMode,
): { objects: number[]; curves: number[] } {
  const count = mode === 'twoCurves' ? 2 : mode === 'curve' ? 1 : 0;
  const cut = Math.max(0, ids.length - count);
  return { objects: ids.slice(0, cut), curves: ids.slice(cut) };
}

/** Kernel arguments for the curve modes. */
export function curveArgs(
  mode: Exclude<DistortMode, 'box'>,
  objects: number[],
  curves: number[],
  componentIds: number[],
  angle = 0,
): DistortArgs {
  const args: DistortArgs = { ids: objects, componentIds, mode, curves };
  if (angle !== 0) {
    args.angle = angle;
  }
  return args;
}

/** The envelope a stored distortion was bent with, in its frame, its
 * sides as node lists. */
export function storedEnvelope(stored: StoredDistortion): Envelope {
  const envelope: Envelope = { bbox: stored.bbox, bottom: stored.bottom, top: stored.top };
  if (stored.left !== undefined && stored.right !== undefined) {
    envelope.left = stored.left;
    envelope.right = stored.right;
  }
  if (stored.lines !== undefined) {
    envelope.lines = stored.lines;
  }
  return withSides(envelope);
}

/** A job point in the frame turned `angle` degrees about the origin. */
export const toFrame = toEnvelopeFrame;

/** A frame point back in job space. */
export const fromFrame = fromEnvelopeFrame;
