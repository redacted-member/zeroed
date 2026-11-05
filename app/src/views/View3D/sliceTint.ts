// The Slice Model form's picked slice on the model surface: red from the
// slice's bottom to its top, green above it (the part the slice leaves
// off), untouched below. Heights are scene Y, so the patch works the same
// on the GPU surface, the CPU built mesh and the flat material block.

import type { IUniform, WebGLProgramParametersWithUniforms } from 'three';

export interface SliceTintUniforms {
  carveSliceOn: IUniform<boolean>;
  carveSliceLow: IUniform<number>;
  carveSliceHigh: IUniform<number>;
  [name: string]: IUniform;
}

/** Uniforms with no slice shown. */
export function sliceTintUniforms(): SliceTintUniforms {
  return {
    carveSliceOn: { value: false },
    carveSliceLow: { value: 0 },
    carveSliceHigh: { value: 0 },
  };
}

/** Shows the slice from scene height `low` to `high`, or none for null. */
export function setSliceTint(
  uniforms: SliceTintUniforms,
  band: { low: number; high: number } | null,
): void {
  uniforms.carveSliceOn.value = band !== null;
  uniforms.carveSliceLow.value = band?.low ?? 0;
  uniforms.carveSliceHigh.value = band?.high ?? 0;
}

/** Which colour a surface point at scene height `y` takes: the slice, what
 * lies above it, or neither. The shader below makes the same choice. */
export function sliceTintAt(
  y: number,
  band: { low: number; high: number } | null,
): 'slice' | 'above' | null {
  if (band === null) {
    return null;
  }
  if (y > band.high) {
    return 'above';
  }
  return y >= band.low ? 'slice' : null;
}

const VERTEX_HEADER = /* glsl */ `
varying float vGrooveSliceY;
`;

const VERTEX_PASS = /* glsl */ `
#include <project_vertex>
vGrooveSliceY = ( modelMatrix * vec4( transformed, 1.0 ) ).y;
`;

const FRAGMENT_HEADER = /* glsl */ `
uniform bool carveSliceOn;
uniform float carveSliceLow;
uniform float carveSliceHigh;
varying float vGrooveSliceY;
`;

// After every other colour change, so the slice shows over component
// colours and material pictures alike.
const COLOUR_FRAGMENT = /* glsl */ `
#include <color_fragment>
if ( carveSliceOn ) {
  if ( vGrooveSliceY > carveSliceHigh ) {
    diffuseColor.rgb = mix( diffuseColor.rgb, vec3( 0.05, 0.55, 0.08 ), 0.7 );
  } else if ( vGrooveSliceY >= carveSliceLow ) {
    diffuseColor.rgb = mix( diffuseColor.rgb, vec3( 0.75, 0.03, 0.03 ), 0.7 );
  }
}
`;

/**
 * Adds the slice colouring of `uniforms` (shared, so later changes reach
 * the compiled program) to `shader`. Run it before a material's other
 * patches: they keep `#include <color_fragment>` first in their own
 * replacements, so this colouring stays last.
 */
export function withSliceTint(
  shader: WebGLProgramParametersWithUniforms,
  uniforms: SliceTintUniforms,
): void {
  Object.assign(shader.uniforms, uniforms);
  shader.vertexShader = shader.vertexShader
    .replace('#include <common>', `#include <common>\n${VERTEX_HEADER}`)
    .replace('#include <project_vertex>', VERTEX_PASS);
  shader.fragmentShader = shader.fragmentShader
    .replace('#include <common>', `#include <common>\n${FRAGMENT_HEADER}`)
    .replace('#include <color_fragment>', COLOUR_FRAGMENT);
}
