import { describe, expect, it } from 'vitest';

import type { ToolpathGroup, ToolpathSummary } from '../../bridge/cam';
import {
  buildTree,
  dropPlace,
  groupVisibility,
  initialSheetFilter,
  onSide,
  sheetForEdit,
  shownToolpaths,
  statusOf,
} from './tree';

const row = (id: number, extra: Partial<ToolpathSummary> = {}): ToolpathSummary => ({
  id,
  name: `T${id}`,
  kind: 'profile',
  visible: true,
  stale: false,
  toolName: 'End Mill 6 mm',
  toolNumber: 1,
  estimatedSeconds: 1,
  moves: 1,
  ...extra,
});

const group = (id: number, sheet = 1): ToolpathGroup => ({
  id,
  name: `G${id}`,
  sheet,
  expanded: true,
});

describe('toolpath tree', () => {
  it('puts a group where its first member is and empty groups last', () => {
    const list = [row(1), row(2, { group: 5 }), row(3, { group: 5 }), row(4)];
    const tree = buildTree(list, [group(5), group(6)], 1, 1);
    expect(tree.map((n) => (n.kind === 'group' ? `g${n.group.id}` : `t${n.toolpath.id}`))).toEqual([
      't1',
      'g5',
      't4',
      'g6',
    ]);
    const first = tree[1];
    expect(first?.kind === 'group' ? first.members.map((m) => m.toolpath.id) : []).toEqual([2, 3]);
    expect(shownToolpaths(tree).map((t) => t.id)).toEqual([1, 2, 3, 4]);
  });

  it('filters by sheet and fades other sheets with All Sheets', () => {
    const list = [row(1, { sheet: 1 }), row(2, { sheet: 2 }), row(3, { sheet: 0 }), row(4)];
    expect(shownToolpaths(buildTree(list, [], 1, 1)).map((t) => t.id)).toEqual([1, 3, 4]);
    expect(shownToolpaths(buildTree(list, [], 2, 1)).map((t) => t.id)).toEqual([2, 3, 4]);
    const all = buildTree(list, [group(9, 2)], 'all', 1);
    const faded = all.filter((n) => n.faded).map((n) => (n.kind === 'group' ? -1 : n.toolpath.id));
    expect(faded).toEqual([2, -1]);
    // A group of another sheet hides when filtering on this one.
    expect(buildTree([], [group(9, 2)], 1, 1)).toEqual([]);
  });

  it('reads group visibility and toolpath status', () => {
    const tree = buildTree(
      [row(1, { group: 5 }), row(2, { group: 5, visible: false })],
      [group(5)],
      1,
      1,
    );
    const node = tree[0];
    expect(node?.kind === 'group' ? groupVisibility(node) : null).toBe('some');
    expect(statusOf(row(1))).toBe('ok');
    expect(statusOf(row(1, { stale: true, cuts: false }))).toBe('stale');
    expect(statusOf(row(1, { cuts: false }))).toBe('empty');
  });

  it('turns a drop into a place in the list', () => {
    const tree = buildTree([row(1), row(2, { group: 5 })], [group(5)], 1, 1);
    const [top, g] = tree;
    expect(dropPlace(2, top ?? null)).toEqual({ group: 0, before: 1 });
    expect(dropPlace(1, g ?? null)).toEqual({ group: 5 });
    expect(dropPlace(1, top ?? null)).toBeNull();
    expect(dropPlace(1, null)).toEqual({ group: 0 });
  });
});

describe('toolpaths of a double sided job', () => {
  it('lists the side shown only', () => {
    const list = [{ id: 1, side: 'top' as const }, { id: 2, side: 'bottom' as const }, { id: 3 }];
    expect(onSide(list, 'top', true).map((t) => t.id)).toEqual([1, 3]);
    expect(onSide(list, 'bottom', true).map((t) => t.id)).toEqual([2]);
    expect(onSide(list, 'bottom', false)).toHaveLength(3);
  });
});

describe('sheet filter preferences', () => {
  it('starts on the active sheet or on all sheets', () => {
    expect(initialSheetFilter('activeSheet')).toBeNull();
    expect(initialSheetFilter('allSheets')).toBe('all');
  });

  it('activates another sheet before editing only when asked', () => {
    expect(sheetForEdit({ sheet: 2 }, 1, true)).toBe(2);
    expect(sheetForEdit({ sheet: 2 }, 1, false)).toBeNull();
    expect(sheetForEdit({ sheet: 1 }, 1, true)).toBeNull();
    expect(sheetForEdit({}, 1, true)).toBeNull();
  });
});
