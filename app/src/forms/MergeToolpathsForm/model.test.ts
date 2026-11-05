import { describe, expect, it } from 'vitest';

import type { ToolpathSummary } from '../../bridge/cam';
import { initialMergeState, mergeRequest, mergedToolpaths, mixesTools, setOrder } from './model';

const toolpath = (id: number, toolName: string, visible = true): ToolpathSummary => ({
  id,
  name: `T${id}`,
  kind: 'profile',
  visible,
  stale: false,
  toolName,
  toolNumber: 1,
  estimatedSeconds: 10,
  moves: 20,
});

describe('merge toolpaths form model', () => {
  it('merges the visible toolpaths, in list order', () => {
    const list = [
      toolpath(3, 'End Mill 6'),
      toolpath(2, 'V-Bit', false),
      toolpath(1, 'End Mill 6'),
    ];
    expect(mergedToolpaths(list).map((t) => t.id)).toEqual([3, 1]);
    expect(mixesTools(list)).toBe(false);
    const state = { ...initialMergeState(), name: ' Frame ' };
    expect(mergeRequest(state, list)).toEqual({
      ids: [3, 1],
      name: 'Frame',
      options: { orders: [], byPart: true },
    });
  });

  it('needs two visible toolpaths of one tool', () => {
    const state = initialMergeState();
    expect(mergeRequest(state, [toolpath(1, 'A'), toolpath(2, 'A', false)])).toBeNull();
    const mixed = [toolpath(1, 'A'), toolpath(2, 'B')];
    expect(mixesTools(mixed)).toBe(true);
    expect(mergeRequest(state, mixed)).toBeNull();
  });

  it('ticks orders on and off, keeping the form order', () => {
    expect(initialMergeState().orders).toEqual([]);
    const two = setOrder(setOrder(initialMergeState(), 'shortestPath', true), 'bottomToTop', true);
    expect(two.orders).toEqual(['bottomToTop', 'shortestPath']);
    expect(setOrder(two, 'leftToRight', true).orders).toEqual([
      'leftToRight',
      'bottomToTop',
      'shortestPath',
    ]);
    expect(setOrder(two, 'shortestPath', false).orders).toEqual(['bottomToTop']);
  });
});
