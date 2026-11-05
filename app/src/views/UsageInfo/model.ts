// The usage information line along the bottom of the 3D view: what the
// mouse and keys do in the mode the view is in, following the navigation
// scheme chosen in the preferences.

import type { Navigation3d } from '../../bridge/prefs';
import { strings } from '../../lib/formStrings';

/** What the 3D view is doing, which decides the controls it lists. */
export type UsageMode = 'navigate' | 'model' | 'sculpt' | 'transform' | 'slice';

/** The control hints for `mode`, each a short "action: gesture" phrase. */
export function usageLines(mode: UsageMode, navigation: Navigation3d, moveKey: string): string[] {
  const t = strings.usageInfo;
  switch (mode) {
    case 'sculpt':
      return [t.sculpt, t.sculptTurn, t.pan, t.sculptBrush];
    case 'transform':
      return [t.transformDrag, t.transformValues, t.transformDone];
    case 'slice':
    case 'navigate':
    case 'model': {
      const lines = [
        t.rotate,
        navigation === 'legacy' ? t.panLegacy : t.panModern,
        navigation === 'legacy' ? t.zoomLegacy : t.zoomModern,
      ];
      if (mode === 'slice') {
        return [...lines, t.sliceMove, t.sliceInsert, t.sliceNudge];
      }
      return mode === 'model' ? [...lines, t.moveComponent(moveKey), t.editComponent] : lines;
    }
  }
}
