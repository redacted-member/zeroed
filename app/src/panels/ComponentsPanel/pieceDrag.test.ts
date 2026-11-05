import { describe, expect, it } from 'vitest';

import type { DropLanding } from '../../bridge/assembly';
import { pressStep, releaseAction, rowSpot, type PiecePress } from './pieceDrag';

const press = (dragging: boolean): PiecePress => ({
  pointerId: 1,
  component: 5,
  name: 'Screen',
  ids: [5, 6],
  from: { x: 10, y: 10 },
  dragging,
});

const landing: DropLanding = { point: [1, 2, 3], onto: { sheet: 1, part: 2, component: null } };

describe('dragging a tree row while the Assembly View is open', () => {
  it('becomes a drag only past a few pixels', () => {
    expect(pressStep(press(false), { x: 11, y: 11 })).toBe('wait');
    expect(pressStep(press(false), { x: 20, y: 10 })).toBe('start');
    expect(pressStep(press(true), { x: 11, y: 10 })).toBe('move');
  });

  it('lands where it is let go over a view, and cancels anywhere else', () => {
    expect(releaseAction(press(true), landing)).toEqual({
      kind: 'drop',
      component: 5,
      at: landing,
    });
    expect(releaseAction(press(true), null)).toEqual({ kind: 'cancel' });
    expect(releaseAction(press(false), landing)).toEqual({ kind: 'none' });
  });

  it('reorders what it carries when let go over the tree, and nothing where no row takes it', () => {
    const spot = { kind: 'component' as const, id: 9, after: true };
    expect(releaseAction(press(true), null, { spot })).toEqual({
      kind: 'reorder',
      ids: [5, 6],
      spot,
    });
    expect(releaseAction(press(true), landing, { spot: null })).toEqual({ kind: 'cancel' });
    expect(releaseAction(press(false), null, { spot })).toEqual({ kind: 'none' });
  });

  it('reads the spot a row stands for from its data, the upper half of a component row going later', () => {
    expect(rowSpot({ dropComponent: '4' }, 100, 20, 105)).toEqual({
      kind: 'component',
      id: 4,
      after: true,
    });
    expect(rowSpot({ dropComponent: '4' }, 100, 20, 115)).toEqual({
      kind: 'component',
      id: 4,
      after: false,
    });
    expect(rowSpot({ dropGroup: '7', dropLevel: '2' }, 0, 20, 5)).toEqual({
      kind: 'group',
      level: 2,
      group: 7,
    });
    expect(rowSpot({ dropLevel: '2' }, 0, 20, 5)).toEqual({ kind: 'level', level: 2 });
    expect(rowSpot({}, 0, 20, 5)).toBeNull();
    expect(rowSpot({ dropComponent: 'x' }, 0, 20, 5)).toBeNull();
  });
});
