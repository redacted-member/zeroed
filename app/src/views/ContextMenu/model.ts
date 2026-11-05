// Builds the right click menu of the drawing view from the current state.
// Pure so that the menu contents can be tested without the DOM.

import { keys } from '../../lib/keyLabels';
import type { AppCommand } from '../../lib/shortcuts';
import { strings } from '../../lib/strings';
import { menuIcon } from '../../shell/MenuBar/menuIcons';
import { isSeparator, isSubmenu, type PopupEntry, type PopupItem } from '../PopupMenu/model';

export { isSeparator };

export type MenuKind = 'canvas' | 'node';

export interface MenuContext {
  kind: MenuKind;
  selectedCount: number;
  clipboardCount: number;
  canUndo: boolean;
  canRedo: boolean;
  undoLabel: string | null;
  redoLabel: string | null;
  hasGroup: boolean;
  /** A closed vector is selected: Select Vectors Inside applies. */
  hasClosedSelection?: boolean;
  /** A bitmap is selected: the picture tools apply. */
  hasBitmap?: boolean;
  /** Closed vectors are selected along with the bitmap: Crop Bitmap applies. */
  canCropBitmap?: boolean;
  /** Text set on a curve is selected: Remove Text From The Curve applies. */
  hasCurveText?: boolean;
  /** One text object is selected: its spacing and arc can be adjusted. */
  hasText?: boolean;
  /** Node editing: what the pointer was over when the menu opened. */
  nodeHover?: 'node' | 'span' | null;
}

export type MenuAction = { command: AppCommand } | { toolKey: string };

export type MenuItem = PopupItem<MenuAction>;

export type MenuEntry = PopupEntry<MenuAction>;

const m = strings.contextMenu;

/** The icons of the node editing items, by item id. */
const NODE_ICONS: Readonly<Record<string, string>> = {
  smooth: 'nodeSmooth',
  start: 'nodeStart',
  alignX: 'nodeAlignX',
  alignY: 'nodeAlignY',
  properties: 'properties',
  reverse: 'reverseDirection',
  mirrorH: 'nodeMirrorH',
  mirrorV: 'nodeMirrorV',
  tangency: 'tangency',
  insert: 'nodeInsert',
  midpoint: 'nodeMidpoint',
  toLine: 'spanLine',
  toArc: 'drawArc',
  toBezier: 'spanBezier',
};

/** Items that run a command show that command's menu icon, node editing
 * items theirs, unless they name their own. */
function withIcons(entries: MenuEntry[]): MenuEntry[] {
  return entries.map((entry) => {
    if (isSeparator(entry) || isSubmenu(entry) || entry.icon !== undefined) {
      return entry;
    }
    const icon = 'command' in entry.action ? menuIcon(entry.action.command) : NODE_ICONS[entry.id];
    return icon === undefined ? entry : { ...entry, icon };
  });
}

/** The icons of the items a drawing form offers on right click, by item id
 * (Two Rail Sweep and Extrude and Weave rails and cross sections). */
const CONTROLLER_ICONS: Readonly<Record<string, string>> = {
  reverse: 'reverseDirection',
  makeStart: 'nodeStart',
  insertStart: 'nodeInsert',
  removeAll: 'editDelete',
  deleteSection: 'editDelete',
  smooth: 'nodeSmooth',
  allNodes: 'sectionAllNodes',
};

/** The icon of an item a drawing form offers on right click. */
export function controllerIcon(id: string): string | undefined {
  return CONTROLLER_ICONS[id];
}

export function buildMenu(context: MenuContext): MenuEntry[] {
  return withIcons(
    context.kind === 'node' ? nodeMenu(context.nodeHover ?? null) : canvasMenu(context),
  );
}

