// Keyboard shortcuts handled by the web view, with their defaults. Menu accelerators are owned by the native menu and
// arrive as menu commands instead; in the browser the table below covers them
// too so the same keys work in both hosts.

import type { FormId } from '../state/uiStore';
import type { MenuCommand } from './menu';

/** Commands the shell can run: menu commands plus view level ones. */
export type AppCommand =
  | MenuCommand
  | 'edit.selectNone'
  | 'edit.copy'
  | 'edit.cut'
  | 'edit.paste'
  | 'edit.duplicate'
  | 'edit.group'
  | 'edit.ungroup'
  | 'edit.ungroupToGroupLayer'
  | 'edit.ungroupDeep'
  | 'edit.ungroupDeepToGroupLayer'
  | 'edit.mirrorMaterialHorizontalCopy'
  | 'edit.mirrorMaterialVerticalCopy'
  | 'form.join'
  | 'form.measure'
  | 'tool.zoomBox'
  | 'tabs.layers'
  | 'tabs.drawingTab'
  | 'edit.textToCurves'
  | 'edit.weld'
  | 'edit.subtract'
  | 'edit.intersect'
  | 'edit.closeVectors'
  | 'edit.closeSmooth'
  | 'edit.closeMoveEnds'
  | 'edit.rotateCcw45'
  | 'edit.rotateCw45'
  | 'edit.mirrorHorizontal'
  | 'edit.mirrorVertical'
  | 'edit.mirrorHorizontalCopy'
  | 'edit.mirrorVerticalCopy'
  | 'edit.mirrorMaterialHorizontal'
  | 'edit.mirrorMaterialVertical'
  | 'edit.nudge'
  | 'view.zoomToSelection'
  | 'view.snapSettings'
  | 'view.modelVolume'
  | 'view.centreSelection'
  | 'edit.centreInMaterial'
  | 'view.tileVertical'
  | 'view.tileHorizontal'
  | 'tool.select'
  | 'tool.node'
  | 'tool.transform'
  | 'tool.nodeToggle'
  | 'tabs.drawing'
  | 'tabs.toggleDrawingModeling'
  | 'tabs.toolpaths'
  | 'form.move'
  | 'form.scale'
  | 'form.rotate'
  | 'view.sideTop'
  | 'view.sideBottom'
  | 'edit.swapSides'
  | 'select.insideBoundary'
  | 'bitmap.editPicture'
  | 'bitmap.crop'
  | 'text.removeFromCurve'
  | 'text.adjust'
  | 'toolpaths.edit'
  | 'toolpaths.duplicate'
  | 'toolpaths.delete'
  | 'toolpaths.summary';

export interface Shortcut {
  /** `KeyboardEvent.key`, compared case insensitively for letters. */
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  command: AppCommand;
}

export interface NudgeShortcut {
  dx: number;
  dy: number;
}

