import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import {
  chamferMaxDepth,
  chamferParams,
  chamferSizes,
  chamferStateFrom,
  chamferToolWarning,
  editChamferSize,
  initialChamferState,
  validateChamfer,
} from './model';

function tool(kind: Tool['toolType']['kind'], extra: Record<string, unknown> = {}): Tool {
  return {
    id: 1,
    name: 'Tool',
    number: 1,
    diameter: 12,
    toolType: { kind, diameter: 12, ...extra },
    passDepth: 3,
    stepover: 1,
    stepoverPercent: 10,
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

describe('chamfer toolpath form model', () => {
  it('bevels inside, sloping down, 3 mm wide by default', () => {
    expect(chamferParams(initialChamferState('mm'), 'mm')).toEqual({
      selectionOrder: false,
      width: 3,
      depth: null,
      side: 'inside',
      startDepth: 0,
      direction: 'climb',
      overcut: 0,
      slope: 'down',
      angle: 45,
    });
  });

  it('sends a depth only when chamfering by depth', () => {
    const state = {
      ...initialChamferState('mm'),
      by: 'depth' as const,
      depth: '2',
      side: 'on' as const,
    };
    expect(chamferParams(state, 'mm')).toMatchObject({
      width: 3,
      depth: 2,
      side: 'on',
      startDepth: 0,
    });
    expect(chamferParams({ ...state, by: 'width' }, 'mm')?.depth).toBeNull();
  });

  it('checks the field that drives the chamfer', () => {
    const state = { ...initialChamferState('mm'), depth: '0' };
    expect(validateChamfer(state, 'mm')).toEqual({});
    expect(validateChamfer({ ...state, by: 'depth' }, 'mm').depth).toBe(strings.form.errorPositive);
    expect(validateChamfer({ ...state, width: '-1' }, 'mm').width).toBeDefined();
    expect(validateChamfer({ ...state, startDepth: '-1' }, 'mm').startDepth).toBeDefined();
    expect(chamferParams({ ...state, by: 'depth' }, 'mm')).toBeNull();
  });

  it('converts inches to millimetres and round trips', () => {
    const state = { ...initialChamferState('inch'), by: 'depth' as const, depth: '0.1' };
    const params = chamferParams(state, 'inch');
    expect(params?.depth).toBeCloseTo(2.54);
    if (params !== null) {
      expect(chamferParams(chamferStateFrom(params, 'C', 'inch'), 'inch')).toEqual(params);
      expect(chamferStateFrom(params, 'C', 'inch').by).toBe('depth');
    }
  });

  it('warns when the tool is not V shaped', () => {
    expect(chamferToolWarning(null)).toBeUndefined();
    expect(chamferToolWarning(tool('vBit'))).toBeUndefined();
    expect(chamferToolWarning(tool('engravingTool'))).toBeUndefined();
    expect(chamferToolWarning(tool('ballNose'))).toBeUndefined();
    expect(chamferToolWarning(tool('endMill'))).toBe(strings.toolpaths.chamfer.notV);
  });

  it('shows the depth the tool reaches, with the overcut', () => {
    const state = { ...initialChamferState('mm'), width: '2', overcut: '0.5', startDepth: '1' };
    // A 90 degree V: as deep as wide, plus the overcut down the same slope.
    expect(chamferMaxDepth(state, 'mm', tool('vBit', { includedAngle: 90 }))).toBeCloseTo(3.5);
    // A 60 degree V: 2 / tan 30.
    expect(
      chamferMaxDepth({ ...state, overcut: '0' }, 'mm', tool('vBit', { includedAngle: 60 })),
    ).toBeCloseTo(1 + 2 / Math.tan(Math.PI / 6));
    // A ball nose uses the form's angle.
    expect(
      chamferMaxDepth({ ...state, angle: '45', overcut: '0' }, 'mm', tool('ballNose')),
    ).toBeCloseTo(3);
    expect(chamferMaxDepth(state, 'mm', null)).toBeNull();
    expect(validateChamfer({ ...state, angle: '95', overcut: '-1' }, 'mm')).toMatchObject({
      angle: expect.any(String),
      overcut: expect.any(String),
    });
  });

  it('opens a chamfer saved before direction, overcut and slope', () => {
    const state = chamferStateFrom(
      { width: 2, depth: null, side: 'inside', startDepth: 0 },
      'Old',
      'mm',
    );
    expect(state).toMatchObject({ direction: 'climb', overcut: '0', slope: 'up', angle: '45' });
  });

  it('links Width and Cut Depth through the tool angle', () => {
    const vbit = tool('vBit', { includedAngle: 90 });
    const state = initialChamferState('mm');
    expect(chamferSizes(state, 'mm', vbit)).toEqual({ width: '3', depth: '3' });
    const byDepth = editChamferSize(state, 'depth', '5', 'mm', vbit);
    expect(byDepth.by).toBe('depth');
    expect(chamferSizes(byDepth, 'mm', vbit)).toEqual({ width: '5', depth: '5' });
    expect(chamferParams(byDepth, 'mm')?.depth).toBe(5);
    // Without a tool angle the other field keeps what it showed.
    expect(chamferSizes(byDepth, 'mm', null).depth).toBe('5');
  });

  it('sends Use Vector Selection Order and reads it back', () => {
    const state = { ...initialChamferState('mm'), selectionOrder: true };
    const params = chamferParams(state, 'mm');
    expect(params?.selectionOrder).toBe(true);
    if (params !== null) {
      expect(chamferStateFrom(params, 'N', 'mm').selectionOrder).toBe(true);
      expect(
        chamferStateFrom({ ...params, selectionOrder: undefined }, 'N', 'mm').selectionOrder,
      ).toBe(false);
    }
  });
});
