import { describe, expect, it } from 'vitest';

import type { ComponentGroup } from '../../bridge/componentTree';
import type { ComponentSummary, Level } from '../../bridge/model';
import {
  acceptsDrop,
  buildTree,
  dropTarget,
  listedIds,
  listedTree,
  rangeIds,
  stepTarget,
  takesDrop,
} from './tree';

const group = (id: number): ComponentGroup => ({
  id,
  name: `Group ${id}`,
  visible: true,
  combine: 'add',
  expanded: true,
  color: null,
});
const level = (id: number, groups: ComponentGroup[] = []): Level => ({
  id,
  name: `Level ${id}`,
  visible: true,
  combine: 'add',
  groups,
  clip: [],
});
const part = (id: number, onLevel: number, inGroup = 0): ComponentSummary => ({
  id,
  name: `Part ${id}`,
  visible: true,
  combine: 'add',
  heightScale: 1,
  level: onLevel,
  group: inGroup,
  maxHeight: 1,
});

describe('the component tree', () => {
  it('lists levels and components in combine order, groups where their first member is', () => {
    const tree = buildTree(
      [level(1, [group(9)]), level(2)],
      [part(1, 1), part(2, 1, 9), part(3, 2), part(4, 1, 9), part(5, 1)],
    );
    expect(tree.map((n) => n.level.id)).toEqual([1, 2]);
    const first = tree[0]?.items ?? [];
    expect(first.map((i) => (i.kind === 'component' ? i.component.id : `g${i.group.id}`))).toEqual([
      1,
      'g9',
      5,
    ]);
    const g = first[1];
    expect(g?.kind === 'group' ? g.members.map((m) => m.id) : []).toEqual([2, 4]);
    expect(listedIds(tree)).toEqual([1, 2, 4, 5, 3]);
  });

  it('lists the tree bottom up: the base level and first combined at the bottom', () => {
    const tree = buildTree(
      [level(1, [group(9)]), level(2)],
      [part(1, 1), part(2, 1, 9), part(3, 2), part(4, 1, 9), part(5, 1)],
    );
    const listed = listedTree(tree);
    expect(listed.map((n) => n.level.id)).toEqual([2, 1]);
    expect(listedIds(listed)).toEqual([3, 5, 4, 2, 1]);
    expect(listedIds(tree)).toEqual([1, 2, 4, 5, 3]);
  });

  it('puts unknown levels on the first level and unknown groups on their own', () => {
    const tree = buildTree([level(4), level(5)], [part(1, 0), part(2, 5, 77)]);
    expect(tree[0]?.items.map((i) => i.kind)).toEqual(['component']);
    expect(tree[1]?.items[0]?.kind).toBe('component');
  });

  it('takes Shift ranges in listed order either way round', () => {
    const tree = buildTree([level(1)], [part(1, 1), part(2, 1), part(3, 1), part(4, 1)]);
    expect(rangeIds(tree, 3, 1)).toEqual([1, 2, 3]);
    expect(rangeIds(tree, 2, 4)).toEqual([2, 3, 4]);
    expect(rangeIds(tree, 99, 2)).toEqual([2]);
  });

  it('turns drops into kernel targets', () => {
    const tree = buildTree(
      [level(1, [group(9)]), level(2)],
      [part(1, 1), part(2, 1, 9), part(3, 2)],
    );
    expect(dropTarget(tree, [3], { kind: 'level', level: 1 })).toEqual({ level: 1 });
    expect(dropTarget(tree, [3], { kind: 'group', level: 1, group: 9 })).toEqual({
      level: 1,
      group: 9,
    });
    expect(dropTarget(tree, [3], { kind: 'component', id: 1, after: false })).toEqual({
      level: 1,
      before: 1,
    });
    // Between members of a group joins the group.
    expect(dropTarget(tree, [1], { kind: 'component', id: 2, after: true })).toEqual({
      level: 1,
      group: 9,
      after: 2,
    });
    expect(dropTarget(tree, [1], { kind: 'component', id: 1, after: true })).toBeNull();
    expect(dropTarget(tree, [], { kind: 'level', level: 2 })).toBeNull();
  });

  it('does not drop a group into itself', () => {
    const tree = buildTree([level(1, [group(9)])], [part(1, 1, 9), part(2, 1, 9), part(3, 1)]);
    const g = tree[0]?.items[0];
    expect(g !== undefined && acceptsDrop(g, [1, 2])).toBe(false);
    expect(g !== undefined && acceptsDrop(g, [3])).toBe(true);
  });

  it('says where a pointer drag in the Assembly View may reorder to', () => {
    const tree = buildTree([level(1, [group(9)])], [part(1, 1, 9), part(2, 1, 9), part(3, 1)]);
    expect(takesDrop(tree, [3], { kind: 'component', id: 1, after: true })).toBe(true);
    expect(takesDrop(tree, [3], { kind: 'component', id: 3, after: true })).toBe(false);
    expect(takesDrop(tree, [1, 2], { kind: 'group', level: 1, group: 9 })).toBe(false);
    expect(takesDrop(tree, [3], { kind: 'group', level: 1, group: 9 })).toBe(true);
    expect(takesDrop(tree, [3], { kind: 'group', level: 1, group: 44 })).toBe(false);
    expect(takesDrop(tree, [3], { kind: 'level', level: 1 })).toBe(true);
  });

  it('steps a component past its neighbour, a group counting as one', () => {
    const tree = buildTree(
      [level(1, [group(9)])],
      [part(1, 1), part(2, 1, 9), part(3, 1, 9), part(4, 1)],
    );
    expect(stepTarget(tree, 4, -1)).toEqual({ level: 1, before: 2 });
    expect(stepTarget(tree, 1, 1)).toEqual({ level: 1, after: 3 });
    expect(stepTarget(tree, 1, -1)).toBeNull();
    expect(stepTarget(tree, 3, -1)).toEqual({ level: 1, group: 9, before: 2 });
    expect(stepTarget(tree, 3, 1)).toBeNull();
  });
});
