import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { initialClearance } from '../../lib/clearanceFields';
import { strings } from '../../lib/formStrings';
import {
  canSharpen,
  entriesFrom,
  firstToolWarning,
  kernelEntries,
  sharedOptions,
  syncEntries,
  validateEntries,
  type ClearanceEntry,
} from './model';

function tool(id: number, kind: Tool['toolType']['kind']): Tool {
  return { id, name: `T${id}`, toolType: { kind, diameter: 6 } } as unknown as Tool;
}

const conventional = { ...initialClearance('mm'), direction: 'conventional' as const };

describe('clearance tools list', () => {
  it('keeps each kept tool its options and starts a new one with the first tools', () => {
    const a: ClearanceEntry = {
      tool: tool(1, 'endMill'),
      cornerSharpen: false,
      clearance: conventional,
    };
    const b: ClearanceEntry = {
      tool: tool(2, 'vBit'),
      cornerSharpen: true,
      clearance: initialClearance('mm'),
    };
    expect(syncEntries([a, b], [b.tool, a.tool], 'mm')).toEqual([b, a]);
    const added = syncEntries([a], [a.tool, tool(3, 'endMill')], 'mm');
    expect(added[1]).toMatchObject({ cornerSharpen: false, clearance: conventional });
    // An edited copy of a row keeps that row's options.
    const edited = { ...a.tool, name: 'Edited' };
    expect(syncEntries([a, b], [edited, b.tool], 'mm')[0]).toEqual({ ...a, tool: edited });
    expect(syncEntries([a, b], [b.tool], 'mm')).toEqual([b]);
  });

  it('sends every tool its options and sharpening only for a later V tool', () => {
    const entries: ClearanceEntry[] = [
      { tool: tool(1, 'vBit'), cornerSharpen: true, clearance: initialClearance('mm') },
      { tool: tool(2, 'endMill'), cornerSharpen: true, clearance: conventional },
      { tool: tool(3, 'engravingTool'), cornerSharpen: true, clearance: initialClearance('mm') },
    ];
    const sent = kernelEntries(entries, 'mm');
    expect(sent.map((e) => e.cornerSharpen)).toEqual([false, false, true]);
    expect(sent[1]?.options?.direction).toBe('conventional');
    expect(kernelEntries(entries, 'mm', false).some((e) => e.cornerSharpen)).toBe(false);
    expect(sharedOptions(entries, 'mm').direction).toBe('climb');
    expect(canSharpen(tool(1, 'vBit'))).toBe(true);
    expect(canSharpen(tool(1, 'endMill'))).toBe(false);
    expect(firstToolWarning(entries)).toBe(strings.toolpaths.vgroove.clearanceV);
    expect(firstToolWarning(entries.slice(1))).toBeUndefined();
  });

  it('reads a saved list, older tools taking the shared options', () => {
    const shared = {
      ...kernelEntries([], 'mm'),
      strategy: 'raster',
      rasterAngle: 10,
      direction: 'climb',
      ramp: { enabled: false, distance: 20 },
    } as never;
    const back = entriesFrom([{ tool: tool(1, 'endMill'), cornerSharpen: false }], shared, 'mm');
    expect(back[0]?.clearance).toMatchObject({ strategy: 'raster', rasterAngle: '10' });
    const bad = [
      {
        ...back[0],
        clearance: { ...initialClearance('mm'), ramp: true, rampDistance: '0' },
      } as ClearanceEntry,
    ];
    expect(validateEntries(bad, 'mm')?.errors.rampDistance).toBeDefined();
    expect(validateEntries(back, 'mm')).toBeNull();
  });
});
