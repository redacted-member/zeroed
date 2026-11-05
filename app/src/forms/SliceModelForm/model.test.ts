import { describe, expect, it } from 'vitest';

import {
  addHeightAt,
  applyHeight,
  changeSlicing,
  convertSliceModelUnits,
  cutHeights,
  deleteHeight,
  editingTop,
  heightAtBar,
  heightBands,
  listHeights,
  selectHeight,
  shownHeights,
  sliceBand,
  viewHeightEdit,
  initialSliceModelState,
  sliceModelArgs,
  sliceFacts,
  validateSliceModel,
} from './model';

describe('slice model form', () => {
  it('slices the whole model at one thickness by default', () => {
    const state = { ...initialSliceModelState('mm'), thickness: '5' };
    expect(sliceModelArgs(state, 'mm')).toEqual({ thicknesses: [5] });
  });

  it('slices the composite model only, with no source, list or name', () => {
    const state = initialSliceModelState('mm');
    expect(Object.keys(state)).not.toContain('scope');
    expect(Object.keys(state)).not.toContain('name');
    expect(Object.keys(state)).not.toContain('thicknesses');
    expect(sliceModelArgs(state, 'mm')).toEqual({ thicknesses: [12] });
  });

  it('refuses a thickness of zero', () => {
    const state = { ...initialSliceModelState('mm'), thickness: '0' };
    expect(validateSliceModel(state, 'mm').thickness).toBeDefined();
    expect(sliceModelArgs(state, 'mm')).toBeNull();
  });

  it('converts both fields when the unit changes', () => {
    const state = { ...initialSliceModelState('mm'), thickness: '25.4', heightText: '12.7' };
    const inch = convertSliceModelUnits(state, 'mm', 'inch');
    expect(inch.thickness).toBe('1');
    expect(inch.heightText).toBe('0.5');
  });

  it('reads out the number of slices and the part slice left at the top', () => {
    const slabs = [
      { index: 1, bottom: 0, top: 4 },
      { index: 2, bottom: 4, top: 8 },
    ];
    expect(sliceFacts(slabs, 6)).toEqual({ count: 2, sliceThickness: 4, partThickness: 2 });
    // A whole number of slices leaves no part slice.
    expect(sliceFacts(slabs, 8)).toEqual({ count: 2, sliceThickness: 4, partThickness: 0 });
  });

  it('slices from the top when asked', () => {
    const state = { ...initialSliceModelState('mm'), thickness: '4', fromTop: true };
    expect(sliceModelArgs(state, 'mm')).toEqual({ thicknesses: [4], fromTop: true });
  });

  it('slices into a number of equal slices', () => {
    const state = { ...initialSliceModelState('mm'), mode: 'count' as const, count: '3' };
    expect(sliceModelArgs(state, 'mm')).toEqual({ thicknesses: [], count: 3 });
    // From Top belongs to Slice Thickness only.
    expect(sliceModelArgs({ ...state, fromTop: true }, 'mm')).toEqual({
      thicknesses: [],
      count: 3,
    });
    for (const count of ['0', '2.5', 'x', '201']) {
      expect(validateSliceModel({ ...state, count }, 'mm').count).toBeDefined();
      expect(sliceModelArgs({ ...state, count }, 'mm')).toBeNull();
    }
  });

  it('asks for boundary vectors when ticked', () => {
    const state = { ...initialSliceModelState('mm'), thickness: '5', boundaryVectors: true };
    expect(sliceModelArgs(state, 'mm')).toEqual({ thicknesses: [5], boundaryVectors: true });
  });
});

