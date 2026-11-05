import { describe, expect, it } from 'vitest';

import {
  backFaces,
  shownBack,
  convertExportStlUnits,
  countError,
  initialExportStlState,
  limitsTriangles,
  meshOptions,
  toleranceError,
  type ExportStlState,
} from './model';

const mm = initialExportStlState('mm');

function state(patch: Partial<ExportStlState>): ExportStlState {
  return { ...mm, ...patch };
}

describe('export model form', () => {
  it('starts at a fine tolerance and a flat back, with no base thickness field', () => {
    expect(mm.reduceBy).toBe('tolerance');
    expect(mm.back).toBe('flat');
    expect(meshOptions(mm, 'mm')).toEqual({ back: 'flat', tolerance: 0.01 });
    expect('base' in mm).toBe(false);
  });

  it('takes the tolerance in the job unit', () => {
    const options = meshOptions(state({ tolerance: '0.001' }), 'inch');
    expect(options?.tolerance).toBeCloseTo(0.0254);
  });

  it('asks for a triangle budget instead of a tolerance', () => {
    const counted = state({ reduceBy: 'count', count: '5000', back: 'inverted', tolerance: 'x' });
    expect(meshOptions(counted, 'mm')).toEqual({ back: 'inverted', maxTriangles: 5000 });
    expect(countError(state({ reduceBy: 'count', count: '1' }))).toBeDefined();
    expect(countError(state({ reduceBy: 'count', count: '10.5' }))).toBeDefined();
    expect(meshOptions(state({ reduceBy: 'count', count: 'x' }), 'mm')).toBeNull();
  });

  it('refuses a tolerance that is not above zero', () => {
    expect(toleranceError(state({ tolerance: '0' }), 'mm')).toBeDefined();
    expect(meshOptions(state({ tolerance: '0' }), 'mm')).toBeNull();
  });

  it('offers the other side only on a double sided job; a rotary model is open or capped', () => {
    expect(backFaces('singleSided')).toEqual(['open', 'inverted', 'flat']);
    expect(backFaces('doubleSided')).toContain('otherSide');
    expect(backFaces('rotary')).toEqual(['open', 'sides']);
    expect(shownBack('sides', 'singleSided')).toBe('flat');
    expect(shownBack('sides', 'rotary')).toBe('sides');
    expect(shownBack('flat', 'rotary')).toBe('sides');
    expect(limitsTriangles('rotary')).toBe(false);
    expect(limitsTriangles('doubleSided')).toBe(true);
    expect(meshOptions(state({ back: 'otherSide' }), 'mm')?.back).toBe('otherSide');
  });

  it('converts lengths between units', () => {
    const inch = convertExportStlUnits(state({ tolerance: '2.54' }), 'mm', 'inch');
    expect(meshOptions(inch, 'inch')?.tolerance).toBeCloseTo(2.54);
  });
});
