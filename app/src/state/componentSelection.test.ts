import { describe, expect, it } from 'vitest';
import type { ComponentSummary } from '../bridge/model';
import { boxOf, componentUnder, pickAfterClick } from './componentSelection';

function comp(
  id: number,
  bbox: [number, number, number, number],
  visible = true,
): ComponentSummary {
  return { id, visible, bbox } as unknown as ComponentSummary;
}

describe('component selection in the 2D view', () => {
  const list = [comp(1, [0, 0, 10, 10]), comp(2, [5, 5, 20, 20]), comp(3, [30, 30, 40, 40], false)];

  it('picks the topmost visible component under the point', () => {
    expect(componentUnder(list, 7, 7)?.id).toBe(2);
    expect(componentUnder(list, 2, 2)?.id).toBe(1);
    expect(componentUnder(list, 35, 35)).toBeNull();
  });

  it('joins the picked boxes', () => {
    expect(boxOf(list, [1, 2])).toEqual([0, 0, 20, 20]);
    expect(boxOf(list, [])).toBeNull();
  });

  it('adds and takes away with Shift or Ctrl', () => {
    expect(pickAfterClick([1], 2, false)).toEqual([2]);
    expect(pickAfterClick([1], 2, true)).toEqual([1, 2]);
    expect(pickAfterClick([1, 2], 1, true)).toEqual([2]);
  });
});