export const SHORTCUTS: readonly Shortcut[] = [
  { key: 'z', ctrl: true, command: 'edit.undo' },
  { key: 'y', ctrl: true, command: 'edit.redo' },
  { key: 'z', ctrl: true, shift: true, command: 'edit.redo' },
  { key: 'c', ctrl: true, command: 'edit.copy' },
  { key: 'x', ctrl: true, command: 'edit.cut' },
  { key: 'v', ctrl: true, command: 'edit.paste' },
  { key: 'n', ctrl: true, command: 'file.new' },
  { key: 'o', ctrl: true, command: 'file.open' },
  { key: 's', ctrl: true, command: 'file.save' },
  { key: 'i', ctrl: true, command: 'file.importVectorsBitmaps' },
  { key: 'd', ctrl: true, command: 'tabs.drawingTab' },
  { key: 'l', ctrl: true, command: 'tabs.layers' },
  { key: 'm', ctrl: true, command: 'form.measure' },
  { key: 'j', command: 'form.join' },
  { key: 'z', command: 'tool.zoomBox' },
  { key: 'a', ctrl: true, command: 'edit.selectAll' },
  { key: ',', ctrl: true, command: 'app.preferences' },
  { key: 'Delete', command: 'edit.delete' },
  { key: 'Backspace', command: 'edit.delete' },
  { key: 'n', command: 'tool.nodeToggle' },
  { key: 't', command: 'form.scale' },
  { key: 'm', command: 'form.move' },
  { key: 'r', command: 'form.rotate' },
  { key: '1', command: 'view.sideTop' },
  { key: '2', command: 'view.sideBottom' },
  { key: '=', command: 'view.multiSided' },
  { key: '9', command: 'edit.rotateCcw45' },
  { key: '0', command: 'edit.rotateCw45' },
  { key: 'g', command: 'edit.group' },
  { key: 'u', command: 'edit.ungroup' },
  { key: 'u', ctrl: true, command: 'edit.ungroupToGroupLayer' },
  { key: 'u', shift: true, command: 'edit.ungroupDeep' },
  { key: 'u', ctrl: true, shift: true, command: 'edit.ungroupDeepToGroupLayer' },
  { key: 'h', command: 'edit.mirrorHorizontal' },
  { key: 'v', command: 'edit.mirrorVertical' },
  { key: 'h', ctrl: true, command: 'edit.mirrorHorizontalCopy' },
  { key: 'h', ctrl: true, shift: true, command: 'edit.mirrorMaterialHorizontalCopy' },
  { key: 'v', ctrl: true, shift: true, command: 'edit.mirrorMaterialVerticalCopy' },
  { key: 'h', shift: true, command: 'edit.mirrorMaterialHorizontal' },
  { key: 'v', shift: true, command: 'edit.mirrorMaterialVertical' },
  { key: 'F1', command: 'help.contents' },
  { key: 'F2', command: 'view.mode2d' },
  { key: 'F3', command: 'view.mode3d' },
  { key: 'F4', command: 'view.snapSettings' },
  { key: 'F6', command: 'view.zoomToJob' },
  { key: 'f', command: 'view.zoomToJob' },
  { key: 'f', ctrl: true, command: 'view.zoomToSelection' },
  { key: 'F8', command: 'tabs.toggleDrawingModeling' },
  { key: 'F9', command: 'edit.centreInMaterial' },
  { key: 'F10', command: 'form.align' },
  { key: 'F11', command: 'tabs.drawing' },
  { key: 'F12', command: 'tabs.toolpaths' },
  { key: 'PageUp', command: 'view.tileVertical' },
  { key: 'PageDown', command: 'view.tileHorizontal' },
  { key: 'v', ctrl: true, shift: true, alt: true, command: 'view.modelVolume' },
];

/** Forms the T, M, R, J, Ctrl+M and F10 keys open on the Drawing tab. */
export const TRANSFORM_FORM_KEYS = {
  'form.join': 'join',
  'form.measure': 'measure',
  'form.move': 'move',
  'form.scale': 'setSize',
  'form.rotate': 'rotate',
  'form.align': 'align',
} as const satisfies Partial<Record<AppCommand, FormId>>;

/**
 * Arrow nudges in millimetres: plain = one, Shift = ten, Ctrl = a tenth, and
 * Ctrl+Shift the fixed nudge distance from Snap Settings.
 */
export function nudgeForKey(event: KeyboardEvent, fixedMm = 5): NudgeShortcut | null {
  const ctrl = event.ctrlKey || event.metaKey;
  const step = ctrl && event.shiftKey ? fixedMm : event.shiftKey ? 10 : ctrl ? 0.1 : 1;
  switch (event.key) {
    case 'ArrowLeft':
      return { dx: -step, dy: 0 };
    case 'ArrowRight':
      return { dx: step, dy: 0 };
    case 'ArrowUp':
      return { dx: 0, dy: step };
    case 'ArrowDown':
      return { dx: 0, dy: -step };
    default:
      return null;
  }
}

function matches(shortcut: Shortcut, event: KeyboardEvent): boolean {
  const ctrl = event.ctrlKey || event.metaKey;
  const key = shortcut.key.length === 1 ? shortcut.key.toLowerCase() : shortcut.key;
  const pressed = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  // Option on a Mac turns a letter into another character, so a letter
  // with Alt is matched by the physical key.
  const letterWithAlt = shortcut.alt === true && /^[a-z]$/.test(key);
  return (
    (key === pressed || (letterWithAlt && event.code === `Key${key.toUpperCase()}`)) &&
    ctrl === (shortcut.ctrl ?? false) &&
    event.shiftKey === (shortcut.shift ?? false) &&
    event.altKey === (shortcut.alt ?? false)
  );
}

/** Maps a key event to a command, or null when nothing is bound. */
export function commandForKey(event: KeyboardEvent): AppCommand | null {
  const match = SHORTCUTS.find((shortcut) => matches(shortcut, event));
  return match ? match.command : null;
}
