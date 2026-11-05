import { describe, expect, it } from 'vitest';

import { applyMatrix } from '../../lib/geometry';
import {
  boxPivot,
  frameCentre,
  pivotAt,
  snapPivot,
  initialRotateState,
  pivotFromSelection,
  rotateMatrixFromState,
  switchRotateMode,
  validateRotate,
} from './model';

const box: [number, number, number, number] = [10, 10, 30, 20];
const frame = { angle: 0, extent: box };
const round = (v: number) => Math.round(v * 1000) / 1000;

describe('rotate form model', () => {
  it('pivots on a corner of the selection', () => {
    expect(boxPivot('topLeft', frame)).toEqual({ x: 10, y: 20 });
    expect(boxPivot('bottomRight', frame)).toEqual({ x: 30, y: 10 });
    const state = { ...initialRotateState('mm'), pivot: 'bottomLeft' as const, angle: '90' };
    expect(pivotFromSelection(state, 'mm', frame).pivotX).toBe('10');
    const m = rotateMatrixFromState(state, 'mm', frame);
    const p = m === null ? null : applyMatrix(m, { x: 30, y: 10 });
    expect(p === null ? null : [round(p.x), round(p.y)]).toEqual([10, 30]);
  });

  it('rotates about the selection centre by default', () => {
    const m = rotateMatrixFromState({ ...initialRotateState('mm'), angle: '90' }, 'mm', frame);
    const p = m === null ? null : applyMatrix(m, { x: 30, y: 15 });
    expect(p === null ? null : [round(p.x), round(p.y)]).toEqual([20, 25]);
  });

  it('rotates about a typed pivot', () => {
    const state = {
      ...initialRotateState('mm'),
      angle: '180',
      pivot: 'point' as const,
      pivotX: '0',
      pivotY: '0',
    };
    const m = rotateMatrixFromState(state, 'mm', frame);
    const p = m === null ? null : applyMatrix(m, { x: 10, y: 0 });
    expect(p?.x ?? 0).toBeCloseTo(-10, 6);
    expect(p?.y ?? 1).toBeCloseTo(0, 6);
  });

  it('sets an absolute angle from the kept rotation', () => {
    const rotated = { angle: 30, extent: box };
    const state = switchRotateMode(initialRotateState('mm'), 'absolute', rotated);
    expect(state.angle).toBe('30');
    const m = rotateMatrixFromState({ ...state, angle: '0' }, 'mm', rotated);
    // Turning back by 30 degrees about the rotated box centre.
    const centre = frameCentre(rotated);
    const p = m === null ? null : applyMatrix(m, { x: centre.x + 10, y: centre.y });
    const angle = p === null ? 0 : (Math.atan2(p.y - centre.y, p.x - centre.x) * 180) / Math.PI;
    expect(round(angle)).toBe(-30);
  });

  it('fills the pivot from the selection', () => {
    const state = pivotFromSelection(initialRotateState('mm'), 'mm', frame);
    expect(state.pivotX).toBe('20');
    expect(validateRotate({ ...state, angle: 'x' }, 'mm').angle).toBeDefined();
  });

  it('places the pivot by clicking, snapping to the box', () => {
    const frame = { angle: 0, extent: box };
    expect(snapPivot(frame, { x: 10.2, y: 9.9 }, 0.5)).toEqual({ x: 10, y: 10 });
    expect(snapPivot(frame, { x: 20.1, y: 15 }, 0.5)).toEqual({ x: 20, y: 15 });
    expect(snapPivot(frame, { x: 3, y: 3 }, 0.5)).toEqual({ x: 3, y: 3 });
    const placed = pivotAt(initialRotateState('mm'), 'mm', { x: 4, y: 5 });
    expect([placed.pivot, placed.pivotX, placed.pivotY]).toEqual(['point', '4', '5']);
  });
});
