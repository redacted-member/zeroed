// The surface finish of components given a preview material (Use
// Material): the material's grain or texture laid over the component's
// colour, and its roughness and shine. The finishes are pictures made here,
// one layer each of an array texture that repeats across the job, and each
// palette entry of the component colours says which layer and finish it
// takes. The GPU surface reads them per cell, the CPU mesh per vertex.

import type { IUniform, Material, WebGLProgramParametersWithUniforms } from 'three';
import {
  DataArrayTexture,
  LinearFilter,
  LinearMipmapLinearFilter,
  RedFormat,
  RepeatWrapping,
  UnsignedByteType,
} from 'three';

import type { Heightfield } from '../../bridge/cam';
import type { PaletteColours } from '../../bridge/componentTree';
import type { Material3d } from '../../bridge/prefs';
import { materialLook } from './appearance';
import { colourAt } from './componentColours';
import { withSliceTint, type SliceTintUniforms } from './sliceTint';

/** Preview materials with a finish, in layer order (layer = slot - 1). */
export const FINISHES = [
  'pine',
  'oak',
  'walnut',
  'maple',
  'mdf',
  'acrylic',
  'aluminium',
] as const satisfies readonly Material3d[];

export type Finish = (typeof FINISHES)[number];

/** Side of each finish picture, pixels. */
export const FINISH_SIZE = 256;
/** How far one finish picture reaches across the job before it repeats, mm. */
export const FINISH_TILE_MM = 160;
/** How far the grain may lighten or darken the colour, as a fraction. */
const GRAIN_DEPTH = 0.28;

/** The slot of a material name: 0 for none, else its layer plus one. */
export function finishSlot(name: string | null | undefined): number {
  const at = FINISHES.findIndex((f) => f === name);
  return at + 1;
}

/**
 * Per palette entry, RGBA bytes: the finish slot, roughness and metalness
 * (0 to 255 for 0 to 1) and 255 where the entry has a finish. Null when no
 * entry has one, so the plain colours show.
 */
export function finishPalette(
  colours: Pick<PaletteColours, 'palette' | 'materials'>,
): Uint8Array | null {
  const entries = Math.floor(colours.palette.length / 4);
  const materials = colours.materials ?? [];
  const out = new Uint8Array(Math.max(entries, 1) * 4);
  let any = false;
  for (let entry = 0; entry < entries; entry += 1) {
    const slot = finishSlot(materials[entry]);
    const finish = FINISHES[slot - 1];
    if (finish === undefined) {
      continue;
    }
    const look = materialLook(finish, '', '');
    out.set(
      [slot, Math.round(look.roughness * 255), Math.round(look.metalness * 255), 255],
      entry * 4,
    );
    any = true;
  }
  return any ? out : null;
}

/**
 * Per vertex slot, roughness and metalness (three floats, slot 0 for none)
 * for a heightfield mesh whose first vertices are the top samples at field
 * columns `xs` and rows `ys`, row by row. Null when no cell has a finish or
 * the colours do not cover the field.
 */
export function vertexFinish(
  field: Pick<Heightfield, 'width' | 'height'>,
  colours: PaletteColours,
  xs: readonly number[],
  ys: readonly number[],
  vertexCount: number,
): Float32Array | null {
  const palette = finishPalette(colours);
  if (
    palette === null ||
    colours.width !== field.width ||
    colours.height !== field.height ||
    colours.index.length < field.width * field.height
  ) {
    return null;
  }
  const out = new Float32Array(vertexCount * 3);
  let v = 0;
  for (const cy of ys) {
    for (const cx of xs) {
      if (v >= vertexCount) {
        return out;
      }
      const at = colourAt(colours, cy * field.width + cx);
      if ((palette[at + 3] ?? 0) !== 0) {
        out[v * 3] = palette[at] ?? 0;
        out[v * 3 + 1] = (palette[at + 1] ?? 0) / 255;
        out[v * 3 + 2] = (palette[at + 2] ?? 0) / 255;
      }
      v += 1;
    }
  }
  return out;
}

