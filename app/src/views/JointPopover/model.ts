// The seam popover's pure parts: what a pick changes (one connection, or
// with "Use for all like this" its whole class) and where the popover
// stands so it stays inside the view.

import type {
  ConnectionRow,
  FurnitureJoint,
  FurnitureJoints,
  SetJointArgs,
} from '../../bridge/furniture';
import { withClassJoint, withConnectionJoint } from '../../forms/FurnitureDesigner/model';

/**
 * The kernel change for picking `joint` on a seam of a made design: the
 * connection alone, or its class when `forAll`.
 */
export function pickArgs(
  design: number,
  connection: ConnectionRow,
  joint: FurnitureJoint,
  forAll: boolean,
): SetJointArgs {
  return forAll
    ? { design, scope: 'class', key: connection.class, joint }
    : { design, scope: 'connection', key: connection.key, joint };
}

/**
 * The joint choices after picking `joint` on a seam of a design not made
 * yet. For all like this sets the class and drops the class's connections
 * set by hand, so every seam of the class takes it.
 */
export function pickLocally(
  joints: FurnitureJoints,
  connection: ConnectionRow,
  joint: FurnitureJoint,
  forAll: boolean,
  connections: readonly ConnectionRow[],
): FurnitureJoints {
  if (!forAll) {
    return withConnectionJoint(joints, connection.key, joint);
  }
  const clearing = connections.filter((c) => c.class === connection.class).map((c) => c.key);
  return withClassJoint(joints, connection.class, joint, clearing);
}

/** The keys a class pick on a made design must clear: its connections set by hand. */
export function classOverrides(
  connection: ConnectionRow,
  connections: readonly ConnectionRow[],
): string[] {
  return connections
    .filter((c) => c.class === connection.class && c.source === 'connection')
    .map((c) => c.key);
}

/**
 * Where the popover's top left goes for a click at (`x`, `y`) in a view of
 * `viewW` by `viewH`: below right of the click, flipped left or up where it
 * would run out, and never past the view's edges (`margin` clear).
 */
export function popoverPlace(
  x: number,
  y: number,
  width: number,
  height: number,
  viewW: number,
  viewH: number,
  margin = 8,
): { left: number; top: number } {
  const offset = 12;
  let left = x + offset;
  if (left + width > viewW - margin) {
    left = x - offset - width;
  }
  let top = y + offset;
  if (top + height > viewH - margin) {
    top = y - offset - height;
  }
  const clamp = (v: number, size: number, limit: number) =>
    Math.max(margin, Math.min(v, limit - margin - size));
  return { left: clamp(left, width, viewW), top: clamp(top, height, viewH) };
}
