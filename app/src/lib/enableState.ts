// Which commands can run in the current state of the app
// greys them (the Enable states table below). The menus
// draw a command that cannot run as disabled, and runEnabledCommand (menus
// and keyboard shortcuts) ignores it, so a key does nothing where its menu
// item is greyed.
//
// The Design tab and the Toolpaths panel icons never grey:
// whatever is selected, in the 2D or the 3D view, every icon keeps its
// colors, and a tool that needs a selection says so in its form. Their
// commands therefore have no rule here.

import type { AppCommand } from './shortcuts';

/** What the enable rules look at. */
export interface EnableState {
  /** Vectors selected in the 2D view. */
  vectors: number;
  /** Components picked in the 3D view or the Component Tree. */
  components: number;
  canUndo: boolean;
  canRedo: boolean;
  /** Objects a paste would add; 0 when nothing was copied. */
  clipboard: number;
  /** Whether a toolpath is selected in the toolpath list. */
  toolpathSelected: boolean;
  /** Toolpaths > Toolpath Drawing > Draw 2D Previews is on. */
  draw2dPreviews: boolean;
  doubleSided: boolean;
  rotary: boolean;
  /** True in the desktop app, where the host can open folders. */
  native: boolean;
}

/** Nothing selected, nothing to undo, a flat single sided job in a browser. */
export const IDLE_STATE: EnableState = {
  vectors: 0,
  components: 0,
  canUndo: false,
  canRedo: false,
  clipboard: 0,
  toolpathSelected: false,
  draw2dPreviews: true,
  doubleSided: false,
  rotary: false,
  native: false,
};

type Rule = (state: EnableState) => boolean;

const hasSelection: Rule = (s) => s.vectors + s.components > 0;
const previewsDrawn: Rule = (s) => s.draw2dPreviews;
const hasComponent: Rule = (s) => s.components > 0;
const componentAndVectors: Rule = (s) => s.components > 0 && s.vectors > 0;

const RULES: Partial<Record<AppCommand, Rule>> = {
  'edit.undo': (s) => s.canUndo,
  'edit.redo': (s) => s.canRedo,
  'edit.cut': hasSelection,
  'edit.copy': hasSelection,
  'edit.delete': hasSelection,
  'edit.paste': (s) => s.clipboard > 0,
  'edit.swapSides': (s) => s.doubleSided,
  'view.sideTop': (s) => s.doubleSided,
  'view.sideBottom': (s) => s.doubleSided,
  'view.multiSided': (s) => s.doubleSided,
  'view.autoWrap': (s) => s.rotary,
  'toolpaths.saveTemplateSelected': (s) => s.toolpathSelected,
  'toolpaths.drawRapids': previewsDrawn,
  'toolpaths.drawPlunges': previewsDrawn,
  'toolpaths.drawRetracts': previewsDrawn,
  'toolpaths.drawJoins': previewsDrawn,
  'file.openDataFolder': (s) => s.native,
  // Model menu items that act on the selected components.
  'model.bake': hasComponent,
  'model.exportClipart': hasComponent,
  'model.sculpt': hasComponent,
  'model.smooth': hasComponent,
  'model.boundary': hasComponent,
  'model.emboss': hasComponent,
  'model.distort': hasComponent,
  'model.properties': hasComponent,
  // Reset Inside / Outside Selected Vectors and Split Selected Component
  // cut the selected component with the selected vectors.
  'model.resetInside': componentAndVectors,
  'model.resetOutside': componentAndVectors,
  'model.split': componentAndVectors,
};

/** Whether `command` can run in `state`. Commands without a rule always can. */
export function commandEnabled(command: AppCommand, state: EnableState): boolean {
  const rule = RULES[command];
  return rule === undefined || rule(state);
}

/** The commands that have an enable rule, for tests and documentation. */
export function gatedCommands(): AppCommand[] {
  return Object.keys(RULES) as AppCommand[];
}

/**
 * Whether a Design tab or Toolpaths panel icon is enabled: whenever it has
 * something to run. These icons never grey by state.
 */
export function toolEnabled(tool: { form?: string; command?: AppCommand }): boolean {
  return tool.form !== undefined || tool.command !== undefined;
}
