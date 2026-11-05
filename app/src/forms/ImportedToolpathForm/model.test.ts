import { describe, expect, it } from 'vitest';

import type { ImportedToolpath, Tool } from '../../bridge/cam';
import { convertImportedUnits, importedEdit, importedState, validateImported } from './model';

const TOOL = { name: 'End Mill 6 mm' } as Tool;
const IMPORTED: ImportedToolpath = {
  id: 4,
  name: 'part',
  file: '/jobs/part.nc',
  offsetX: 25.4,
  offsetY: 0,
  startDepth: 1,
  feed: 1200,
  plunge: 300,
  spindle: 18000,
  tool: TOOL,
};

describe('imported toolpath form', () => {
  it('shows the placement and feeds in the display unit', () => {
    const state = importedState(IMPORTED, 'inch');
    expect(state.offsetX).toBe('1');
    expect(state.spindle).toBe('18000');
    expect(convertImportedUnits(state, 'inch', 'mm').offsetX).toBe('25.4');
  });

  it('sends millimetres, and nothing while a field is wrong', () => {
    const state = { ...importedState(IMPORTED, 'mm'), offsetY: '-5', startDepth: '2' };
    expect(importedEdit(4, state, 'mm')).toEqual({
      id: 4,
      name: 'part',
      offsetX: 25.4,
      offsetY: -5,
      startDepth: 2,
      feed: 1200,
      plunge: 300,
      spindle: 18000,
      tool: TOOL,
    });
    const broken = { ...state, feed: '0' };
    expect(validateImported(broken, 'mm').feed).toBeDefined();
    expect(importedEdit(4, broken, 'mm')).toBeNull();
    expect(validateImported({ ...state, name: ' ' }, 'mm').name).toBeDefined();
  });
});