describe('slice heights list', () => {
  // A model 20 mm high planned as slices of 6: cuts at 6, 12 and 18.
  const slabs = [
    { index: 1, bottom: 0, top: 6 },
    { index: 2, bottom: 6, top: 12 },
    { index: 3, bottom: 12, top: 18 },
    { index: 4, bottom: 18, top: 24 },
  ];
  const start = initialSliceModelState('mm');

  it('follows the options until it is edited', () => {
    expect(cutHeights(slabs)).toEqual([6, 12, 18]);
    expect(shownHeights(start, slabs)).toEqual([6, 12, 18]);
    expect(sliceModelArgs(start, 'mm')?.heights).toBeUndefined();
  });

  it('edits the picked height with Apply and sends the list', () => {
    const shown = shownHeights(start, slabs);
    const picked = selectHeight(start, shown, 1, 'mm');
    expect(picked.activeHeight).toBe(1);
    expect(picked.heightText).toBe('12');
    const moved = applyHeight({ ...picked, heightText: '3' }, shown, 20, 'mm');
    expect(moved.ok).toBe(true);
    if (!moved.ok) {
      return;
    }
    expect(moved.state.heights).toEqual([3, 6, 18]);
    expect(moved.state.activeHeight).toBeNull();
    expect(sliceModelArgs(moved.state, 'mm')).toEqual({
      thicknesses: [12],
      heights: [3, 6, 18],
    });
  });

  it('adds a typed height when none is picked, and refuses bad ones', () => {
    const shown = [6, 12, 18];
    const added = applyHeight({ ...start, heightText: '15' }, shown, 20, 'mm');
    expect(added.ok && added.state.heights).toEqual([6, 12, 15, 18]);
    for (const text of ['0', '20', '25', 'x', '-1']) {
      expect(applyHeight({ ...start, heightText: text }, shown, 20, 'mm').ok).toBe(false);
    }
    const same = applyHeight({ ...start, heightText: '12' }, shown, 20, 'mm');
    expect(same.ok).toBe(false);
    // Moving a height onto its own place is fine.
    const self = applyHeight({ ...start, activeHeight: 1, heightText: '12' }, shown, 20, 'mm');
    expect(self.ok && self.state.heights).toEqual([6, 12, 18]);
    // Inches are typed in inches.
    const inch = applyHeight({ ...start, heightText: '0.5' }, shown, 20, 'inch');
    expect(inch.ok && inch.state.heights).toEqual([6, 12, 12.7, 18]);
  });

  it('deletes the picked height, joining the slices either side', () => {
    const shown = [6, 12, 18];
    expect(deleteHeight(start, shown)).toBe(start);
    const gone = deleteHeight({ ...start, activeHeight: 0 }, shown);
    expect(gone.heights).toEqual([12, 18]);
    expect(gone.activeHeight).toBeNull();
    // Every height deleted: one slice of the whole model.
    const none = deleteHeight({ ...start, heights: [6], activeHeight: 0 }, [6]);
    expect(sliceModelArgs(none, 'mm')?.heights).toEqual([]);
  });

  it('rebuilds the list when an option changes', () => {
    const edited = { ...start, heights: [3], activeHeight: 0 };
    const changed = changeSlicing(edited, { thickness: '4' });
    expect(changed.heights).toBeNull();
    expect(changed.activeHeight).toBeNull();
    expect(changed.thickness).toBe('4');
  });

  it('adds a height where the bar is double clicked and draws the bands', () => {
    expect(heightAtBar(0.25, 20)).toBe(5);
    expect(heightAtBar(2, 20)).toBe(20);
    const added = addHeightAt(start, [6, 12], heightAtBar(0.25, 20), 20, 'mm');
    expect(added.ok && added.state.heights).toEqual([5, 6, 12]);
    expect(added.ok && added.state.heightText).toBe('5');
    const rounded = addHeightAt(start, [], 18.01374, 20, 'mm');
    expect(rounded.ok && rounded.state.heights).toEqual([18.014]);
    expect(heightBands([5, 10], 20)).toEqual([
      { index: 1, from: 0, to: 0.25 },
      { index: 2, from: 0.25, to: 0.5 },
      { index: 3, from: 0.5, to: 1 },
    ]);
    expect(heightBands([5], 0)).toEqual([]);
  });

  it('keeps the typed height when the unit changes', () => {
    const state = { ...start, heightText: '25.4' };
    expect(convertSliceModelUnits(state, 'mm', 'inch').heightText).toBe('1');
  });
});

describe('slice heights with the model top', () => {
  const shown = [6, 12, 18];
  const start = { ...initialSliceModelState('mm'), heights: shown };

  it('lists the model top after the slice heights, fixed', () => {
    expect(listHeights(shown, 20)).toEqual([6, 12, 18, 20]);
    expect(listHeights([], 0)).toEqual([]);
    const top = selectHeight(start, shown, 3, 'mm', 20);
    expect(top.activeHeight).toBe(3);
    expect(top.heightText).toBe('20');
    expect(editingTop(top, shown)).toBe(true);
    expect(editingTop(selectHeight(start, shown, 2, 'mm', 20), shown)).toBe(false);
    // Apply does not move it or add a height, and Delete leaves it.
    const applied = applyHeight({ ...top, heightText: '15' }, shown, 20, 'mm');
    expect(applied.ok).toBe(false);
    expect(deleteHeight(top, shown)).toBe(top);
    // Past the top there is nothing to pick.
    expect(selectHeight(start, shown, 4, 'mm', 20).activeHeight).toBeNull();
  });
});

