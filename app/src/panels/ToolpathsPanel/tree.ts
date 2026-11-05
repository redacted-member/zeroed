// The toolpath list as a tree: toolpaths in list (cutting) order, with each
// group's members under a group row where its first member is and empty
// groups at the end. A sheet filter picks what shows; with All Sheets the
// toolpaths of sheets other than the active one show faded.

import type { ToolpathGroup, ToolpathSummary } from '../../bridge/cam';
import type { Side } from '../../bridge/types';

/** A sheet id, or every sheet. */
export type SheetFilter = number | 'all';

/** The filter the list starts with: null follows the active sheet. */
export function initialSheetFilter(mode: 'activeSheet' | 'allSheets'): SheetFilter | null {
  return mode === 'allSheets' ? 'all' : null;
}

/** The sheet to make active before editing a toolpath, or null to leave
 * the sheets as they are. */
export function sheetForEdit(
  toolpath: { sheet?: number },
  activeSheet: number,
  selectOnEdit: boolean,
): number | null {
  const sheet = toolpath.sheet ?? 0;
  return selectOnEdit && sheet !== 0 && sheet !== activeSheet ? sheet : null;
}

export interface TreeToolpath {
  kind: 'toolpath';
  toolpath: ToolpathSummary;
  /** In a group, so drawn indented. */
  inGroup: boolean;
  faded: boolean;
}

export interface TreeGroup {
  kind: 'group';
  group: ToolpathGroup;
  members: TreeToolpath[];
  faded: boolean;
}

export type TreeNode = TreeToolpath | TreeGroup;

/** How a toolpath stands: calculated, needing recalculation, or cutting nothing. */
export type ToolpathStatus = 'ok' | 'stale' | 'empty';

function onSheetOf(sheet: number | undefined, filter: SheetFilter): boolean {
  return filter === 'all' || sheet === undefined || sheet === 0 || sheet === filter;
}

function fadedOn(sheet: number | undefined, filter: SheetFilter, active: number): boolean {
  return filter === 'all' && sheet !== undefined && sheet !== 0 && sheet !== active;
}

export function buildTree(
  toolpaths: readonly ToolpathSummary[],
  groups: readonly ToolpathGroup[],
  filter: SheetFilter,
  activeSheet: number,
): TreeNode[] {
  const shownGroups = new Map(
    groups.filter((g) => onSheetOf(g.sheet, filter)).map((g) => [g.id, g] as const),
  );
  const nodes: TreeNode[] = [];
  const placed = new Map<number, TreeGroup>();
  for (const toolpath of toolpaths) {
    if (!onSheetOf(toolpath.sheet, filter)) {
      continue;
    }
    const faded = fadedOn(toolpath.sheet, filter, activeSheet);
    const group = shownGroups.get(toolpath.group ?? 0);
    if (group === undefined) {
      nodes.push({ kind: 'toolpath', toolpath, inGroup: false, faded });
      continue;
    }
    let node = placed.get(group.id);
    if (node === undefined) {
      node = {
        kind: 'group',
        group,
        members: [],
        faded: fadedOn(group.sheet, filter, activeSheet),
      };
      placed.set(group.id, node);
      nodes.push(node);
    }
    node.members.push({ kind: 'toolpath', toolpath, inGroup: true, faded });
  }
  for (const group of shownGroups.values()) {
    if (!placed.has(group.id)) {
      nodes.push({
        kind: 'group',
        group,
        members: [],
        faded: fadedOn(group.sheet, filter, activeSheet),
      });
    }
  }
  return nodes;
}

/** Every toolpath the tree shows, in order. */
export function shownToolpaths(nodes: readonly TreeNode[]): ToolpathSummary[] {
  return nodes.flatMap((n) =>
    n.kind === 'toolpath' ? [n.toolpath] : n.members.map((m) => m.toolpath),
  );
}

/** Whether all, none or some of a group's toolpaths are visible. */
export function groupVisibility(group: TreeGroup): 'all' | 'none' | 'some' {
  const shown = group.members.filter((m) => m.toolpath.visible).length;
  if (shown === 0) {
    return 'none';
  }
  return shown === group.members.length ? 'all' : 'some';
}

export function statusOf(toolpath: ToolpathSummary): ToolpathStatus {
  if (toolpath.stale) {
    return 'stale';
  }
  return toolpath.cuts === false ? 'empty' : 'ok';
}

/**
 * Where a toolpath dragged onto `target` goes: into the target group at its
 * end, or in front of the target toolpath in that toolpath's group. Null
 * when the drop would not move it.
 */
export function dropPlace(
  dragged: number,
  target: TreeNode | null,
): { group: number; before?: number } | null {
  if (target === null) {
    return { group: 0 };
  }
  if (target.kind === 'group') {
    return { group: target.group.id };
  }
  if (target.toolpath.id === dragged) {
    return null;
  }
  return { group: target.toolpath.group ?? 0, before: target.toolpath.id };
}

/** A double sided job lists the toolpaths of the side shown only. */
export function onSide<T extends { side?: Side }>(
  toolpaths: readonly T[],
  side: Side,
  doubleSided: boolean,
): T[] {
  return doubleSided ? toolpaths.filter((t) => (t.side ?? 'top') === side) : [...toolpaths];
}
