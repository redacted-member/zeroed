import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import type { Sheet } from '../../bridge/document';
import { initialClearance } from '../../lib/clearanceFields';
import {
  activeSheetOf,
  initialInlayState,
  inlayBaseName,
  inlayJobs,
  inlayParams,
  inlayStateFrom,
  inlayToolWarning,
  plugSheetFor,
  validateInlay,
} from './model';

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

const text = strings.toolpaths.inlay;

describe('inlay toolpath form model', () => {
  it('starts from the depths for each part', () => {
    expect(inlayParams(initialInlayState('mm'), 'mm', 'female')).toEqual({
      pocketDepth: 5,
      glueGap: 0.2,
      plugDepth: 0.2,
      plugBoundary: 'sheetLimits',
      boundaryExtra: 2,
      mirrorPlug: false,
      boundaryOffset: null,
      maxPassDepth: null,
      clearanceTool: null,
      clearanceOptions: {
        strategy: 'offset',
        rasterAngle: 0,
        direction: 'climb',
        ramp: { enabled: false, distance: 20 },
      },
      clearanceTools: [],
      part: 'female',
    });
    expect(inlayParams(initialInlayState('mm'), 'mm', 'male')?.part).toBe('male');
  });

  it('needs a glue gap above zero and less than the pocket, and a surface clearance', () => {
    const state = initialInlayState('mm');
    expect(validateInlay(state, 'mm')).toEqual({});
    expect(validateInlay({ ...state, glueGap: '5' }, 'mm').glueGap).toBe(text.glueTooDeep);
    expect(validateInlay({ ...state, glueGap: '0' }, 'mm').glueGap).toBeDefined();
    expect(validateInlay({ ...state, surfaceGap: '0' }, 'mm').surfaceGap).toBeDefined();
    expect(validateInlay({ ...state, pocketDepth: '0' }, 'mm').pocketDepth).toBe(
      strings.form.errorPositive,
    );
  });

  it('checks the boundary offset only for a vector offset boundary', () => {
    const state = {
      ...initialInlayState('mm'),
      boundaryOffset: '-1',
      plugBoundary: 'vectorOffset' as const,
    };
    expect(validateInlay(state, 'mm').boundaryOffset).toBeDefined();
    expect(validateInlay({ ...state, plugBoundary: 'sheetLimits' }, 'mm')).toEqual({});
  });

  it('always makes a pocket and a plug named after a free base name', () => {
    const state = initialInlayState('mm');
    const jobs = inlayJobs(state, 'mm', ['VGroove Inlay 1 Pocket'], null);
    expect(jobs.map((j) => [j.part, j.name])).toEqual([
      ['female', 'VGroove Inlay 2 Pocket'],
      ['male', 'VGroove Inlay 2 Plug'],
    ]);
    const named = inlayJobs({ ...state, name: ' Rose ' }, 'mm', [], null);
    expect(named.map((j) => j.name)).toEqual(['Rose Pocket', 'Rose Plug']);
    expect(inlayBaseName('', [])).toBe('VGroove Inlay 1');
  });

  it('recalculates only the part being edited, under its own name', () => {
    const params = inlayParams(initialInlayState('mm'), 'mm', 'male');
    expect(params).not.toBeNull();
    if (params !== null) {
      const state = inlayStateFrom(params, 'Inlay 3 Plug', 'mm');
      const jobs = inlayJobs(state, 'mm', ['Inlay 3 Plug'], 'male');
      expect(jobs.map((j) => [j.part, j.name])).toEqual([['male', 'Inlay 3 Plug']]);
    }
  });

  it('reads an older toolpath without a surface clearance as cleared to the pocket depth', () => {
    const state = inlayStateFrom(
      {
        pocketDepth: 4,
        glueGap: 1,
        plugDepth: null,
        mirrorPlug: true,
        boundaryOffset: null,
        maxPassDepth: null,
        clearanceTool: null,
        part: 'female',
      },
      'Old',
      'mm',
    );
    expect(state.surfaceGap).toBe('4');
    expect(state.plugBoundary).toBe('vectorOffset');
    // Older plugs mirror their own vectors.
    expect(state.mirrorPlug).toBe(true);
  });

  it('round trips every setting in inches', () => {
    const state = {
      ...initialInlayState('inch'),
      plugBoundary: 'sheetLimits' as const,
      useClearance: true,
      clearanceTools: [
        { tool: tool(4, 'endMill', 6), cornerSharpen: false, clearance: initialClearance('inch') },
        { tool: tool(5, 'endMill', 3), cornerSharpen: false, clearance: initialClearance('inch') },
      ],
    };
    const params = inlayParams(state, 'inch', 'female');
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(params.pocketDepth).toBeCloseTo(5.08);
      expect(inlayParams(inlayStateFrom(params, 'A', 'inch'), 'inch', 'female')).toEqual(params);
    }
  });

  it('warns about a tool that is not V shaped', () => {
    expect(inlayToolWarning(null)).toBeUndefined();
    expect(inlayToolWarning(tool(1, 'vBit', 12))).toBeUndefined();
    expect(inlayToolWarning(tool(2, 'endMill', 6))).toBe(text.notV);
  });

  it('sends the clearance tools in order only while they are used', () => {
    const entry = (id: number) => ({
      tool: tool(id, 'endMill', 6),
      cornerSharpen: true,
      clearance: initialClearance('mm'),
    });
    const state = { ...initialInlayState('mm'), clearanceTools: [entry(1), entry(2)] };
    expect(inlayParams(state, 'mm', 'male')?.clearanceTools).toEqual([]);
    const sent = inlayParams({ ...state, useClearance: true }, 'mm', 'male')?.clearanceTools ?? [];
    expect(sent.map((e) => [e.tool.id, e.cornerSharpen])).toEqual([
      [1, false],
      [2, false],
    ]);
    // An older single clearance tool reads as the list.
    const old = inlayStateFrom(
      {
        ...(inlayParams(initialInlayState('mm'), 'mm', 'male') ?? ({} as never)),
        clearanceTool: tool(7, 'endMill', 6),
        clearanceTools: [],
      },
      'Old',
      'mm',
    );
    expect(old.useClearance).toBe(true);
    expect(old.clearanceTools[0]?.tool.id).toBe(7);
  });

  it('puts the plug on the picked sheet, else the active one', () => {
    const sheets = [
      { id: 1, name: 'Sheet 1', active: false },
      { id: 2, name: 'Sheet 2', active: true },
    ] as Sheet[];
    expect(activeSheetOf(sheets)).toBe(2);
    expect(activeSheetOf([])).toBeNull();
    const state = initialInlayState('mm');
    expect(plugSheetFor(state, 2)).toBe(2);
    expect(plugSheetFor({ ...state, plugSheet: 1 }, 2)).toBe(1);
  });
});
