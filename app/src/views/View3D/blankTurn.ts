// A rotary job's preview turns the blank under an upright tool, as the machine
// does. The blank and everything cut or drawn on it turn together about its
// axis, which runs along scene x (blank along X) or scene z (blank along Y) at
// one radius below the scene origin.

import { Matrix4, Vector3 } from 'three';

import type { MovesReply } from '../../bridge/cam';
import type { RotaryAxis } from '../../bridge/types';
import { moveGroup } from '../../lib/moveGroups';
import type { Box3 } from './camera3d';

/** The transform that turns the blank by `radians` about its axis. */
export function blankTurnMatrix(axis: RotaryAxis, radius: number, radians: number): Matrix4 {
  const turn =
    axis === 'x' ? new Matrix4().makeRotationX(radians) : new Matrix4().makeRotationZ(radians);
  return new Matrix4()
    .makeTranslation(0, -radius, 0)
    .multiply(turn)
    .multiply(new Matrix4().makeTranslation(0, radius, 0));
}

/** How much of a full spread round the blank still counts as one place. */
const MIN_GATHERED = 0.1;

/**
 * The turn that brings a rotary job's cutting moves up to face the viewer while
 * no tool is running: their mean angle round the blank, so a design on the far
 * side does not look as if nothing was cut. Moves spread evenly all the way
 * round have no such place and leave the blank where it is.
 */
export function restingTurn(moves: MovesReply, axis: RotaryAxis, radius: number): number {
  if (radius <= 0) {
    return 0;
  }
  const flat = axis === 'x' ? 1 : 0;
  let sin = 0;
  let cos = 0;
  let count = 0;
  for (const toolpath of moves.toolpaths) {
    if (!toolpath.visible) {
      continue;
    }
    for (let i = toolpath.offset; i < toolpath.offset + toolpath.count; i += 1) {
      if (moveGroup(moves.kind[i]) !== 'cut') {
        continue;
      }
      const angle = (moves.xyz[i * 3 + flat] ?? 0) / radius;
      sin += Math.sin(angle);
      cos += Math.cos(angle);
      count += 1;
    }
  }
  if (count === 0 || Math.hypot(sin, cos) / count < MIN_GATHERED) {
    return 0;
  }
  return Math.atan2(sin, cos);
}

/** The box that holds `box` once `matrix` has moved it. */
export function movedBox(box: Box3, matrix: Matrix4): Box3 {
  const min: [number, number, number] = [Infinity, Infinity, Infinity];
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity];
  for (const x of [box.min[0], box.max[0]]) {
    for (const y of [box.min[1], box.max[1]]) {
      for (const z of [box.min[2], box.max[2]]) {
        const p = new Vector3(x, y, z).applyMatrix4(matrix);
        const at = [p.x, p.y, p.z] as const;
        at.forEach((value, i) => {
          min[i] = Math.min(min[i] ?? value, value);
          max[i] = Math.max(max[i] ?? value, value);
        });
      }
    }
  }
  return { min, max };
}
