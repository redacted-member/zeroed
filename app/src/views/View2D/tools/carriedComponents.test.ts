import { describe, expect, it } from 'vitest';
import type { ComponentSummary } from '../../../bridge/model';
import { mirrorAbout } from '../../../state/documentStore';
import { carriedBoxes, carriedKey } from './carriedComponents';

function comp(id: number, bbox: [number, number, number, number], visible = true) {
  return { id, visible, bbox } as unknown as ComponentSummary;
}

describe('components carried by a selection drag', () => {
  const list = [comp(1, [0, 0, 10, 10]), comp(2, [5, 5, 20, 20]), comp(3, [1, 1, 2, 2], false)];

  it('carries the boxes of the visible selected components', () => {
    expect(carriedBoxes(list, [2, 3])).toEqual([[5, 5, 20, 20]]);
    expect(carriedBoxes(list, [])).toEqual([]);
  });

  it('names the picture by the components and the model stamp', () => {
    expect(carriedKey([1, 2], 7)).toBe('1,2@7');
    expect(carriedKey([1, 2], 8)).not.toBe(carriedKey([1, 2], 7));
  });

  it('mirrors vectors and components about the middle of their joint box', () => {
    expect(mirrorAbout('h', [0, 0, 10, 4])).toEqual([-1, 0, 0, 1, 10, 0]);
    expect(mirrorAbout('v', [0, 0, 10, 4])).toEqual([1, 0, 0, -1, 0, 4]);
  });
});
