// Right click menus of the toolpath list: a toolpath row, a group row and the
// empty space below the rows. Pure so the contents can be tested.

import type { ToolpathGroup, ToolpathSummary } from '../../bridge/cam';
import { strings } from '../../lib/strings';
import { withIcons, type PopupEntry } from '../../views/PopupMenu/model';

/** The icons of the list menus, by item id: the Toolpaths tab icons where one runs the same. */
const ICONS: Readonly<Record<string, string>> = {
  visible: 'visible',
  edit: 'editToolpath',
  rename: 'rename',
  duplicate: 'duplicateToolpath',
  recalculate: 'recalculateAll',
  recalculateAll: 'recalculateAll',
  recalculateGroup: 'recalculateAll',
  showGroup: 'visible',
  createGroup: 'group',
  ungroup: 'ungroup',
  save: 'saveToolpaths',
  delete: 'deleteToolpath',
  deleteAll: 'deleteToolpath',
  deleteGroup: 'deleteToolpath',
  showOnly: 'showOnly',
  showAllBut: 'showAllBut',
  showSameTool: 'showSameTool',
  showAll: 'showAll',
  hideAll: 'hideAll',
  activateSheet: 'sheetActive',
  recalculateVisible: 'recalculateVisible',
  groupVisible: 'groupVisible',
  deleteInvisible: 'deleteHidden',
  deleteVisible: 'deleteVisible',
  hideGroup: 'hidden',
};

export type ToolpathAction =
  | 'showThis'
  | 'hideThis'
  | 'showOnly'
  | 'showAllBut'
  | 'showSameTool'
  | 'showAll'
  | 'hideAll'
  | 'activateSheet'
  | 'edit'
  | 'rename'
  | 'duplicate'
  | 'recalculate'
  | 'recalculateVisible'
  | 'recalculateAll'
  | 'createGroup'
  | 'groupVisible'
  | 'ungroup'
  | 'save'
  | 'delete'
  | 'deleteInvisible'
  | 'deleteVisible'
  | 'deleteAll'
  | { moveToSheet: number };

export interface SheetChoice {
  id: number;
  name: string;
}

export interface ToolpathMenuContext {
  toolpath: ToolpathSummary;
  /** Whether this build has a form that edits this kind of toolpath. */
  canEdit: boolean;
  /** The active sheet. */
  activeSheet: number;
  /** Every sheet, for moving the toolpath to another. */
  sheets: readonly SheetChoice[];
}

const m = strings.panelMenus;
const t = strings.toolpathShell.list;

export function buildToolpathMenu({
  toolpath,
  canEdit,
  activeSheet,
  sheets,
}: ToolpathMenuContext): PopupEntry<ToolpathAction>[] {
  const sheet = toolpath.sheet ?? 0;
  const others = sheets.filter((s) => s.id !== sheet);
  return withIcons<ToolpathAction>(
    [
      toolpath.visible
        ? { id: 'visible', label: t.hideThis, action: 'hideThis' }
        : { id: 'visible', label: t.showThis, action: 'showThis' },
      { id: 'showOnly', label: t.showOnly, action: 'showOnly' },
      { id: 'showAllBut', label: t.showAllBut, action: 'showAllBut' },
      { id: 'showSameTool', label: t.showSameTool, action: 'showSameTool' },
      { id: 'showAll', label: t.showAll, action: 'showAll' },
      { id: 'hideAll', label: t.hideAll, action: 'hideAll' },
      { separator: true },
      {
        id: 'activateSheet',
        label: t.activateSheet,
        disabled: sheet === 0 || sheet === activeSheet,
        action: 'activateSheet',
      },
      { id: 'edit', label: m.edit, disabled: !canEdit, action: 'edit' },
      { id: 'rename', label: m.rename, action: 'rename' },
      { id: 'duplicate', label: m.duplicate, action: 'duplicate' },
      { separator: true },
      { id: 'recalculate', label: m.recalculate, action: 'recalculate' },
      { id: 'recalculateVisible', label: t.recalculateVisible, action: 'recalculateVisible' },
      { id: 'recalculateAll', label: t.recalculateEvery, action: 'recalculateAll' },
      { separator: true },
      { id: 'createGroup', label: t.createGroup, action: 'createGroup' },
      { id: 'groupVisible', label: t.groupVisible, action: 'groupVisible' },
      {
        id: 'ungroup',
        label: t.ungroup,
        disabled: (toolpath.group ?? 0) === 0,
        action: 'ungroup',
      },
      ...(others.length > 0 ? [{ separator: true } as const] : []),
      ...others.map((s) => ({
        id: `sheet-${s.id}`,
        label: t.moveToSheet(s.name),
        icon: 'moveToSheet',
        action: { moveToSheet: s.id },
      })),
      { separator: true },
      { id: 'save', label: m.saveToolpath, action: 'save' },
      { separator: true },
      { id: 'delete', label: t.deleteThis, action: 'delete' },
      { id: 'deleteInvisible', label: t.deleteInvisible, action: 'deleteInvisible' },
      { id: 'deleteVisible', label: t.deleteVisible, action: 'deleteVisible' },
      { id: 'deleteAll', label: t.deleteAll, action: 'deleteAll' },
    ],
    ICONS,
  );
}

export type GroupAction =
  | 'rename'
  | 'expand'
  | 'showGroup'
  | 'hideGroup'
  | 'recalculateGroup'
  | 'ungroup'
  | 'deleteGroup'
  | 'createGroup'
  | 'groupVisible';

export function buildGroupMenu(group: ToolpathGroup, members: number): PopupEntry<GroupAction>[] {
  return withIcons<GroupAction>(
    [
      { id: 'rename', label: m.rename, action: 'rename' },
      {
        id: 'expand',
        label: group.expanded ? t.collapse : t.expand,
        icon: group.expanded ? 'treeCollapse' : 'treeExpand',
        action: 'expand',
      },
      { separator: true },
      { id: 'showGroup', label: t.showGroup, disabled: members === 0, action: 'showGroup' },
      { id: 'hideGroup', label: t.hideGroup, disabled: members === 0, action: 'hideGroup' },
      {
        id: 'recalculateGroup',
        label: t.recalculateGroup,
        disabled: members === 0,
        action: 'recalculateGroup',
      },
      { separator: true },
      { id: 'createGroup', label: t.createGroup, action: 'createGroup' },
      { id: 'groupVisible', label: t.groupVisible, action: 'groupVisible' },
      { id: 'ungroup', label: t.ungroup, action: 'ungroup' },
      { id: 'deleteGroup', label: t.deleteGroup, action: 'deleteGroup' },
    ],
    ICONS,
  );
}

export type ListAction = 'createGroup' | 'groupVisible';

/** The menu of the empty space below the rows. */
export function buildListMenu(anyVisible: boolean): PopupEntry<ListAction>[] {
  return withIcons<ListAction>(
    [
      { id: 'createGroup', label: t.createGroup, action: 'createGroup' },
      { id: 'groupVisible', label: t.groupVisible, disabled: !anyVisible, action: 'groupVisible' },
    ],
    ICONS,
  );
}
