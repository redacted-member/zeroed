// Every English string the interface shows. Nothing user visible is written
// inline in a component. Each area keeps its strings in strings/<area>.ts;
// a new area adds its file and one spread line here. Areas used only by
// dialogs that load on demand (dialogs, prefs, machineSetup) are imported
// by those dialogs directly, and the drawing, modeling and toolpaths areas,
// used mostly by forms, are spread in lib/formStrings.ts, so they stay out
// of the start bundle. strings/startup.ts holds the few of their strings the
// start bundle needs (form titles, the toolpath list). Strings of the areas
// below that only lazily loaded modules show wait in strings/deferred.ts.

import { bitmapsStrings } from './strings/bitmaps';
import { clipartStrings } from './strings/clipart';
import { fileStrings } from './strings/file';
import { gadgetsStrings } from './strings/gadgets';
import { layersStrings } from './strings/layers';
import { machineStrings } from './strings/machine';
import { menusStrings } from './strings/menus';
import { shellStrings } from './strings/shell';
import { startupStrings } from './strings/startup';

export const strings = {
  ...shellStrings,
  ...menusStrings,
  ...layersStrings,
  ...startupStrings,
  ...machineStrings,
  ...fileStrings,
  ...bitmapsStrings,
  ...gadgetsStrings,
  ...clipartStrings,
} as const;
