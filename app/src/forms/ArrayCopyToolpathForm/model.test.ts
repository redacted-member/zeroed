import { describe, expect, it } from 'vitest';

import type { ToolpathSummary } from '../../bridge/cam';
import {
  arrayCopySpec,
  arraySources,
  initialArrayCopyState,
  mixesTools,
  validateArrayCopy,
} from './model';

function toolpath(id: number, toolName: string, visible = true): ToolpathSummary {
  return {
    id,
    name: `T${id}`,
    kind: 'profile',
    visible,
    stale: false,
    toolName,
    toolNumber: 1,
    estimatedSeconds: 1,
    moves: 3,
  };
}

describe('array copy toolpath form', () => {
  it('copies the visible toolpaths', () => {
    const list = [toolpath(1, 'A'), toolpath(2, 'A', false), toolpath(3, 'B')];
    expect(arraySources(list).map((t) => t.id)).toEqual([1, 3]);
    expect(mixesTools(arraySources(list))).toBe(true);
    expect(mixesTools([toolpath(1, 'A')])).toBe(false);
  });

  it('sends the grid and spacing in millimetres', () => {
    const state = {
      ...initialArrayCopyState('inch'),
      rows: '3',
      spacing: 'offset' as const,
      x: '2',
    };
    expect(arrayCopySpec(state, 'inch', [toolpath(1, 'A')])).toEqual({
      rows: 3,
      columns: 2,
      spacing: 'offset',
      x: 50.8,
      y: 12.7,
      minimizeToolChanges: false,
    });
    expect(
      arrayCopySpec(state, 'inch', [toolpath(1, 'A'), toolpath(2, 'B')])?.minimizeToolChanges,
    ).toBe(true);
  });

  it('needs at least two copies and whole rows and columns', () => {
    const state = initialArrayCopyState('mm');
    expect(validateArrayCopy({ ...state, rows: '1', columns: '1' }, 'mm', 'grid').grid).toBe(
      'grid',
    );
    expect(validateArrayCopy({ ...state, rows: '1.5' }, 'mm', 'grid').rows).toBeDefined();
    expect(arrayCopySpec({ ...state, columns: '0' }, 'mm', [])).toBeNull();
  });
});