/** A repeatable pseudo random value from 0 to 1 for a lattice point. */
function hash(x: number, y: number, seed: number): number {
  let h = Math.imul(x, 374761393) ^ Math.imul(y, 668265263) ^ Math.imul(seed, 2147483647);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

/**
 * Smooth noise from 0 to 1 over the unit square that repeats every
 * `cellsX` by `cellsY` lattice cells, so a picture made of it tiles.
 */
function tiledNoise(u: number, v: number, cellsX: number, cellsY: number, seed: number): number {
  const x = u * cellsX;
  const y = v * cellsY;
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const fx = x - x0;
  const fy = y - y0;
  const sx = fx * fx * (3 - 2 * fx);
  const sy = fy * fy * (3 - 2 * fy);
  const wrap = (n: number, m: number) => ((n % m) + m) % m;
  const at = (i: number, j: number) => hash(wrap(i, cellsX), wrap(j, cellsY), seed);
  const top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * sx;
  const bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * sx;
  return top + (bottom - top) * sy;
}

interface Grain {
  /** Growth rings across the picture (along job Y); a whole number so it tiles. */
  rings: number;
  /** How far the rings wander, in rings. */
  warp: number;
  /** Strength of the rings, 0 to 1. */
  ringDepth: number;
  /** Strength of the fine streaks along the grain, 0 to 1. */
  streaks: number;
  seed: number;
}

const WOOD: Readonly<Record<'pine' | 'oak' | 'walnut' | 'maple', Grain>> = {
  pine: { rings: 7, warp: 1.6, ringDepth: 0.85, streaks: 0.25, seed: 11 },
  oak: { rings: 11, warp: 1.1, ringDepth: 0.6, streaks: 0.55, seed: 23 },
  walnut: { rings: 9, warp: 2.2, ringDepth: 0.55, streaks: 0.45, seed: 37 },
  maple: { rings: 5, warp: 0.9, ringDepth: 0.35, streaks: 0.2, seed: 41 },
};

/** Brightness factor (1 unchanged) of a wood grain at picture point u, v. */
function woodShade(grain: Grain, u: number, v: number): number {
  const wander = tiledNoise(u, v, 3, 4, grain.seed) - 0.5;
  const phase = v * grain.rings + wander * grain.warp;
  const ring = 0.5 + 0.5 * Math.sin(2 * Math.PI * phase);
  // Late wood is a narrow dark band, early wood wide and light.
  const band = ring ** 3;
  const streak = tiledNoise(u, v, 4, 96, grain.seed + 1) - 0.5;
  return 1 + GRAIN_DEPTH * (grain.ringDepth * (0.35 - band) + grain.streaks * streak);
}

/** Brightness factor of `finish` at picture point u, v (0 to 1 each). */
export function finishShade(finish: Finish, u: number, v: number): number {
  switch (finish) {
    case 'pine':
    case 'oak':
    case 'walnut':
    case 'maple':
      return woodShade(WOOD[finish], u, v);
    case 'mdf':
      // Pressed fibres: a fine even speckle.
      return 1 + 0.12 * (tiledNoise(u, v, 128, 128, 5) - 0.5);
    case 'acrylic':
      return 1;
    case 'aluminium':
      // Brushed along X: long streaks, fine across.
      return 1 + 0.16 * (tiledNoise(u, v, 2, 160, 7) - 0.5);
  }
}

/** The finish pictures, one `size` square layer per finish, bytes where
 * 128 leaves the colour unchanged. */
export function finishLayers(size = FINISH_SIZE): Uint8Array {
  const out = new Uint8Array(size * size * FINISHES.length);
  FINISHES.forEach((finish, layer) => {
    const base = layer * size * size;
    for (let j = 0; j < size; j += 1) {
      for (let i = 0; i < size; i += 1) {
        const shade = finishShade(finish, (i + 0.5) / size, (j + 0.5) / size);
        out[base + j * size + i] = Math.min(255, Math.max(0, Math.round(shade * 128)));
      }
    }
  });
  return out;
}

let shared: DataArrayTexture | null = null;

/** The finish pictures as an array texture that repeats; made once. */
export function finishTexture(): DataArrayTexture {
  if (shared === null) {
    const texture = new DataArrayTexture(finishLayers(), FINISH_SIZE, FINISH_SIZE, FINISHES.length);
    texture.format = RedFormat;
    texture.type = UnsignedByteType;
    texture.wrapS = RepeatWrapping;
    texture.wrapT = RepeatWrapping;
    texture.magFilter = LinearFilter;
    texture.minFilter = LinearMipmapLinearFilter;
    texture.generateMipmaps = true;
    texture.needsUpdate = true;
    shared = texture;
  }
  return shared;
}

/** Uniforms the finish shader code reads. */
export interface FinishUniforms {
  carveFinishMap: IUniform<DataArrayTexture>;
  carveFinishTile: IUniform<number>;
  [name: string]: IUniform;
}

export function finishUniforms(): FinishUniforms {
  return {
    carveFinishMap: { value: finishTexture() },
    carveFinishTile: { value: FINISH_TILE_MM },
  };
}

/**
 * Fragment shader declarations: `carveFinishShade(slot, mm)` is the
 * brightness factor of finish `slot` at job point `mm`, and the finish's
 * roughness and metalness, once set, replace the material's.
 */
export const FINISH_FRAGMENT_HEADER = /* glsl */ `
uniform highp sampler2DArray carveFinishMap;
uniform float carveFinishTile;
float carveFinishRough = -1.0;
float carveFinishMetal = -1.0;
float carveFinishShade(int slot, vec2 mm) {
  float shade = texture( carveFinishMap, vec3( mm / carveFinishTile, float( slot - 1 ) ) ).r;
  return shade * 255.0 / 128.0;
}
`;

/** Replaces the roughness and metalness chunks so a finish can set them. */
export const FINISH_ROUGHNESS = /* glsl */ `
#include <roughnessmap_fragment>
if ( carveFinishRough >= 0.0 ) {
  roughnessFactor = carveFinishRough;
}
`;

export const FINISH_METALNESS = /* glsl */ `
#include <metalnessmap_fragment>
if ( carveFinishMetal >= 0.0 ) {
  metalnessFactor = carveFinishMetal;
}
`;

/** Adds the finish chunks to a fragment shader already painting `apply`
 * after its colour chunk. */
export function withFinish(fragment: string, apply: string): string {
  return fragment
    .replace('#include <common>', `#include <common>\n${FINISH_FRAGMENT_HEADER}`)
    .replace('#include <color_fragment>', `#include <color_fragment>\n${apply}`)
    .replace('#include <roughnessmap_fragment>', FINISH_ROUGHNESS)
    .replace('#include <metalnessmap_fragment>', FINISH_METALNESS);
}

/** Name of the CPU mesh's per vertex finish: slot, roughness, metalness. */
export const FINISH_ATTRIBUTE = 'carveFinish';

// Scene x is job x and scene z is minus job y on the flat meshes.
const VERTEX_FINISH = /* glsl */ `
#include <begin_vertex>
vGrooveFinish = carveFinish;
vGrooveFinishMm = vec2( position.x, -position.z );
`;

const APPLY_VERTEX_FINISH = /* glsl */ `
if ( vGrooveFinish.x > 0.5 ) {
  diffuseColor.rgb *= carveFinishShade( int( vGrooveFinish.x + 0.5 ), vGrooveFinishMm );
  carveFinishRough = vGrooveFinish.y;
  carveFinishMetal = vGrooveFinish.z;
}
`;

/**
 * Makes `material` (the CPU mesh's vertex coloured material) draw the
 * finish of the `carveFinish` attribute over its colours, and the Slice
 * Model slice of `tint` when given.
 */
export function patchFinishMaterial<M extends Material>(
  material: M,
  uniforms: FinishUniforms,
  tint?: SliceTintUniforms,
): M {
  material.onBeforeCompile = (shader: WebGLProgramParametersWithUniforms) => {
    if (tint !== undefined) {
      withSliceTint(shader, tint);
    }
    Object.assign(shader.uniforms, uniforms);
    shader.vertexShader = shader.vertexShader
      .replace(
        '#include <common>',
        `#include <common>\nattribute vec3 carveFinish;\nvarying vec3 vGrooveFinish;\nvarying vec2 vGrooveFinishMm;`,
      )
      .replace('#include <begin_vertex>', VERTEX_FINISH);
    shader.fragmentShader = withFinish(
      shader.fragmentShader.replace(
        '#include <common>',
        `#include <common>\nvarying vec3 vGrooveFinish;\nvarying vec2 vGrooveFinishMm;`,
      ),
      APPLY_VERTEX_FINISH,
    );
  };
  material.customProgramCacheKey = () =>
    tint === undefined ? 'carve-finish' : 'carve-finish-slice';
  return material;
}
