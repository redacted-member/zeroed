import { afterEach, describe, expect, it } from 'vitest';

import { doubleClickEdit, useSliceViewStore, wheelEdit } from './sliceViewStore';

const wheel = {
  deltaX: 0,
  deltaY: -100,
  shiftKey: true,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
};

describe('slice heights in the 3D view', () => {
  afterEach(() => useSliceViewStore.setState({ active: false, band: null, request: null }));

  it('moves with a double click and inserts with Shift', () => {
    expect(doubleClickEdit(7, false)).toEqual({ kind: 'move', z: 7 });
    expect(doubleClickEdit(7, true)).toEqual({ kind: 'insert', z: 7 });
  });

  it('nudges only with Shift alone on the wheel, up raising', () => {
    expect(wheelEdit(wheel)).toEqual({ kind: 'nudge', steps: 1 });
    expect(wheelEdit({ ...wheel, deltaY: 100 })).toEqual({ kind: 'nudge', steps: -1 });
    // Shift turns the wheel sideways on some systems.
    expect(wheelEdit({ ...wheel, deltaY: 0, deltaX: 50 })).toEqual({ kind: 'nudge', steps: -1 });
    expect(wheelEdit({ ...wheel, shiftKey: false })).toBeNull();
    expect(wheelEdit({ ...wheel, ctrlKey: true })).toBeNull();
    expect(wheelEdit({ ...wheel, altKey: true })).toBeNull();
    expect(wheelEdit({ ...wheel, deltaY: 0 })).toBeNull();
  });

  it('counts requests so the same edit twice is new, and clears the band on close', () => {
    const store = useSliceViewStore.getState();
    store.setActive(true);
    store.requestEdit({ kind: 'nudge', steps: 1 });
    const first = useSliceViewStore.getState().request;
    store.requestEdit({ kind: 'nudge', steps: 1 });
    const second = useSliceViewStore.getState().request;
    expect(first?.seq).toBe(1);
    expect(second?.seq).toBe(2);
    store.setBand({ low: 1, high: 2 });
    const band = useSliceViewStore.getState().band;
    store.setBand({ low: 1, high: 2 });
    expect(useSliceViewStore.getState().band).toBe(band);
    store.setActive(false);
    expect(useSliceViewStore.getState().band).toBeNull();
  });
});
