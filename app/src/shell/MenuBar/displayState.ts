// The display toggles the Toolpaths > Toolpath Drawing and View menus show
// as check marks, taken from the preferences, and the preference each one
// flips.

import type { Preferences, PrefsPatch, ToolpathPrefs, View3dPrefs } from '../../bridge/prefs';
import type { AppCommand } from '../../lib/shortcuts';

export type DrawingToggle =
  | 'draw2dPreviews'
  | 'solidPreviews'
  | 'autoOpen3d'
  | 'drawRapids'
  | 'drawPlunges'
  | 'drawRetracts'
  | 'drawJoins';

export type ViewToggle =
  | 'drawOrigin'
  | 'drawModelingPlane'
  | 'multiSided'
  | 'autoWrap'
  | 'colorShaded'
  | 'lightFollows'
  | 'shadowShading';

export interface DisplayState {
  toolpaths: Pick<ToolpathPrefs, DrawingToggle>;
  view3d: Pick<View3dPrefs, ViewToggle>;
  /** Whether the job is a rotary job, for Auto-Wrapping View. */
  rotary: boolean;
}

/** Menu commands that flip a toolpath drawing preference. */
export const DRAWING_TOGGLES: Readonly<Partial<Record<AppCommand, DrawingToggle>>> = {
  'toolpaths.draw2dPreviews': 'draw2dPreviews',
  'toolpaths.solidPreviews': 'solidPreviews',
  'toolpaths.autoOpen3d': 'autoOpen3d',
  'toolpaths.drawRapids': 'drawRapids',
  'toolpaths.drawPlunges': 'drawPlunges',
  'toolpaths.drawRetracts': 'drawRetracts',
  'toolpaths.drawJoins': 'drawJoins',
};

/** Menu commands that flip a 3D view preference. */
export const VIEW_TOGGLES: Readonly<Partial<Record<AppCommand, ViewToggle>>> = {
  'view.drawOrigin': 'drawOrigin',
  'view.drawModelingPlane': 'drawModelingPlane',
  'view.multiSided': 'multiSided',
  'view.autoWrap': 'autoWrap',
  'view.colorShaded': 'colorShaded',
  'view.lightFollows': 'lightFollows',
  'view.shadowShading': 'shadowShading',
};

/** The toggles' current state. */
export function displayState(prefs: Preferences, rotary: boolean): DisplayState {
  const t = prefs.toolpaths;
  const v = prefs.view3d;
  return {
    toolpaths: {
      draw2dPreviews: t.draw2dPreviews,
      solidPreviews: t.solidPreviews,
      autoOpen3d: t.autoOpen3d,
      drawRapids: t.drawRapids,
      drawPlunges: t.drawPlunges,
      drawRetracts: t.drawRetracts,
      drawJoins: t.drawJoins,
    },
    view3d: {
      drawOrigin: v.drawOrigin,
      drawModelingPlane: v.drawModelingPlane,
      multiSided: v.multiSided,
      autoWrap: v.autoWrap,
      colorShaded: v.colorShaded,
      lightFollows: v.lightFollows,
      shadowShading: v.shadowShading,
    },
    rotary,
  };
}

/** The preferences patch that flips `command`'s toggle, or null for another command. */
export function togglePatch(command: AppCommand, prefs: Preferences): PrefsPatch | null {
  const drawing = DRAWING_TOGGLES[command];
  if (drawing !== undefined) {
    return { toolpaths: { [drawing]: !prefs.toolpaths[drawing] } };
  }
  const view = VIEW_TOGGLES[command];
  if (view !== undefined) {
    return { view3d: { [view]: !prefs.view3d[view] } };
  }
  return null;
}
