import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { initialClearance } from '../../lib/clearanceFields';
import { syncEntries, type ClearanceEntry } from '../ClearanceToolsField/model';
import { initialVGrooveState, validateVGroove, vgrooveParams, vgrooveStateFrom } from './model';

function tool(id: number, kind: Tool['toolType']['kind'], diameter: number): Tool {
  return {
    id,
    name: `Tool ${id}`,
    number: id,
    diameter,
    toolType: { kind, diameter },
    passDepth: 3,
    stepover: diameter * 0.4,
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

const OFFSET_CLIMB = {
  strategy: 'offset',
  rasterAngle: 0,
  direction: 'climb',
  ramp: { enabled: false, distance: 20 },
};

function entry(t: Tool, cornerSharpen = false, clearance = initialClearance('mm')): ClearanceEntry {
  return { tool: t, cornerSharpen, clearance };
}

describe('v-groove toolpath form model', () => {
  it('carves as deep as needed, in the tool passes, unless a flat depth is set', () => {
    expect(vgrooveParams(initialVGrooveState('mm'), 'mm')).toEqual({
      startDepth: 0,
      flatDepth: null,
      maxPassDepth: null,
      clearanceTool: null,
      clearanceOptions: OFFSET_CLIMB,
      clearanceTools: [],
      selectionOrder: false,
      vectorStart: false,
    });
    expect(
      vgrooveParams({ ...initialVGrooveState('mm'), useFlatDepth: true }, 'mm')?.flatDepth,
    ).toBe(6);
  });

  it('checks the flat depth only when it is used, and round trips', () => {
    const state = { ...initialVGrooveState('inch'), flatDepth: '0' };
    expect(validateVGroove(state, 'inch')).toEqual({});
    expect(validateVGroove({ ...state, useFlatDepth: true }, 'inch').flatDepth).toBeDefined();
    const params = vgrooveParams(
      {
        ...initialVGrooveState('inch'),
        useFlatDepth: true,
        selectionOrder: true,
        vectorStart: true,
        useClearance: true,
        clearanceTools: [entry(tool(7, 'endMill', 6)), entry(tool(8, 'engravingTool', 3), true)],
      },
      'inch',
    );
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(vgrooveParams(vgrooveStateFrom(params, 'V', 'inch'), 'inch')).toEqual(params);
    }
  });

  it('sends the clearance tools only while Use Clearance Tools is ticked', () => {
    const endMill = tool(3, 'endMill', 6);
    const state = { ...initialVGrooveState('mm'), clearanceTools: [entry(endMill)] };
    expect(vgrooveParams(state, 'mm')?.clearanceTools).toEqual([]);
    expect(vgrooveParams({ ...state, useClearance: true }, 'mm')?.clearanceTools).toEqual([
      { tool: endMill, cornerSharpen: false, options: OFFSET_CLIMB },
    ]);
  });

  it('keeps each tool its own options, the strategy from the first, sharpening a later V', () => {
    const big = entry(tool(1, 'endMill', 12), false, {
      ...initialClearance('mm'),
      strategy: 'raster',
      rasterAngle: '30',
    });
    const small = entry(tool(2, 'endMill', 3), true, {
      ...initialClearance('mm'),
      direction: 'conventional',
    });
    const engraver = entry(tool(3, 'engravingTool', 3), true);
    const state = {
      ...initialVGrooveState('mm'),
      useClearance: true,
      clearanceTools: [big, small, engraver],
    };
    const sent = vgrooveParams(state, 'mm')?.clearanceTools ?? [];
    expect(sent.map((e) => [e.tool.id, e.cornerSharpen])).toEqual([
      [1, false],
      [2, false],
      [3, true],
    ]);
    expect(sent[1]?.options).toMatchObject({
      strategy: 'raster',
      rasterAngle: 30,
      direction: 'conventional',
    });
    // Moved first, the engraver clears rather than sharpens.
    const moved = syncEntries(state.clearanceTools, [engraver.tool, big.tool], 'mm');
    const again = vgrooveParams({ ...state, clearanceTools: moved }, 'mm')?.clearanceTools ?? [];
    expect(again.map((e) => [e.tool.id, e.cornerSharpen])).toEqual([
      [3, false],
      [1, false],
    ]);
  });

  it('checks the clearance options only while the tools are used', () => {
    const bad = entry(tool(2, 'endMill', 6), false, {
      ...initialClearance('mm'),
      strategy: 'raster',
      rasterAngle: '400',
    });
    const state = { ...initialVGrooveState('mm'), clearanceTools: [bad] };
    expect(validateVGroove(state, 'mm')).toEqual({});
    expect(validateVGroove({ ...state, useClearance: true }, 'mm').rasterAngle).toBeDefined();
  });

  it('loads toolpaths saved before the list and per tool options existed', () => {
    const old = { startDepth: 0, flatDepth: 3 } as unknown as Parameters<
      typeof vgrooveStateFrom
    >[0];
    const state = vgrooveStateFrom(old, 'Old', 'mm');
    expect(state.useClearance).toBe(false);
    expect(state.clearanceTools).toEqual([]);
    const single = vgrooveStateFrom(
      {
        startDepth: 0,
        flatDepth: 3,
        maxPassDepth: null,
        clearanceTool: tool(4, 'endMill', 6),
        clearanceOptions: { ...OFFSET_CLIMB, direction: 'conventional' },
      } as never,
      'Single',
      'mm',
    );
    expect(single.useClearance).toBe(true);
    expect(single.clearanceTools[0]?.tool.id).toBe(4);
    expect(single.clearanceTools[0]?.clearance.direction).toBe('conventional');
  });
});
