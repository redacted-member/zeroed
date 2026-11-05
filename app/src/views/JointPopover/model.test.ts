import { describe, expect, it } from 'vitest';

import type { ConnectionRow } from '../../bridge/furniture';
import { initialJoints, withConnectionJoint } from '../../forms/FurnitureDesigner/model';
import { classOverrides, pickArgs, pickLocally, popoverPlace } from './model';

function row(key: string, cls: string, source: ConnectionRow['source'] = 'style'): ConnectionRow {
  return {
    key,
    a: 'a',
    b: 'b',
    type: 'tee',
    class: cls,
    joint: 'dado',
    params: {},
    source,
    choices: ['dado', 'dowel'],
    seam: { from: [0, 0, 0], to: [1, 0, 0] },
  };
}

const ROWS = [
  row('left~shelf#0', 'shelf'),
  row('left~shelf#1', 'shelf', 'connection'),
  row('left~top', 'carcass', 'connection'),
];

describe('pickArgs', () => {
  it('changes one connection, or its class for all like this', () => {
    const seam = ROWS[0];
    if (seam === undefined) {
      throw new Error('no row');
    }
    expect(pickArgs(7, seam, 'dowel', false)).toEqual({
      design: 7,
      scope: 'connection',
      key: 'left~shelf#0',
      joint: 'dowel',
    });
    expect(pickArgs(7, seam, 'dowel', true)).toEqual({
      design: 7,
      scope: 'class',
      key: 'shelf',
      joint: 'dowel',
    });
  });
});

describe('pickLocally', () => {
  const seam = ROWS[0];
  if (seam === undefined) {
    throw new Error('no row');
  }

  it('sets the one connection', () => {
    expect(pickLocally(initialJoints(), seam, 'dowel', false, ROWS).connections).toEqual({
      'left~shelf#0': { joint: 'dowel' },
    });
  });

  it('sets the class and clears its seams set by hand, keeping other classes', () => {
    let joints = withConnectionJoint(initialJoints(), 'left~shelf#1', 'tabSlot');
    joints = withConnectionJoint(joints, 'left~top', 'finger');
    const after = pickLocally(joints, seam, 'dowel', true, ROWS);
    expect(after.classes).toEqual({ shelf: { joint: 'dowel' } });
    expect(after.connections).toEqual({ 'left~top': { joint: 'finger' } });
  });
});

describe('classOverrides', () => {
  it('lists the class seams whose joint was set one by one', () => {
    const seam = ROWS[0];
    if (seam === undefined) {
      throw new Error('no row');
    }
    expect(classOverrides(seam, ROWS)).toEqual(['left~shelf#1']);
  });
});

describe('popoverPlace', () => {
  it('stands below right of the click when it fits', () => {
    expect(popoverPlace(100, 100, 200, 150, 800, 600)).toEqual({ left: 112, top: 112 });
  });

  it('flips left and up near the right and bottom edges', () => {
    expect(popoverPlace(700, 500, 200, 150, 800, 600)).toEqual({ left: 488, top: 338 });
  });

  it('flips at the exact edge: 1 px too wide flips, a fit stays', () => {
    // left 112 + 680 = 792 = 800 - 8 fits; 681 wide does not.
    expect(popoverPlace(100, 100, 680, 10, 800, 600).left).toBe(112);
    expect(popoverPlace(100, 100, 681, 10, 800, 600).left).toBe(8);
  });

  it('stays inside a view smaller than it', () => {
    expect(popoverPlace(10, 10, 400, 400, 300, 300)).toEqual({ left: 8, top: 8 });
  });
});
