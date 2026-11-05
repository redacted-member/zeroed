// Which component a point of the model belongs to, for picking components
// in the 3D view: the topmost visible one whose box holds the point.

import type { ComponentSummary } from '../../bridge/model';

export function componentAt(
  components: readonly ComponentSummary[],
  x: number,
  y: number,
): ComponentSummary | null {
  for (let i = components.length - 1; i >= 0; i -= 1) {
    const c = components[i];
    if (c === undefined || !c.visible || c.bbox === undefined) {
      continue;
    }
    const [minX, minY, maxX, maxY] = c.bbox;
    if (x >= minX && x <= maxX && y >= minY && y <= maxY) {
      return c;
    }
  }
  return null;
}
