import { describe, expect, it } from 'vitest';

import type { Tool } from '../../bridge/cam';
import { initialPocketState } from '../PocketToolpathForm/model';
import { inlayPocketParams, inlayPocketStateFrom } from './model';

const small = { id: 9, name: 'End Mill 3 mm', toolType: { kind: 'endMill' } } as unknown as Tool;

describe('female inlay pocket form', () => {
  it('grows the cavity by the pocket allowance and sends the tool list', () => {
    const state = { ...initialPocketState('mm'), allowance: '0.5', laterTools: [small] };
    const params = inlayPocketParams(state, 'mm');
    expect(params).toMatchObject({ part: 'female', female: 'pocket', pocketAllowance: 0.5 });
    expect(params?.pocketOptions?.allowance).toBe(-0.5);
    expect(params?.restTools).toEqual([small]);
    expect(inlayPocketParams({ ...state, allowance: 'x' }, 'mm')).toBeNull();
  });

  it('reads a pocket inlay back, and an older one without Pocket options', () => {
    const state = { ...initialPocketState('mm'), strategy: 'raster' as const, allowance: '0.2' };
    const params = inlayPocketParams(state, 'mm');
    expect(params).not.toBeNull();
    if (params === null) {
      return;
    }
    const back = inlayPocketStateFrom(params, 'Female Inlay 1', 'mm');
    expect(back).toMatchObject({ name: 'Female Inlay 1', strategy: 'raster', allowance: '0.2' });
    expect(inlayPocketParams(back, 'mm')).toEqual(params);
    const legacy = inlayPocketStateFrom(
      { ...params, pocketOptions: undefined, strategy: 'offset' },
      'Old',
      'mm',
    );
    expect(legacy.strategy).toBe('offset');
    expect(legacy.allowance).toBe('0.2');
  });
});
