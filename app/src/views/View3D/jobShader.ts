// Material texture and lithophane shading for the surfaces drawn as plain
// meshes: the flat material block, the CPU built preview mesh (fields too
// big for the GPU surface) and rotary jobs wrapped round their axis. Each
// vertex carries where it lies on the job (u, v from 0 to 1 across it) and
// how much material is under it, as a `carveJob` attribute; the shader
// patch below reads it the way surfaceShader.ts reads the height textures.
// The preview check's colors come as a `carveMark` attribute (checkMarks.ts).

import type { IUniform, Material, Texture, WebGLProgramParametersWithUniforms } from 'three';
import { Vector2 } from 'three';

import { MARK_ATTRIBUTE } from './checkMarks';
import { FINISH_FRAGMENT_HEADER, finishUniforms } from './componentFinish';
import { withSliceTint, type SliceTintUniforms } from './sliceTint';

/** Name of the per vertex attribute: u, v across the job, then the
 * thickness fraction plus one (0 means no job coordinates, as on the walls
 * of through cuts). */
export const JOB_ATTRIBUTE = 'carveJob';

export interface JobUniforms {
  carveJobMap: IUniform<Texture | null>;
  carveJobMapOn: IUniform<boolean>;
  carveJobLitho: IUniform<number>;
  /** The grain of the job's material (a finish slot), 0 for none. */
  carveJobFinish: IUniform<number>;
  /** The job's width and height, mm, for placing the grain. */
  carveJobSize: IUniform<Vector2>;
  [name: string]: IUniform;
}

/** Uniforms with no texture, no grain and no lithophane. */
export function jobUniforms(): JobUniforms {
  return {
    carveJobMap: { value: null },
    carveJobMapOn: { value: false },
    carveJobLitho: { value: -1 },
    carveJobFinish: { value: 0 },
    carveJobSize: { value: new Vector2(1, 1) },
    ...finishUniforms(),
  };
}

/** The job box the texture is stretched over, in job millimetres. */
export interface JobBox {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

/** The fraction of `value` across `min` to `max`. */
function across(value: number, min: number, max: number): number {
  return max - min > 0 ? (value - min) / (max - min) : 0;
}

/**
 * Job coordinates of flat scene `positions` (x is job x, z is minus job y,
 * y is height with the material top at `top` and its bottom at `bottom`),
 * moved by `offset` first, as the `carveJob` attribute.
 */
export function flatJobCoords(
  positions: ArrayLike<number>,
  box: JobBox,
  top: number,
  bottom: number,
  offset: readonly [number, number, number] = [0, 0, 0],
): Float32Array {
  const count = Math.floor(positions.length / 3);
  const out = new Float32Array(count * 3);
  for (let i = 0; i < count; i += 1) {
    const x = (positions[i * 3] ?? 0) + offset[0];
    const y = (positions[i * 3 + 1] ?? 0) + offset[1];
    const z = (positions[i * 3 + 2] ?? 0) + offset[2];
    out[i * 3] = across(x, box.minX, box.maxX);
    out[i * 3 + 1] = across(0 - z, box.minY, box.maxY);
    out[i * 3 + 2] = 1 + Math.min(Math.max(across(y, bottom, top), 0), 1);
  }
  return out;
}

const VERTEX_HEADER = /* glsl */ `
attribute vec3 carveJob;
attribute vec4 ${MARK_ATTRIBUTE};
varying vec3 vGrooveJob;
varying vec4 vGrooveMark;
`;

const VERTEX_PASS = /* glsl */ `
#include <begin_vertex>
vGrooveJob = carveJob;
vGrooveMark = ${MARK_ATTRIBUTE};
`;

const FRAGMENT_HEADER = /* glsl */ `
uniform sampler2D carveJobMap;
uniform bool carveJobMapOn;
uniform float carveJobLitho;
uniform int carveJobFinish;
uniform vec2 carveJobSize;
varying vec3 vGrooveJob;
varying vec4 vGrooveMark;
vec3 carveJobGlow = vec3( 0.0 );
vec3 carveJobLinear(vec3 c) {
  return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}
`;

// The picture over the job (or the material's grain), the preview check's
// mark, then the lithophane: the material as if lit from behind, white
// where thinnest and its own colour at full thickness, as the GPU surface
// draws it.
const COLOUR_FRAGMENT = /* glsl */ `
#include <color_fragment>
if ( vGrooveJob.z > 0.5 ) {
  if ( carveJobMapOn ) {
    diffuseColor.rgb = carveJobLinear( texture2D( carveJobMap, vGrooveJob.xy ).rgb );
  } else if ( carveJobFinish > 0 ) {
    diffuseColor.rgb *= carveFinishShade( carveJobFinish, vGrooveJob.xy * carveJobSize );
  }
  diffuseColor.rgb = mix( diffuseColor.rgb, vGrooveMark.rgb, 1.0 - vGrooveMark.a );
  if ( carveJobLitho >= 0.0 ) {
    float carveThick = clamp( vGrooveJob.z - 1.0, 0.0, 1.0 );
    float carveK = mix( 0.5, 8.0, clamp( carveJobLitho, 0.0, 1.0 ) );
    float carveLight = ( exp( -carveK * carveThick ) - exp( -carveK ) ) / ( 1.0 - exp( -carveK ) );
    carveJobGlow = mix( diffuseColor.rgb, vec3( 1.0 ), carveLight );
    diffuseColor.rgb *= 0.1;
  }
}
`;

const GLOW_FRAGMENT = /* glsl */ `
#include <emissivemap_fragment>
totalEmissiveRadiance += carveJobGlow;
`;

/**
 * Makes `material` draw the texture and lithophane of `uniforms` (shared,
 * so later changes reach the compiled program) on meshes with the
 * `carveJob` attribute, and the Slice Model slice of `tint` when given.
 */
export function patchJobMaterial<M extends Material>(
  material: M,
  uniforms: JobUniforms,
  tint?: SliceTintUniforms,
): M {
  material.onBeforeCompile = (shader: WebGLProgramParametersWithUniforms) => {
    if (tint !== undefined) {
      withSliceTint(shader, tint);
    }
    Object.assign(shader.uniforms, uniforms);
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', `#include <common>\n${VERTEX_HEADER}`)
      .replace('#include <begin_vertex>', VERTEX_PASS);
    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <common>',
        `#include <common>\n${FINISH_FRAGMENT_HEADER}\n${FRAGMENT_HEADER}`,
      )
      .replace('#include <color_fragment>', COLOUR_FRAGMENT)
      .replace('#include <emissivemap_fragment>', GLOW_FRAGMENT);
  };
  material.customProgramCacheKey = () => (tint === undefined ? 'carve-job' : 'carve-job-slice');
  return material;
}
