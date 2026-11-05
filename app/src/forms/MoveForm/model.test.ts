import { describe, expect, it } from 'vitest';

import { applyMatrix } from '../../lib/geometry';
import {
  initialMoveState,
  moveMatrixFromState,
  moveStateFromSelection,
  switchMoveMode,
  validateMove,
} from './model';

const box: [number, number, number, number] = [10, 10, 30, 20];
const frame = { angle: 0, extent: box };

describe('move form model', () => {
  it('fills the anchor position from the selection', () => {
    const state = moveStateFromSelection(initialMoveState('mm'), 'mm', frame);
    expect(state.x).toBe('20');
    expect(state.y).toBe('15');
  });

  it('moves the anchor to the typed position', () => {
    const state = { ...initialMoveState('mm'), x: '0', y: '0' };
    const m = moveMatrixFromState(state, 'mm', frame);
    expect(m === null ? null : applyMatrix(m, { x: 20, y: 15 })).toEqual({ x: 0, y: 0 });
  });

  it('switches to relative with zero offsets and back', () => {
    const relative = switchMoveMode(initialMoveState('mm'), 'mm', 'relative', frame);
    expect(relative.x).toBe('0');
    const m = moveMatrixFromState({ ...relative, x: '5', y: '-2' }, 'mm', frame);
    expect(m === null ? null : applyMatrix(m, { x: 0, y: 0 })).toEqual({ x: 5, y: -2 });
    const absolute = switchMoveMode(relative, 'mm', 'absolute', frame);
    expect(absolute.x).toBe('20');
  });

  it('places a rotated selection by its rotated anchor', () => {
    // Frame box 20 x 10 at the origin, turned 90 degrees: its frame bottom
    // right corner (20, 0) sits at job (0, 20).
    const rotated = { angle: 90, extent: [0, 0, 20, 10] as [number, number, number, number] };
    const state = moveStateFromSelection(
      { ...initialMoveState('mm'), anchor: 'bottomRight' },
      'mm',
      rotated,
    );
    expect(Math.abs(Number(state.x))).toBe(0);
    expect(state.y).toBe('20');
  });

  it('rejects bad numbers', () => {
    expect(validateMove({ ...initialMoveState('mm'), x: 'q' }, 'mm').x).toBeDefined();
  });
});