describe('slice heights edited in the 3D view', () => {
  const shown = [6, 12, 18];
  const start = { ...initialSliceModelState('mm'), heights: shown };
  const picked = { ...start, activeHeight: 1, heightText: '12' };

  it('moves the picked height to a double click and keeps it picked', () => {
    const moved = viewHeightEdit(picked, shown, { kind: 'move', z: 16.12345 }, 20, 'mm');
    expect(moved.ok).toBe(true);
    if (!moved.ok) {
      return;
    }
    expect(moved.state.heights).toEqual([6, 16.123, 18]);
    expect(moved.state.activeHeight).toBe(1);
    expect(moved.state.heightText).toBe('16.123');
    // Past a neighbour it is re-sorted and still picked.
    const past = viewHeightEdit(picked, shown, { kind: 'move', z: 19 }, 20, 'mm');
    expect(past.ok && past.state.heights).toEqual([6, 18, 19]);
    expect(past.ok && past.state.activeHeight).toBe(2);
  });

  it('fills the field with New height picked, and refuses the model top', () => {
    const fresh = viewHeightEdit(start, shown, { kind: 'move', z: 9.5 }, 20, 'mm');
    expect(fresh.ok && fresh.state.heights).toEqual(shown);
    expect(fresh.ok && fresh.state.heightText).toBe('9.5');
    const top = { ...start, activeHeight: 3 };
    expect(viewHeightEdit(top, shown, { kind: 'move', z: 9 }, 20, 'mm').ok).toBe(false);
    expect(viewHeightEdit(picked, shown, { kind: 'move', z: 25 }, 20, 'mm').ok).toBe(false);
    expect(viewHeightEdit(picked, shown, { kind: 'move', z: 6 }, 20, 'mm').ok).toBe(false);
  });

  it('inserts a height with Shift + double click and picks it', () => {
    const added = viewHeightEdit(start, shown, { kind: 'insert', z: 3 }, 20, 'mm');
    expect(added.ok && added.state.heights).toEqual([3, 6, 12, 18]);
    expect(added.ok && added.state.activeHeight).toBe(0);
    expect(viewHeightEdit(start, shown, { kind: 'insert', z: 12 }, 20, 'mm').ok).toBe(false);
  });

  it('nudges the picked height a notch with Shift + wheel, short of its neighbours', () => {
    const up = viewHeightEdit(picked, shown, { kind: 'nudge', steps: 1 }, 20, 'mm');
    expect(up.ok && up.state.heights).toEqual([6, 12.1, 18]);
    expect(up.ok && up.state.activeHeight).toBe(1);
    const down = viewHeightEdit(picked, shown, { kind: 'nudge', steps: -2 }, 20, 'mm');
    expect(down.ok && down.state.heights).toEqual([6, 11.8, 18]);
    // A notch in an inch job is 0.005 in.
    const inch = { ...picked, heightText: '0.4724' };
    const nudged = viewHeightEdit(inch, shown, { kind: 'nudge', steps: 1 }, 20, 'inch');
    expect(nudged.ok && nudged.state.heightText).toBe('0.4774');
    // It stops a notch short of the height above.
    const near = { ...picked, heights: [6, 12, 12.15] };
    const stopped = viewHeightEdit(near, [6, 12, 12.15], { kind: 'nudge', steps: 5 }, 20, 'mm');
    expect(stopped.ok && stopped.state.heights).toEqual([6, 12.05, 12.15]);
    // Nothing picked, or the top picked: nothing moves.
    expect(viewHeightEdit(start, shown, { kind: 'nudge', steps: 1 }, 20, 'mm')).toEqual({
      ok: true,
      state: start,
    });
    const top = { ...start, activeHeight: 3 };
    expect(viewHeightEdit(top, shown, { kind: 'nudge', steps: 1 }, 20, 'mm')).toEqual({
      ok: true,
      state: top,
    });
  });

  it('colours the picked slice from the height below it', () => {
    expect(sliceBand(start, shown, 20)).toBeNull();
    expect(sliceBand(picked, shown, 20)).toEqual({ low: 6, high: 12 });
    expect(sliceBand({ ...start, activeHeight: 0 }, shown, 20)).toEqual({ low: 0, high: 6 });
    expect(sliceBand({ ...start, activeHeight: 3 }, shown, 20)).toEqual({ low: 18, high: 20 });
    expect(sliceBand(picked, shown, 0)).toBeNull();
  });
});
