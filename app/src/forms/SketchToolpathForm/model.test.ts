import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { DEFAULT_FILTER } from '../../bridge/sketch';
import {
  convertSketchUnits,
  initialSketchState,
  isCarvingTool,
  sketchParams,
  sketchStateFrom,
  validateSketch,
} from './model';

function tool(kind: Tool['toolType']['kind']): Tool {
  return {
    id: 1,
    name: kind,
    number: 1,
    toolType: { kind },
    passDepth: 1,
    stepover: 1,
    stepoverPercent: 10,
    feedRate: 1000,
    plungeRate: 300,
  } as Tool;
}

describe('sketch carving form', () => {
  it('cuts with V shaped and ball nosed tools only', () => {
    expect(isCarvingTool(tool('vBit'))).toBe(true);
    expect(isCarvingTool(tool('ballNose'))).toBe(true);
    expect(isCarvingTool(tool('taperedBallNose'))).toBe(true);
    expect(isCarvingTool(tool('endMill'))).toBe(false);
    expect(isCarvingTool(null)).toBe(false);
  });

  it('sends the start depth and a flat depth only when limited', () => {
    const state = {
      ...initialSketchState('mm'),
      source: { ...initialSketchState('mm').source, bitmap: 5 },
    };
    expect(sketchParams(state, 'mm', 0)).toMatchObject({
      bitmap: 5,
      startDepth: 0,
      flatDepth: null,
    });
    const flat = { ...state, useFlatDepth: true, flatDepth: '0.8', startDepth: '0.2' };
    expect(sketchParams(flat, 'mm', 0)).toMatchObject({ startDepth: 0.2, flatDepth: 0.8 });
    expect(validateSketch({ ...flat, flatDepth: '0' }, 'mm', 0).flatDepth).toBeDefined();
    expect(validateSketch({ ...state, startDepth: '-1' }, 'mm', 0).startDepth).toBeDefined();
  });

  it('keeps an associated vector filter and reads everything back', () => {
    const base = initialSketchState('mm');
    const state = {
      ...base,
      source: { ...base.source, source: 'model' as const, lineThickness: 70 },
      selector: { ...DEFAULT_FILTER, associate: true },
    };
    const params = sketchParams(state, 'mm', 0);
    expect(params?.selector?.associate).toBe(true);
    if (params !== null) {
      const back = sketchStateFrom(params, 'Face', 'mm');
      expect(back.name).toBe('Face');
      expect(back.source.source).toBe('model');
      expect(back.source.lineThickness).toBe(70);
      expect(back.selector?.associate).toBe(true);
    }
  });

  it('converts depths with the job unit', () => {
    const inch = convertSketchUnits(initialSketchState('mm'), 'mm', 'inch');
    expect(Number(inch.flatDepth)).toBeCloseTo(0.0394, 3);
  });
});
