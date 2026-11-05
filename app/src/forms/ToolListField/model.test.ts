import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { addTool, moveTool, removeTool, replaceTool } from './model';

function tool(id: number, diameter = 6): Tool {
  return {
    id,
    name: `Tool ${id}`,
    number: id,
    toolType: { kind: 'endMill', diameter },
    passDepth: 3,
    stepover: 2.4,
    stepoverPercent: 40,
    feedRate: 1000,
    plungeRate: 300,
    feedUnits: 'mm/min',
    spindleSpeed: 18000,
    notes: '',
    units: 'mm',
    clearance: 0,
    cuttingData: [],
  };
}

describe('ordered tool list model', () => {
  it('adds a tool at the end and selects it; picking a listed tool again replaces it', () => {
    const one = addTool([], tool(1));
    expect(one).toEqual({ tools: [tool(1)], index: 0 });
    const two = addTool(one.tools, tool(2));
    expect(two.tools.map((t) => t.id)).toEqual([1, 2]);
    expect(two.index).toBe(1);
    const again = addTool(two.tools, tool(1, 8));
    expect(again.tools.map((t) => t.toolType.diameter)).toEqual([8, 6]);
    expect(again.index).toBe(0);
  });

  it('removes a tool and keeps a row selected', () => {
    const tools = [tool(1), tool(2), tool(3)];
    expect(removeTool(tools, 2)).toEqual({ tools: [tool(1), tool(2)], index: 1 });
    expect(removeTool(tools, 0).tools.map((t) => t.id)).toEqual([2, 3]);
    expect(removeTool([tool(1)], 0)).toEqual({ tools: [], index: -1 });
  });

  it('moves a tool up and down within the list', () => {
    const tools = [tool(1), tool(2), tool(3)];
    const down = moveTool(tools, 0, 1);
    expect(down.tools.map((t) => t.id)).toEqual([2, 1, 3]);
    expect(down.index).toBe(1);
    expect(moveTool(tools, 0, -1).tools.map((t) => t.id)).toEqual([1, 2, 3]);
    expect(moveTool(tools, 2, 1).index).toBe(2);
  });

  it('replaces the edited tool in place', () => {
    const edited = { ...tool(2), passDepth: 1 };
    expect(replaceTool([tool(1), tool(2)], 1, edited)[1]?.passDepth).toBe(1);
  });
});
