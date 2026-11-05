import { describe, expect, it } from 'vitest';

import type { Tool } from '../bridge/cam';
import { toolpathsStrings } from './strings/toolpaths';
import { calculateWhy, lastToolFor, rememberTool } from './useToolpathForm';

const endMill: Tool = {
  id: 24,
  name: 'End Mill 1/4 inch',
  number: 14,
  toolType: { kind: 'endMill', diameter: 6.35 },
  passDepth: 3.175,
  stepover: 2.54,
  stepoverPercent: 40,
  feedRate: 80,
  plungeRate: 25,
  spindleSpeed: 18000,
  units: 'inch',
  feedUnits: 'inchPerMin',
  flutes: 2,
  notes: '',
  clearance: 0,
  cuttingData: [],
} as Tool;

describe('toolpath form tool memory', () => {
  it('starts a new form of a kind with the tool that kind was last calculated with', () => {
    expect(lastToolFor('profile')).toBeNull();
    rememberTool('profile', endMill);
    expect(lastToolFor('profile')).toEqual(endMill);
    // Each kind of toolpath keeps its own.
    expect(lastToolFor('pocket')).toBeNull();
  });
});

describe('why Calculate is off', () => {
  it('names the missing selection first, then the missing tool', () => {
    expect(calculateWhy(false, false)).toBe(toolpathsStrings.toolpaths.needsSelection);
    expect(calculateWhy(false, true)).toBe(toolpathsStrings.toolpaths.needsSelection);
    expect(calculateWhy(true, false)).toBe(toolpathsStrings.toolpaths.needsTool);
    expect(calculateWhy(true, true)).toBeUndefined();
  });
});
