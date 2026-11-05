import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB, type JobSetup } from '../../bridge/types';
import { strings } from '../../lib/formStrings';
import { centrePoint, customAt, initialUnwrapper, unwrapBlocker } from './model';

const rotary: JobSetup = { ...DEFAULT_JOB, kind: 'rotary' };

describe('vector unwrapper model', () => {
  it('works on rotary jobs only, on one closed contour', () => {
    expect(unwrapBlocker(DEFAULT_JOB, 1, true)).toBe(strings.vectorUnwrapper.needsRotary);
    expect(unwrapBlocker(rotary, 1, true)).toBeNull();
    expect(unwrapBlocker(rotary, 0, false)).toBe(strings.vectorUnwrapper.needsSelection);
    expect(unwrapBlocker(rotary, 2, true)).toBe(strings.vectorUnwrapper.needsSelection);
    expect(unwrapBlocker(rotary, 1, false)).toBe(strings.vectorUnwrapper.needsSelection);
  });

  it('takes the centre of the contour, the workspace or a custom point', () => {
    const contour: [number, number, number, number] = [10, 20, 30, 60];
    const workspace: [number, number, number, number] = [0, 0, 100, 200];
    const state = initialUnwrapper('mm', contour);
    expect(state.refine).toBe(true);
    expect(centrePoint(state, contour, workspace, 'mm')).toEqual({ x: 20, y: 40 });
    expect(centrePoint({ ...state, centre: 'workspace' }, contour, workspace, 'mm')).toEqual({
      x: 50,
      y: 100,
    });
    const dragged = customAt(state, { x: 12, y: 34 }, 'mm');
    expect(dragged.centre).toBe('custom');
    expect(centrePoint(dragged, contour, workspace, 'mm')).toEqual({ x: 12, y: 34 });
    expect(centrePoint({ ...dragged, x: 'a' }, contour, workspace, 'mm')).toBeNull();
    expect(centrePoint(state, null, workspace, 'mm')).toBeNull();
  });
});
