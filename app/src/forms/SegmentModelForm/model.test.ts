import { describe, expect, it, vi } from 'vitest';

import { DEFAULT_PLAN, type CutPlane } from '../../bridge/meshImport';
import { menuItems } from '../../views/PopupMenu/model';
import {
  defaultPlane,
  deleteChildren,
  freeTurn,
  isCut,
  layoutOf,
  leafIds,
  merge,
  planeFields,
  planeOf,
  segmentOfNode,
  segmentsOf,
  separate,
  split,
  togglesFreeRotation,
  treeMenu,
  turnSegment,
} from './model';

const CUT: CutPlane = { point: [0, 0, 5], angleX: 0, angleY: 0 };

/** The model cut once, then its lower half cut again. */
function twice() {
  return split(split(DEFAULT_PLAN, 'r', CUT), 'r1', { ...CUT, point: [0, 0, 2] });
}

describe('3D segmenting form', () => {
  it('cuts leaves into two children, above first', () => {
    const plan = twice();
    expect(leafIds(plan.root)).toEqual(['r0', 'r10', 'r11']);
    expect(isCut(plan, 'r')).toBe(true);
    expect(isCut(plan, 'r0')).toBe(false);
    // A node that is already cut is not cut again.
    expect(split(plan, 'r1', CUT)).toBe(plan);
  });

  it('delete children undoes the cuts below a node and forgets them', () => {
    let plan = merge(twice(), 'r0', 'r11');
    plan = turnSegment(plan, 'r10', 'x', 1);
    const back = deleteChildren(plan, 'r1');
    expect(leafIds(back.root)).toEqual(['r0', 'r1']);
    expect(back.merges).toEqual([]);
    expect(back.layouts).toEqual({});
    expect(deleteChildren(back, 'r0')).toBe(back);
  });

  it('merges leaves into one segment and separates them again', () => {
    const plan = merge(twice(), 'r11', 'r0');
    expect(segmentsOf(plan)).toEqual([
      { id: 'r0', leaves: ['r0', 'r11'] },
      { id: 'r10', leaves: ['r10'] },
    ]);
    expect(segmentOfNode(plan, 'r11')).toBe('r0');
    const three = merge(plan, 'r10', 'r0');
    expect(three.merges).toEqual([['r0', 'r10', 'r11']]);
    expect(merge(plan, 'r0', 'nope')).toBe(plan);
    // A merged leaf is not cut.
    expect(split(plan, 'r0', CUT)).toBe(plan);
    expect(separate(plan, 'r11').merges).toEqual([]);
  });

  it('turns a segment by quarters and freely', () => {
    let plan = turnSegment(DEFAULT_PLAN, 'r', 'y', -1);
    expect(layoutOf(plan, 'r').quarterY).toBe(3);
    plan = turnSegment(plan, 'r', 'y', 1);
    expect(layoutOf(plan, 'r').quarterY).toBe(0);
    plan = freeTurn(plan, 'r', { rotX: 10, rotY: 0, rotZ: 5 });
    expect(layoutOf(plan, 'r')).toMatchObject({ rotX: 10, rotZ: 5 });
  });

  it('reads the plane fields in the job units', () => {
    const plane = defaultPlane({ x: 100, y: 50 }, [20, 20, 30]);
    expect(plane.point).toEqual([100, 50, 15]);
    const fields = planeFields(plane, 'inch');
    expect(planeOf(fields, 'inch')?.point[2]).toBeCloseTo(15, 2);
    expect(planeOf({ ...fields, angleX: '120' }, 'inch')).toBeNull();
    expect(planeOf({ ...fields, z: 'x' }, 'inch')).toBeNull();
  });

  it('offers delete children on cut nodes and separate on merged leaves', () => {
    const labels = { deleteChildren: 'Delete', separate: 'Separate' };
    const plan = merge(twice(), 'r0', 'r11');
    const onRoot = menuItems(treeMenu(plan, 'r', labels));
    expect(onRoot.map((i) => i.disabled)).toEqual([false, true]);
    const onLeaf = menuItems(treeMenu(plan, 'r0', labels));
    expect(onLeaf.map((i) => i.disabled)).toEqual([true, false]);
    expect(onLeaf.map((i) => i.icon)).toEqual(['segmentDeleteChildren', 'segmentSeparate']);
  });
});

describe('free rotation key', () => {
  const key = (k: string, target: unknown = null) => ({
    key: k,
    metaKey: false,
    ctrlKey: false,
    target: target as EventTarget | null,
  });

  it('toggles on O unless a field is typed in or a modal dialog is open', () => {
    let open = false;
    vi.stubGlobal('document', {
      querySelector: (selector: string) =>
        selector.startsWith('dialog[open]') && open ? {} : null,
    });
    expect(togglesFreeRotation(key('o'))).toBe(true);
    expect(togglesFreeRotation(key('O'))).toBe(true);
    expect(togglesFreeRotation(key('p'))).toBe(false);
    expect(togglesFreeRotation(key('o', { tagName: 'INPUT' }))).toBe(false);
    expect(togglesFreeRotation({ ...key('o'), metaKey: true })).toBe(false);
    open = true;
    expect(togglesFreeRotation(key('o'))).toBe(false);
    vi.unstubAllGlobals();
  });
});
