import { describe, expect, it } from 'vitest';

import {
  initialPlateState,
  plateArgs,
  plateGrid,
  platesPerSheet,
  setMargin,
  validatePlates,
  withVariables,
} from './model';

describe('plate production model', () => {
  it('keeps margins equal until told otherwise', () => {
    const state = setMargin(initialPlateState('mm'), 'left', '12');
    expect([state.left, state.right, state.top, state.bottom]).toEqual(['12', '12', '12', '12']);
    const free = setMargin({ ...state, equalMargins: false }, 'top', '3');
    expect([free.left, free.top]).toEqual(['12', '3']);
  });

  it('gives variables a column by name, by place or a counter', () => {
    const state = withVariables(initialPlateState('mm'), ['dept', 'name'], ['Name', 'Dept']);
    expect(state.fields['dept']).toEqual({ kind: 'column', column: 1 });
    expect(state.fields['name']).toEqual({ kind: 'column', column: 0 });
    const counters = withVariables(initialPlateState('mm'), ['n'], []);
    expect(counters.fields['n']?.kind).toBe('counter');
    // A column choice past the end of a new file falls back.
    const kept = withVariables(
      { ...state, fields: { name: { kind: 'column', column: 5 } } },
      ['name'],
      ['Name'],
    );
    expect(kept.fields['name']).toEqual({ kind: 'column', column: 0 });
  });

  it('fits plates on the material', () => {
    expect(
      plateGrid({ width: 100, height: 50 }, { width: 30, height: 20 }, { x: 5, y: 5 }),
    ).toEqual({ columns: 3, rows: 2 });
    const state = initialPlateState('mm');
    expect(platesPerSheet(state, 'mm', { width: 120, height: 70 }, { width: 30, height: 20 })).toBe(
      6,
    );
    expect(
      platesPerSheet({ ...state, ownSheets: true }, 'mm', { width: 120, height: 70 }, null),
    ).toBeNull();
    expect(
      platesPerSheet(
        { ...state, ownSheets: true },
        'mm',
        { width: 120, height: 70 },
        { width: 30, height: 20 },
      ),
    ).toBe(1);
  });

  it('builds kernel arguments with counters or a data file', () => {
    const state = withVariables(initialPlateState('mm'), ['n'], []);
    const args = plateArgs({ ...state, count: '4' }, 'mm', [1, 2]);
    expect(args?.count).toBe(4);
    expect(args?.path).toBeUndefined();
    expect(args?.fields['n']).toEqual({ kind: 'counter', start: 1, step: 1, digits: 0 });
    expect(args?.margins.left).toBe(10);
    expect(args?.mode).toBe('sheets');
    expect(plateArgs({ ...state, mode: 'layers' }, 'mm', [1])?.mode).toBe('layers');
    const withFile = plateArgs({ ...state, path: '/tmp/names.csv' }, 'mm', [1]);
    expect(withFile?.path).toBe('/tmp/names.csv');
    expect(withFile?.count).toBeUndefined();
    // Toolpaths only when the template has some and the box is ticked.
    expect(args?.toolpaths).toBe(false);
    expect(plateArgs(state, 'mm', [1], 2)?.toolpaths).toBe(true);
    expect(plateArgs({ ...state, toolpaths: false }, 'mm', [1], 2)?.toolpaths).toBe(false);
    expect(plateArgs(state, 'mm', [])).toBeNull();
    expect(validatePlates({ ...state, count: '0' }, 'mm').count).toBeDefined();
    const bad = {
      ...state,
      fields: { n: { kind: 'counter' as const, start: 'x', step: '1', digits: '0' } },
    };
    expect(validatePlates(bad, 'mm')['field:n']).toBeDefined();
  });
});
