import { describe, expect, it } from 'vitest';

import { DEFAULT_FILTER } from '../../bridge/sketch';
import {
  filterFrom,
  filterState,
  isAutomatic,
  toggleLayer,
  validateFilter,
  withSelector,
} from './model';

describe('vector selector filter', () => {
  it('starts with open and closed vectors on every visible layer', () => {
    const state = filterState(null, 'mm');
    expect(state.allLayers).toBe(true);
    expect(filterFrom(state, 'mm')).toEqual(DEFAULT_FILTER);
  });

  it('takes a circle diameter only with only circles, and checks it', () => {
    const state = { ...filterState(null, 'mm'), diameter: '6' };
    expect(filterFrom(state, 'mm')?.diameter).toBeNull();
    const circles = { ...state, onlyCircles: true };
    expect(filterFrom(circles, 'mm')?.diameter).toBe(6);
    expect(validateFilter({ ...circles, diameter: '-2' }, 'mm').diameter).toBeDefined();
    expect(filterFrom({ ...circles, tolerance: 'x' }, 'mm')).toBeNull();
  });

  it('picks named layers instead of all visible ones', () => {
    let state = filterState(null, 'mm');
    state = toggleLayer(state, 'Engrave', true);
    state = toggleLayer(state, 'Cut', true);
    state = toggleLayer(state, 'Cut', false);
    expect(filterFrom(state, 'mm')?.layers).toEqual(['Engrave']);
    expect(filterState({ ...DEFAULT_FILTER, layers: ['A'] }, 'mm').allLayers).toBe(false);
  });

  it('adds the filter to the params only when it is associated', () => {
    const params = { mode: 'fill' };
    expect(withSelector(params, null)).toEqual(params);
    expect(withSelector(params, DEFAULT_FILTER)).toEqual(params);
    const auto = { ...DEFAULT_FILTER, associate: true };
    expect(isAutomatic(auto)).toBe(true);
    expect(withSelector(params, auto)).toEqual({ mode: 'fill', selector: auto });
  });
});
