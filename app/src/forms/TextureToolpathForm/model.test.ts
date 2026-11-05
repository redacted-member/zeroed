import { describe, expect, it } from 'vitest';

import { strings } from '../../lib/strings';
import {
  MAX_STEPOVER_VARIATION,
  convertTextureUnits,
  initialTextureState,
  loadTextureSettings,
  partOf,
  textureParams,
  textureSettingsFrom,
  textureSettingsText,
  textureStateFrom,
  validateTexture,
} from './model';

describe('texture toolpath form model', () => {
  it('sends the settings with minimums as percentages', () => {
    expect(textureParams(initialTextureState('mm'), 'mm')).toEqual({
      angleDeg: 0,
      angleVariationDeg: 0,
      strokeLength: 10,
      lengthVariation: 0.5,
      spacing: 3,
      stepoverVariationPercent: 25,
      startDepth: 0,
      depth: 4,
      depthVariation: 0.5,
      seed: 1,
      minLengthPercent: 50,
      minDepthPercent: 50,
      maxOverlapPercent: 30,
      overlapVariationPercent: 50,
      boundaryOffset: 0,
      vectorsAsPattern: false,
      ramp: true,
    });
  });

  it('rejects values the kernel would refuse', () => {
    const base = initialTextureState('mm');
    expect(validateTexture(base, 'mm')).toEqual({});
    expect(validateTexture({ ...base, strokeLength: '0' }, 'mm').strokeLength).toBe(
      strings.form.errorPositive,
    );
    expect(validateTexture({ ...base, spacing: '-1' }, 'mm').spacing).toBeDefined();
    expect(validateTexture({ ...base, depth: 'x' }, 'mm').depth).toBe(strings.form.errorNumber);
    expect(validateTexture({ ...base, angle: '91' }, 'mm').angle).toBeDefined();
    expect(validateTexture({ ...base, minLength: '0' }, 'mm').minLength).toBeDefined();
    expect(validateTexture({ ...base, minDepth: '101' }, 'mm').minDepth).toBeDefined();
    expect(validateTexture({ ...base, maxOverlap: '-1' }, 'mm').maxOverlap).toBeDefined();
    expect(validateTexture({ ...base, boundaryOffset: '-1' }, 'mm').boundaryOffset).toBeDefined();
    expect(validateTexture({ ...base, startDepth: '-1' }, 'mm').startDepth).toBeDefined();
    expect(
      validateTexture({ ...base, stepoverVariation: String(MAX_STEPOVER_VARIATION + 1) }, 'mm')
        .stepoverVariation,
    ).toBeDefined();
  });

  it('checks only the depths with a vector pattern', () => {
    const base = { ...initialTextureState('mm'), vectorsAsPattern: true, spacing: '0' };
    expect(validateTexture(base, 'mm')).toEqual({});
    const params = textureParams({ ...base, ramp: false }, 'mm');
    expect(params?.vectorsAsPattern).toBe(true);
    expect(params?.ramp).toBe(false);
  });

  it('converts lengths and round trips', () => {
    const state = { ...initialTextureState('inch'), boundaryOffset: '0.1', angle: '45' };
    const params = textureParams(state, 'inch');
    expect(params?.boundaryOffset).toBeCloseTo(2.54);
    if (params !== null) {
      expect(textureParams(textureStateFrom(params, 'T', 'inch'), 'inch')).toEqual(params);
    }
    const mm = convertTextureUnits(state, 'inch', 'mm');
    expect(Number(mm.boundaryOffset)).toBeCloseTo(2.54);
  });

  it('reads toolpaths made with the older variation settings', () => {
    const old = {
      angleDeg: 10,
      angleVariationDeg: 12,
      strokeLength: 10,
      lengthVariation: 0.3,
      spacing: 3,
      depth: 1,
      depthVariation: 0.25,
      seed: 4,
    };
    const state = textureStateFrom(old, 'Old', 'mm');
    expect(state.minDepth).toBe('75');
    expect(state.minLength).toBe('70');
    expect(state.maxOverlap).toBe('0');
    // They moved strokes a tenth of the stepover and started at the surface.
    expect(state.stepoverVariation).toBe('10');
    expect(state.startDepth).toBe('0');
    expect(textureParams(state, 'mm')?.angleVariationDeg).toBe(12);
    // The seed has no control; a toolpath keeps its own pattern.
    expect(textureParams(state, 'mm')?.seed).toBe(4);
  });

  it('shows the minimums and the variation as lengths beside their sliders', () => {
    expect(partOf('4', '50', 'mm')).toBe('2');
    expect(partOf('x', '50', 'mm')).toBeNull();
  });

  it('saves the settings to a file and loads them back under the form name', () => {
    const state = { ...initialTextureState('mm'), startDepth: '1', stepoverVariation: '40' };
    const params = textureParams(state, 'mm');
    expect(params).not.toBeNull();
    if (params === null) {
      return;
    }
    const loaded = textureSettingsFrom(textureSettingsText(params));
    expect(loaded).toEqual(params);
    if (loaded !== null) {
      const next = loadTextureSettings(
        { ...initialTextureState('inch'), name: 'Texture 2' },
        loaded,
        'inch',
      );
      expect(next.name).toBe('Texture 2');
      expect(Number(next.startDepth)).toBeCloseTo(1 / 25.4, 4);
      expect(next.stepoverVariation).toBe('40');
    }
    expect(textureSettingsFrom('not json')).toBeNull();
    expect(textureSettingsFrom('{"format":"other","params":{}}')).toBeNull();
    expect(
      textureSettingsFrom('{"format":"carve-texture-settings","params":{"depth":"deep"}}'),
    ).toBeNull();
  });
});
