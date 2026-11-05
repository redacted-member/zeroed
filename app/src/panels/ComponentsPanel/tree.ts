// The component tree: levels as folders and in each level its components
// and groups. `buildTree` keeps combine order (the first level is the base
// the others combine onto); the panel lists it bottom up
// does (`listedTree`), so the model builds from the bottom of the list to
// the top. Pure so the ordering and drag and drop can be tested.

import type { ComponentGroup, TreeTarget } from '../../bridge/componentTree';
import type { ComponentSummary, Level } from '../../bridge/model';

export type TreeItem =
  | { kind: 'component'; component: ComponentSummary }
  | { kind: 'group'; group: ComponentGroup; members: ComponentSummary[] };

export interface LevelNode {
  level: Level;
  items: TreeItem[];
}

/** Levels and their items in combine order. A component on no known level
 * sits on the first level; one naming a group its level does not have is
 * listed on its own. A group is listed where its first member is. */
export function buildTree(
  levels: readonly Level[],
  components: readonly ComponentSummary[],
): LevelNode[] {
  const first = levels[0]?.id;
  const known = new Set(levels.map((l) => l.id));
  return levels.map((level) => {
    const groups = level.groups ?? [];
    const own = components.filter((c) => (known.has(c.level) ? c.level : first) === level.id);
    const items: TreeItem[] = [];
    const placed = new Set<number>();
    for (const component of own) {
      const group = groups.find((g) => g.id === (component.group ?? 0));
      if (group === undefined) {
        items.push({ kind: 'component', component });
        continue;
      }
      if (placed.has(group.id)) {
        continue;
      }
      placed.add(group.id);
      items.push({
        kind: 'group',
        group,
        members: own.filter((c) => (c.group ?? 0) === group.id),
      });
    }
    return { level, items };
  });
}

/** The tree as the panel lists it: the last combined level, item and
 * group member at the top, the base at the bottom. */
export function listedTree(tree: readonly LevelNode[]): LevelNode[] {
  return [...tree].reverse().map((node) => ({
    level: node.level,
    items: [...node.items]
      .reverse()
      .map((item) =>
        item.kind === 'group' ? { ...item, members: [...item.members].reverse() } : item,
      ),
  }));
}

/** Component ids in the order the tree lists them, members of collapsed
 * groups included; used for Shift click ranges. */
export function listedIds(tree: readonly LevelNode[]): number[] {
  return tree.flatMap((node) =>
    node.items.flatMap((item) =>
      item.kind === 'component' ? [item.component.id] : item.members.map((m) => m.id),
    ),
  );
}

/** The ids from `from` to `to` inclusive in listed order. */
export function rangeIds(tree: readonly LevelNode[], from: number, to: number): number[] {
  const ids = listedIds(tree);
  const a = ids.indexOf(from);
  const b = ids.indexOf(to);
  if (a < 0 || b < 0) {
    return b < 0 ? [] : [to];
  }
  return ids.slice(Math.min(a, b), Math.max(a, b) + 1);
}

/** Where a row was dropped on. `after` means the dragged items then come
 * next in combine order: the upper half of a row in the bottom up list. */
export type DropSpot =
  | { kind: 'level'; level: number }
  | { kind: 'group'; level: number; group: number }
  | { kind: 'component'; id: number; after: boolean };

/** The kernel target for dropping `ids` on `spot`, or null when the drop
 * would do nothing (a component onto itself). A component row keeps the
 * group it is in, so dropping between members joins the group. */
export function dropTarget(
  tree: readonly LevelNode[],
  ids: readonly number[],
  spot: DropSpot,
): TreeTarget | null {
  if (ids.length === 0) {
    return null;
  }
  if (spot.kind === 'level') {
    return { level: spot.level };
  }
  if (spot.kind === 'group') {
    return { level: spot.level, group: spot.group };
  }
  if (ids.includes(spot.id)) {
    return null;
  }
  for (const node of tree) {
    for (const item of node.items) {
      const members = item.kind === 'component' ? [item.component] : item.members;
      if (!members.some((m) => m.id === spot.id)) {
        continue;
      }
      const target: TreeTarget = { level: node.level.id };
      if (item.kind === 'group') {
        target.group = item.group.id;
      }
      // Later in combine order is higher in the kernel's stack.
      if (spot.after) {
        target.after = spot.id;
      } else {
        target.before = spot.id;
      }
      return target;
    }
  }
  return null;
}

/** Whether a drag carrying `ids` may land on a group: not when it is the
 * group's own members going back into it. */
export function acceptsDrop(item: TreeItem, ids: readonly number[]): boolean {
  if (item.kind === 'component') {
    return !ids.includes(item.component.id);
  }
  return !item.members.every((m) => ids.includes(m.id));
}

/** Whether a drag carrying `ids` may land on `spot`: a component row not
 * among them, a group not made only of them, or any level. */
export function takesDrop(
  tree: readonly LevelNode[],
  ids: readonly number[],
  spot: DropSpot,
): boolean {
  if (spot.kind === 'group') {
    const item = tree
      .find((node) => node.level.id === spot.level)
      ?.items.find((i) => i.kind === 'group' && i.group.id === spot.group);
    return item !== undefined && acceptsDrop(item, ids);
  }
  return dropTarget(tree, ids, spot) !== null;
}

/** Where a component moves one step later (1, up the bottom up list) or
 * earlier (-1) in combine order: past its neighbour in its group, or in
 * its level where a whole group counts as one neighbour. Null at either
 * end. */
export function stepTarget(
  tree: readonly LevelNode[],
  id: number,
  step: -1 | 1,
): TreeTarget | null {
  for (const node of tree) {
    for (const item of node.items) {
      if (item.kind === 'group' && item.members.some((m) => m.id === id)) {
        const ids = item.members.map((m) => m.id);
        const next = ids[ids.indexOf(id) + step];
        if (next === undefined) {
          return null;
        }
        const target: TreeTarget = { level: node.level.id, group: item.group.id };
        return step < 0 ? { ...target, before: next } : { ...target, after: next };
      }
    }
    const at = node.items.findIndex((i) => i.kind === 'component' && i.component.id === id);
    if (at < 0) {
      continue;
    }
    const neighbour = node.items[at + step];
    if (neighbour === undefined) {
      return null;
    }
    const ids =
      neighbour.kind === 'component'
        ? [neighbour.component.id]
        : neighbour.members.map((m) => m.id);
    const target: TreeTarget = { level: node.level.id };
    return step < 0
      ? { ...target, before: ids[0] ?? id }
      : { ...target, after: ids[ids.length - 1] ?? id };
  }
  return null;
}
