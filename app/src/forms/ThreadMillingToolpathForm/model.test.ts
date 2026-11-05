import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import type { EntitySummary } from '../../bridge/document';
import {
  applyPreset,
  holeCircles,
  threadLength,
  findPreset,
  externalFit,
  initialThreadState,
  METRIC_PRESETS,
  minorDiameter,
  multiPoint,
  presetsFor,
  threadParams,
  threadStateFrom,
  UNIFIED_PRESETS,
  validateThread,
  withTool,
} from './model';

describe('thread milling form model', () => {
  it('starts with an M10 internal right hand thread cut upwards', () => {
    expect(threadParams(initialThreadState('mm'), 'mm')).toEqual({
      startDepth: 0,
      maxDepth: 10,
      pitch: 1.5,
      diameter: 10,
      fitTolerance: 0.25,
      kind: 'internal',
      hand: 'right',
      cut: 'up',
      selectionOrder: false,
    });
  });

  it('fills the size from a preset, in the job units', () => {
    const state = applyPreset(initialThreadState('inch'), '1/4-20 UNC', 'inch');
    expect(state.diameter).toBe('0.25');
    expect(state.pitch).toBe('0.05');
    const params = threadParams(state, 'inch');
    expect(params?.diameter).toBeCloseTo(6.35);
    expect(params?.pitch).toBeCloseTo(1.27);
    expect(applyPreset(state, 'no such thread', 'inch').preset).toBe('');
    expect(findPreset('M12')).toEqual({ id: 'M12', diameter: 12, pitch: 1.75 });
  });

  it('knows the minor diameter of internal and external threads', () => {
    const params = threadParams(initialThreadState('mm'), 'mm');
    expect(params).not.toBeNull();
    if (params !== null) {
      expect(minorDiameter(params)).toBeCloseTo(8.376, 3);
      expect(minorDiameter({ ...params, kind: 'external' })).toBeCloseTo(8.16, 3);
    }
  });

  it('checks the sizes and round trips with the preset found again', () => {
    const bad = { ...initialThreadState('mm'), pitch: '0', maxDepth: '-2' };
    expect(Object.keys(validateThread(bad, 'mm')).sort()).toEqual(['maxDepth', 'pitch']);
    expect(threadParams(bad, 'mm')).toBeNull();
    const state = {
      ...applyPreset(initialThreadState('mm'), 'M16', 'mm'),
      kind: 'external' as const,
    };
    const params = threadParams(state, 'mm');
    expect(params).not.toBeNull();
    if (params !== null) {
      const back = threadStateFrom(params, 'T', 'mm');
      expect(back.preset).toBe('M16');
      expect(threadParams(back, 'mm')).toEqual(params);
    }
  });

  it('shortens the thread length by a thread mill tooth offset', () => {
    const tool = {
      toolType: { kind: 'threadMill', diameter: 6, toothOffset: 1.5 },
    } as unknown as Tool;
    expect(threadLength(10, tool)).toBe(8.5);
    expect(threadLength(10, null)).toBe(10);
  });

  it('draws minor diameter circles at the selected closed vectors', () => {
    const params = threadParams(initialThreadState('mm'), 'mm');
    expect(params).not.toBeNull();
    if (params === null) {
      return;
    }
    const entity = (id: number, closed: boolean): EntitySummary =>
      ({ id, kind: 'path', closed, bbox: [10, 20, 20, 30] }) as unknown as EntitySummary;
    const circles = holeCircles(params, [entity(1, true), entity(2, false)]);
    expect(circles).toHaveLength(1);
    const sub = circles[0]?.subpaths[0];
    const minor = minorDiameter(params);
    expect(sub?.start.x).toBeCloseTo(15 + minor / 2);
    expect(sub?.start.y).toBeCloseTo(25);
    expect(sub?.segments).toHaveLength(2);
  });

  it('round trips the selection order', () => {
    const params = threadParams({ ...initialThreadState('mm'), selectionOrder: true }, 'mm');
    expect(params?.selectionOrder).toBe(true);
    if (params !== null) {
      expect(threadStateFrom(params, 'T', 'mm').selectionOrder).toBe(true);
    }
  });

  it('offers the metric presets in a metric job and the unified ones in an inch job', () => {
    expect(presetsFor('mm')).toBe(METRIC_PRESETS);
    expect(presetsFor('inch')).toBe(UNIFIED_PRESETS);
    // A toolpath of the other system shows as Custom.
    const params = threadParams(applyPreset(initialThreadState('mm'), 'M12', 'mm'), 'mm');
    if (params !== null) {
      expect(threadStateFrom(params, 'T', 'inch').preset).toBe('');
      expect(threadStateFrom(params, 'T', 'mm').preset).toBe('M12');
    }
  });

  it('fills the fit tolerance from an external preset only', () => {
    const internal = applyPreset(initialThreadState('mm'), 'M16', 'mm');
    expect(internal.fitTolerance).toBe('0.25');
    const external = applyPreset(
      { ...initialThreadState('mm'), kind: 'external' as const },
      'M16',
      'mm',
    );
    expect(external.fitTolerance).toBe(String(externalFit(2)));
    expect(externalFit(2)).toBe(0.2);
  });

  it('locks the pitch to a multi point tool and caps the thread at its teeth', () => {
    const tool = (teeth: number) =>
      ({
        toolType: { kind: 'threadMill', diameter: 8, pitch: 1.25, teeth, toothOffset: 0 },
      }) as unknown as Tool;
    expect(multiPoint(tool(1))).toBeNull();
    expect(multiPoint(null)).toBeNull();
    expect(multiPoint(tool(4))).toEqual({ pitch: 1.25, reach: 5 });
    const state = withTool(initialThreadState('mm'), tool(4), 'mm');
    expect(state.pitch).toBe('1.25');
    expect(state.preset).toBe('');
    expect(withTool(state, tool(4), 'mm')).toBe(state);
    expect(withTool(initialThreadState('mm'), tool(1), 'mm').pitch).toBe('1.5');
    // Ten millimetres of thread is longer than four teeth reach.
    expect(validateThread(state, 'mm', tool(4)).maxDepth).toBeDefined();
    expect(threadParams(state, 'mm', tool(4))).toBeNull();
    expect(validateThread({ ...state, maxDepth: '5' }, 'mm', tool(4))).toEqual({});
  });
});
