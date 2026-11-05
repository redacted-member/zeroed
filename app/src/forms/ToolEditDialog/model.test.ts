import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import {
  editedTool,
  foldMaxPassDepth,
  foldToolOverrides,
  setStepover,
  setStepoverPercent,
  toolEditState,
  validateToolEdit,
} from './model';

const tool: Tool = {
  id: 7,
  name: 'End Mill 6',
  number: 1,
  toolType: { kind: 'endMill', diameter: 6 },
  passDepth: 3,
  stepover: 2.4,
  stepoverPercent: 40,
  feedRate: 1000,
  plungeRate: 300,
  feedUnits: 'mmPerMin',
  spindleSpeed: 18000,
  notes: '',
  units: 'mm',
  clearance: 0,
  cuttingData: [],
};

describe('Edit tool (toolpath copy)', () => {
  it('starts from the tool and links the stepover to its percentage', () => {
    const state = toolEditState(tool, 'mm');
    expect(state).toMatchObject({ passDepth: '3', stepover: '2.4', stepoverPercent: '40' });
    expect(setStepover(state, '3', tool, 'mm').stepoverPercent).toBe('50');
    expect(setStepoverPercent(state, '25', tool, 'mm').stepover).toBe('1.5');
  });

  it('answers the edited copy, keeping the database id', () => {
    const state = { ...toolEditState(tool, 'mm'), passDepth: '2', feedRate: '1500', number: '4' };
    expect(editedTool(tool, state, 'mm')).toMatchObject({
      id: 7,
      passDepth: 2,
      feedRate: 1500,
      number: 4,
    });
  });

  it('refuses a zero pass depth or a stepover over 100%', () => {
    const state = toolEditState(tool, 'mm');
    expect(validateToolEdit({ ...state, passDepth: '0' }, 'mm').passDepth).toBeDefined();
    expect(
      validateToolEdit({ ...state, stepoverPercent: '120' }, 'mm').stepoverPercent,
    ).toBeDefined();
    expect(editedTool(tool, { ...state, feedRate: '0' }, 'mm')).toBeNull();
  });

  it('folds the pass depth and stepover overrides of older toolpaths into the tool', () => {
    const folded = foldToolOverrides(tool, { passDepth: 1.5, stepover: null, cutDepth: 6 });
    expect(folded.tool.passDepth).toBe(1.5);
    expect(folded.tool.stepover).toBe(2.4);
    expect(folded.params).toEqual({ passDepth: null, stepover: null, cutDepth: 6 });
    const plain = { passDepth: null, cutDepth: 6 };
    expect(foldToolOverrides(tool, plain).params).toBe(plain);
  });

  it('folds an older V-Groove max pass depth into the tool', () => {
    const folded = foldMaxPassDepth(tool, { maxPassDepth: 2, flatDepth: null });
    expect(folded.tool.passDepth).toBe(2);
    expect(folded.params).toEqual({ maxPassDepth: null, flatDepth: null });
    const plain = { maxPassDepth: null };
    expect(foldMaxPassDepth(tool, plain).params).toBe(plain);
  });
});
