import { describe, expect, it } from 'vitest';

import {
  MODELING_COMBINES,
  afterApply,
  afterReset,
  afterStartNew,
  applyTarget,
  componentHeights,
  startTrack,
} from './model';

const reply = { revision: 12, id: 4, shapeHeight: 3, totalHeight: 5 };

describe('modeling tool apply', () => {
  it('offers add, subtract and merge', () => {
    expect(MODELING_COMBINES.map((c) => c.value)).toEqual(['add', 'subtract', 'mergeHigh']);
  });

  it('makes a component, then updates it and replaces the last step', () => {
    const fresh = startTrack(null);
    expect(applyTarget(fresh, undefined)).toEqual({});
    const applied = afterApply(fresh, reply);
    expect(applied).toMatchObject({ target: 4, shapeHeight: 3, totalHeight: 5 });
    expect(applyTarget(applied, 12)).toEqual({ target: 4, replace: 12 });
  });

  it('resets to nothing, or to the component it edits', () => {
    expect(afterReset(afterApply(startTrack(null), reply)).target).toBeNull();
    const editing = afterApply(startTrack(9), { ...reply, id: 9 });
    expect(afterReset(editing)).toMatchObject({ target: 9, totalHeight: null });
    expect(applyTarget(startTrack(9), undefined)).toEqual({ target: 9 });
  });

  it('shows an edited component its own shape and total height', () => {
    const shape = {
      id: 9,
      name: 'Dome',
      visible: true,
      combine: 'add' as const,
      heightScale: 2,
      level: 1,
      maxHeight: 25,
      baseHeight: 5,
    };
    const editing = startTrack(9, shape);
    expect(editing).toMatchObject({ target: 9, shapeHeight: 10, totalHeight: 25 });
    expect(afterReset(afterApply(editing, { ...reply, id: 9 }))).toMatchObject({
      shapeHeight: 10,
      totalHeight: 25,
    });
    const faded = { ...shape, fade: { from: { x: 0, y: 0 }, to: { x: 1, y: 0 }, amount: 50 } };
    expect(startTrack(9, faded).totalHeight).toBeNull();
    expect(componentHeights(undefined)).toEqual({ shapeHeight: null, totalHeight: null });
  });

  it('starts a new component after Start New Component', () => {
    expect(afterStartNew()).toEqual(startTrack(null));
  });
});
