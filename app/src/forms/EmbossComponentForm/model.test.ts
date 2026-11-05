import { describe, expect, it } from 'vitest';

import { embossArgs, embossedTop, initialEmbossComponentState, lowRelief } from './model';

describe('emboss component form model', () => {
  it('starts at a tenth of the height with no detail smoothing', () => {
    expect(initialEmbossComponentState()).toEqual({ scale: 10, detailSmoothing: 0 });
  });

  it('needs a component and keeps the percentages in range', () => {
    const state = { scale: 0, detailSmoothing: 250 };
    expect(embossArgs(state, null, undefined)).toBeNull();
    expect(embossArgs(state, 5, 12)).toEqual({
      id: 5,
      scale: 1,
      detailSmoothing: 100,
      replace: 12,
    });
    expect(embossArgs({ scale: 33.3, detailSmoothing: 10 }, 5, undefined)).toEqual({
      id: 5,
      scale: 33,
      detailSmoothing: 10,
    });
  });

  it('estimates the new top', () => {
    expect(embossedTop(40, { scale: 25, detailSmoothing: 0 })).toBe(10);
    expect(embossedTop(null, initialEmbossComponentState())).toBeNull();
  });

  it('calls a component low relief when it is under a tenth of its width high', () => {
    const part = {
      id: 1,
      name: 'Bird',
      visible: true,
      combine: 'add' as const,
      heightScale: 1,
      level: 1,
      maxHeight: 4,
      bbox: [0, 0, 100, 50] as [number, number, number, number],
    };
    expect(lowRelief(part)).toBe(true);
    expect(lowRelief({ ...part, maxHeight: 30 })).toBe(false);
    expect(lowRelief({ ...part, maxHeight: null })).toBe(false);
  });
});
