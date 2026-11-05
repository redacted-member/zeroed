// The node Properties box: the node's position as text in the display unit,
// and the tool command that moves the node to what was typed.

import type { Point } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { lengthText, lengthValue } from '../../lib/formFields';

export interface NodeFields {
  x: string;
  y: string;
}

export function nodeFields(at: Point, units: Units): NodeFields {
  return { x: lengthText(at.x, units), y: lengthText(at.y, units) };
}

/** The node tool command for the typed position, or null while a field
 * does not read. */
export function moveCommand(fields: NodeFields, units: Units): string | null {
  const x = lengthValue(fields.x, units);
  const y = lengthValue(fields.y, units);
  return x === null || y === null ? null : `node-xy:${x},${y}`;
}
