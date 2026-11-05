import { describe, expect, it } from 'vitest';

import type { Tiling } from '../../bridge/cam';
import {
  clearedTiling,
  datumBlocksTiling,
  initialTilingForm,
  tilingFromForm,
  usedFields,
  validateTilingForm,
} from './model';

const stored: Tiling = {
  enabled: false,
  spec: { method: 'feedY', tileWidth: 0, tileHeight: 400, overlap: 6, smallestFirst: true },
  active: 2,
  originalPosition: true,
};

describe('toolpath tiling form', () => {
  it('starts from the document, with tiling off until it is switched on', () => {
    expect(initialTilingForm(null, 'mm').enabled).toBe(false);
    expect(initialTilingForm({ ...stored, enabled: true }, 'mm').enabled).toBe(true);
    const state = initialTilingForm(stored, 'mm');
    expect(state.enabled).toBe(false);
    expect(state.method).toBe('feedY');
    expect(state.tileHeight).toBe('400');
    expect(state.tileWidth).toBe('600');
  });

  it('sends only the sizes the layout uses, in millimetres', () => {
    const state = initialTilingForm(stored, 'mm');
    const tiling = tilingFromForm(state, 'mm', stored);
    expect(tiling?.spec).toEqual({
      method: 'feedY',
      tileWidth: 0,
      tileHeight: 400,
      overlap: 6,
      smallestFirst: true,
    });
    expect(tiling?.active).toBe(2);
    expect(tiling?.originalPosition).toBe(true);
    const inches = { ...initialTilingForm(null, 'inch'), tileWidth: '24', tileHeight: '12' };
    const spec = tilingFromForm(inches, 'inch', null)?.spec;
    expect(spec?.tileWidth).toBeCloseTo(609.6, 9);
    expect(spec?.tileHeight).toBeCloseTo(304.8, 9);
    expect(usedFields('feedX')).toEqual(['tileWidth', 'overlap']);
  });

  it('holds back a bad size or an overlap as big as a tile', () => {
    const state = { ...initialTilingForm(null, 'mm'), tileWidth: '0' };
    expect(validateTilingForm(state, 'mm').tileWidth).toBeDefined();
    expect(tilingFromForm(state, 'mm', null)).toBeNull();
    const wide = { ...initialTilingForm(null, 'mm'), overlap: '700' };
    expect(validateTilingForm(wide, 'mm').overlap).toBeDefined();
    const ignored = { ...initialTilingForm(null, 'mm'), method: 'feedY' as const, tileWidth: 'x' };
    expect(validateTilingForm(ignored, 'mm')).toEqual({});
  });
  it('needs the datum at the bottom left to tile', () => {
    expect(datumBlocksTiling(true, 'bottomLeft')).toBe(false);
    expect(datumBlocksTiling(true, 'centre')).toBe(true);
    expect(datumBlocksTiling(true, 'topRight')).toBe(true);
    expect(datumBlocksTiling(false, 'centre')).toBe(false);
  });

  it('Delete All Tiling Data leaves the tiling of a new job, switched off', () => {
    const cleared = clearedTiling();
    expect(cleared.enabled).toBe(false);
    expect(cleared.active).toBe(0);
    expect(initialTilingForm(cleared, 'mm')).toEqual(initialTilingForm(null, 'mm'));
  });
});