function canvasMenu(context: MenuContext): MenuEntry[] {
  const hasSelection = context.selectedCount > 0;
  return [
    {
      id: 'undo',
      label: context.undoLabel === null ? m.undo : `${m.undo} ${context.undoLabel}`,
      shortcut: keys('Ctrl+Z'),
      disabled: !context.canUndo,
      action: { command: 'edit.undo' },
    },
    {
      id: 'redo',
      label: context.redoLabel === null ? m.redo : `${m.redo} ${context.redoLabel}`,
      shortcut: keys('Ctrl+Y'),
      disabled: !context.canRedo,
      action: { command: 'edit.redo' },
    },
    { separator: true },
    {
      id: 'cut',
      label: m.cut,
      shortcut: keys('Ctrl+X'),
      disabled: !hasSelection,
      action: { command: 'edit.cut' },
    },
    {
      id: 'copy',
      label: m.copy,
      shortcut: keys('Ctrl+C'),
      disabled: !hasSelection,
      action: { command: 'edit.copy' },
    },
    {
      id: 'paste',
      label: m.paste,
      shortcut: keys('Ctrl+V'),
      disabled: context.clipboardCount === 0,
      action: { command: 'edit.paste' },
    },
    {
      id: 'duplicate',
      label: m.duplicate,
      disabled: !hasSelection,
      action: { command: 'edit.duplicate' },
    },
    {
      id: 'delete',
      label: m.delete,
      shortcut: 'Del',
      disabled: !hasSelection,
      action: { command: 'edit.delete' },
    },
    { separator: true },
    {
      id: 'selectAll',
      label: m.selectAll,
      shortcut: keys('Ctrl+A'),
      action: { command: 'edit.selectAll' },
    },
    {
      id: 'group',
      label: m.group,
      shortcut: 'G',
      disabled: context.selectedCount < 2,
      action: { command: 'edit.group' },
    },
    {
      id: 'ungroup',
      label: m.ungroup,
      shortcut: 'U',
      disabled: !context.hasGroup,
      action: { command: 'edit.ungroup' },
    },
    {
      id: 'selectInside',
      label: m.selectInside,
      disabled: context.hasClosedSelection !== true,
      action: { command: 'select.insideBoundary' },
    },
    ...(context.hasBitmap === true
      ? ([
          { separator: true },
          { id: 'editPicture', label: m.editPicture, action: { command: 'bitmap.editPicture' } },
          {
            id: 'cropBitmap',
            label: m.cropBitmap,
            disabled: context.canCropBitmap !== true,
            action: { command: 'bitmap.crop' },
          },
        ] satisfies MenuEntry[])
      : []),
    {
      id: 'xor',
      label: strings.menus.exclusiveOr,
      disabled: context.selectedCount < 2,
      action: { command: 'edit.xor' },
    },
    ...(context.hasText === true || context.hasCurveText === true
      ? ([{ separator: true }] satisfies MenuEntry[])
      : []),
    ...(context.hasText === true
      ? ([
          {
            id: 'adjustText',
            label: m.adjustText,
            action: { command: 'text.adjust' },
          },
        ] satisfies MenuEntry[])
      : []),
    ...(context.hasCurveText === true
      ? ([
          {
            id: 'removeFromCurve',
            label: m.removeFromCurve,
            action: { command: 'text.removeFromCurve' },
          },
        ] satisfies MenuEntry[])
      : []),
    { separator: true },
    {
      id: 'nodeEdit',
      label: m.nodeEdit,
      shortcut: 'N',
      disabled: context.selectedCount !== 1,
      action: { command: 'tool.nodeToggle' },
    },
    {
      id: 'zoomSelection',
      label: m.zoomToSelection,
      shortcut: keys('Ctrl+F'),
      disabled: !hasSelection,
      action: { command: 'view.zoomToSelection' },
    },
    { id: 'zoomJob', label: m.zoomToJob, shortcut: 'F6', action: { command: 'view.zoomToJob' } },
    { separator: true },
    {
      id: 'saveViewImage',
      label: strings.menus.saveViewImage,
      action: { command: 'file.saveViewImage' },
    },
  ];
}

/** The node editing menus: one over a node, one over a span, and the
 * vector wide entries alone elsewhere. */
function nodeMenu(hover: 'node' | 'span' | null): MenuEntry[] {
  const vector: MenuEntry[] = [
    { id: 'reverse', label: m.reverseVector, action: { toolKey: 'reverse' } },
    { id: 'close', label: m.closeVector, icon: 'close', action: { toolKey: 'close' } },
  ];
  const modes: MenuEntry[] = [
    { id: 'mirrorH', label: m.mirrorNodesH, shortcut: 'H', action: { toolKey: 'h' } },
    { id: 'mirrorV', label: m.mirrorNodesV, shortcut: 'V', action: { toolKey: 'v' } },
    { id: 'tangency', label: m.keepTangency, shortcut: 'K', action: { toolKey: 'k' } },
  ];
  const exit: MenuEntry = {
    id: 'exit',
    label: m.exitNodeMode,
    shortcut: 'N',
    action: { command: 'tool.select' },
  };
  if (hover === 'node') {
    return [
      {
        id: 'delete',
        label: m.deletePoint,
        shortcut: 'D',
        icon: 'editDelete',
        action: { toolKey: 'd' },
      },
      { id: 'smooth', label: m.smoothPoint, shortcut: 'S', action: { toolKey: 's' } },
      {
        id: 'cut',
        label: m.cutVector,
        shortcut: 'C',
        icon: 'editCut',
        action: { toolKey: 'c' },
      },
      { id: 'start', label: m.makeStart, shortcut: 'P', action: { toolKey: 'p' } },
      {
        id: 'join',
        label: m.joinVectors,
        shortcut: 'J',
        icon: 'join',
        action: { toolKey: 'j' },
      },
      { separator: true },
      { id: 'alignX', label: m.alignNodesX, shortcut: 'X', action: { toolKey: 'x' } },
      { id: 'alignY', label: m.alignNodesY, shortcut: 'Y', action: { toolKey: 'y' } },
      { id: 'properties', label: m.nodeProperties, action: { toolKey: 'properties' } },
      { separator: true },
      ...vector,
      { separator: true },
      ...modes,
      { separator: true },
      exit,
    ];
  }
  if (hover === 'span') {
    return [
      { id: 'insert', label: m.insertPoint, shortcut: 'I', action: { toolKey: 'i' } },
      { id: 'midpoint', label: m.insertMidpoint, action: { toolKey: 'insert-midpoint' } },
      {
        id: 'deleteSpan',
        label: m.deleteSpan,
        shortcut: 'D',
        icon: 'editDelete',
        action: { toolKey: 'd' },
      },
      { separator: true },
      { id: 'toLine', label: m.toLine, shortcut: 'L', action: { toolKey: 'l' } },
      { id: 'toArc', label: m.toArc, shortcut: 'A', action: { toolKey: 'a' } },
      { id: 'toBezier', label: m.toBezier, shortcut: 'B', action: { toolKey: 'b' } },
      { separator: true },
      ...vector,
      { separator: true },
      ...modes,
      { separator: true },
      exit,
    ];
  }
  return [...vector, { separator: true }, ...modes, { separator: true }, exit];
}
