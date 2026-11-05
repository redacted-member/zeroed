import { describe, expect, it } from 'vitest';

import { initialPasses } from '../../lib/passFields';
import {
  addPassAt,
  applyDepth,
  clearPasses,
  deletePass,
  editorResult,
  nearestPass,
  openPassEditor,
  selectPass,
  setAssociated,
  setLastPass,
} from './model';

const span = { span: 11, toolPassDepth: 5 };

describe('Edit Passes dialog', () => {
  it('opens with the passes the tool pass depth makes', () => {
    const state = openPassEditor(initialPasses('mm'), span, 'mm');
    expect(state.depths).toEqual([5.5, 11]);
    expect(state.custom).toBe(false);
  });

  it('adds, moves and deletes passes, which makes the list custom', () => {
    let state = openPassEditor(initialPasses('mm'), span, 'mm');
    state = addPassAt(state, 2, span, 'mm');
    expect(state.depths).toEqual([2, 5.5, 11]);
    expect(state.selected).toBe(0);
    expect(state.custom).toBe(true);
    state = applyDepth({ ...state, depthText: '8' }, span, 'mm');
    expect(state.depths).toEqual([5.5, 8, 11]);
    expect(state.selected).toBe(1);
    // The final pass stays on the cut depth.
    expect(applyDepth(selectPass(state, 2, 'mm'), span, 'mm').depths).toEqual([5.5, 8, 11]);
    state = deletePass(selectPass(state, 0, 'mm'), 'mm');
    expect(state.depths).toEqual([8, 11]);
    expect(clearPasses(state, span, 'mm').depths).toEqual([11]);
    expect(editorResult(state, initialPasses('mm')).list).toEqual([8]);
  });

  it('refuses a pass outside the cut or on another pass', () => {
    const state = openPassEditor(initialPasses('mm'), span, 'mm');
    expect(addPassAt(state, 12, span, 'mm')).toBe(state);
    expect(addPassAt(state, 5.5, span, 'mm')).toBe(state);
  });

  it('makes the associated passes by count or exact depth, dropping custom ones', () => {
    let state = addPassAt(openPassEditor(initialPasses('mm'), span, 'mm'), 1, span, 'mm');
    state = setAssociated({ ...state, method: 'count', countText: '4' }, span, 'mm');
    expect(state.depths).toEqual([2.75, 5.5, 8.25, 11]);
    expect(state.custom).toBe(false);
    state = setAssociated({ ...state, method: 'tool', exact: true }, span, 'mm');
    expect(state.depths).toEqual([5, 10, 11]);
    const result = editorResult(state, initialPasses('mm'));
    expect(result).toMatchObject({ byCount: false, exact: true, list: null });
  });

  it('sets the last pass by its thickness', () => {
    let state = openPassEditor(initialPasses('mm'), span, 'mm');
    state = setLastPass(state, { useLastPass: true, lastText: '1' }, span, 'mm');
    expect(state.depths).toEqual([5, 10, 11]);
    state = addPassAt(state, 3, span, 'mm');
    state = setLastPass(state, { lastText: '2' }, span, 'mm');
    expect(state.depths).toEqual([3, 5, 9, 11]);
  });

  it('picks the pass nearest a click on the diagram', () => {
    const state = openPassEditor(initialPasses('mm'), span, 'mm');
    expect(nearestPass(state, 0.49, span)).toBe(0);
    expect(nearestPass(state, 0.2, span)).toBeNull();
  });
  it('reads the Depth box in the units shown and keeps the passes in millimeters', () => {
    const opened = selectPass(openPassEditor(initialPasses('inch'), span, 'inch'), 0, 'inch');
    const state = applyDepth({ ...opened, depthText: '0.1' }, span, 'inch');
    expect(state.depths).toHaveLength(2);
    expect(state.depths[0]).toBeCloseTo(2.54, 9);
    expect(state.depths[1]).toBe(11);
    expect(state.depthText).toBe('0.1');
  });
});
