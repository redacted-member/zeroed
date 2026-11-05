// Pure form state for Create Vector Texture: row angle and spacing, waves,
// noise and the layer the vectors go on.

import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';

export interface VectorTextureState {
  /** Degrees, -90 to 90. */
  angle: string;
  maxSpacing: string;
  /** 0 (even) to 100 (random gaps). */
  spacingVariation: number;
  wavelength: string;
  amplitude: string;
  /** 0 to 100. */
  noise: number;
  onLayer: boolean;
  layerName: string;
}

export type VectorTextureField = 'angle' | 'maxSpacing' | 'wavelength' | 'amplitude';
export type VectorTextureErrors = Partial<Record<VectorTextureField, string>>;

export interface TextureParams {
  angle: number;
  maxSpacing: number;
  spacingVariation: number;
  wavelength: number;
  amplitude: number;
  noise: number;
}

export interface VectorTextureRequest {
  params: TextureParams;
  layerName?: string;
}

export function initialVectorTextureState(units: Units): VectorTextureState {
  return {
    angle: '0',
    maxSpacing: lengthText(units === 'inch' ? 5.08 : 5, units),
    spacingVariation: 0,
    wavelength: lengthText(units === 'inch' ? 50.8 : 50, units),
    amplitude: lengthText(0, units),
    noise: 0,
    onLayer: false,
    layerName: 'Vector Texture',
  };
}

export function convertVectorTextureUnits(
  state: VectorTextureState,
  from: Units,
  to: Units,
): VectorTextureState {
  return convertLengthFields(state, ['maxSpacing', 'wavelength', 'amplitude'], from, to);
}

export function validateVectorTexture(
  state: VectorTextureState,
  units: Units,
): VectorTextureErrors {
  const errors: VectorTextureErrors = {};
  const angle = numberValue(state.angle);
  const angleError = fieldError(angle, { min: -90, max: 90 });
  if (angleError !== undefined) {
    errors.angle = angleError;
  }
  const spacing = fieldError(lengthValue(state.maxSpacing, units), { positive: true });
  if (spacing !== undefined) {
    errors.maxSpacing = spacing;
  }
  const wavelength = fieldError(lengthValue(state.wavelength, units), { positive: true });
  if (wavelength !== undefined) {
    errors.wavelength = wavelength;
  }
  const amplitude = fieldError(lengthValue(state.amplitude, units), { min: 0 });
  if (amplitude !== undefined) {
    errors.amplitude = amplitude;
  }
  return errors;
}

/** The kernel request, or null while a field is wrong. */
export function vectorTextureRequest(
  state: VectorTextureState,
  units: Units,
): VectorTextureRequest | null {
  if (Object.keys(validateVectorTexture(state, units)).length > 0) {
    return null;
  }
  const request: VectorTextureRequest = {
    params: {
      angle: numberValue(state.angle) ?? 0,
      maxSpacing: lengthValue(state.maxSpacing, units) ?? 0,
      spacingVariation: state.spacingVariation,
      wavelength: lengthValue(state.wavelength, units) ?? 0,
      amplitude: lengthValue(state.amplitude, units) ?? 0,
      noise: state.noise,
    },
  };
  const name = state.layerName.trim();
  if (state.onLayer && name !== '') {
    request.layerName = name;
  }
  return request;
}
