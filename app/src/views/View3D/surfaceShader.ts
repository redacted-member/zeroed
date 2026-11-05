// Shader additions that lift the fixed surface grid by the heightfield
// textures. They patch the standard three.js materials through
// onBeforeCompile, so lighting, shading modes and the material finish stay
// those of the CPU mesh. Heights are job Z, uploaded as the kernel sent
// them: u16 codes in an unsigned integer texture, decoded here as
// `base + q * step` with the top code standing for NaN, or plain floats in a
// float texture. A NaN cell is cut through and drops to the floor, and
// triangles touching it are discarded. Component or machined colours are a
// palette: each cell's entry comes from an unsigned integer texture, its
// colour from the palette, and it is laid over the material by its alpha.

import type { IUniform, Material, WebGLProgramParametersWithUniforms } from 'three';

import { BLOCK_SAMPLES } from './blockSampling';
import { CHECK_COLOURS } from './checkColours';
import { HEIGHT_NAN } from '../../bridge/heights';
import { FINISH_FRAGMENT_HEADER, FINISH_METALNESS, FINISH_ROUGHNESS } from './componentFinish';
import { withSliceTint, type SliceTintUniforms } from './sliceTint';
import { ROLE } from './surfaceGrid';

/** Whether the surface is painted with palette colours: `palette` leaves
 * entry 0 (no component, an uncut cell) unpainted, `all` paints it too
 * (Surface Color). */
export const PAINT = { none: 0, palette: 1, all: 2 } as const;

/** The color a laser chars the material to where it burns at full strength. */
export const LASER_CHAR = '#20150d';

// Reading the heights, shared by both stages. carveTopCode and
// carveFloorCode are (coded, base, step): coded above 0.5 reads the codes
// texture, otherwise the float one. Without floor heights the underside is
// the material bottom, open where the top is. Each read answers the job Z
// in x and 1.0 in y where the cell is cut through.
const HEIGHT_READ = /* glsl */ `
uniform highp sampler2D carveHeights;
uniform highp sampler2D carveFloors;
uniform highp usampler2D carveTopCodes;
uniform highp usampler2D carveFloorCodes;
uniform vec3 carveTopCode;
uniform vec3 carveFloorCode;
uniform float carveTop;
uniform float carveFloor;
uniform bool carveHasFloor;

bool carveIsNan(float v) {
  uint bits = floatBitsToUint(v);
  return (bits & 0x7f800000u) == 0x7f800000u && (bits & 0x007fffffu) != 0u;
}
ivec2 carveSize() {
  return carveTopCode.x > 0.5 ? textureSize(carveTopCodes, 0) : textureSize(carveHeights, 0);
}
ivec2 carveClamp(ivec2 c) {
  return clamp(c, ivec2(0), carveSize() - 1);
}
vec2 carveDecode(uint q, vec3 code) {
  return q == ${HEIGHT_NAN}u ? vec2(0.0, 1.0) : vec2(code.y + float(q) * code.z, 0.0);
}
vec2 carveFloat(float z) {
  return carveIsNan(z) ? vec2(0.0, 1.0) : vec2(z, 0.0);
}
vec2 carveTopRaw(ivec2 c) {
  ivec2 at = carveClamp(c);
  return carveTopCode.x > 0.5
    ? carveDecode(texelFetch(carveTopCodes, at, 0).r, carveTopCode)
    : carveFloat(texelFetch(carveHeights, at, 0).r);
}
vec2 carveFloorRaw(ivec2 c) {
  if (!carveHasFloor) {
    return vec2(carveTop + carveFloor, carveTopRaw(c).y);
  }
  ivec2 at = carveClamp(c);
  return carveFloorCode.x > 0.5
    ? carveDecode(texelFetch(carveFloorCodes, at, 0).r, carveFloorCode)
    : carveFloat(texelFetch(carveFloors, at, 0).r);
}
`;

// Each vertex stands for the block of carveStep by carveStep cells round
// its cell (blockSampling.ts says which cells): its height is the mean of
// the block's kept cells, so detail finer than the grid (the scallops of a
// small stepover) averages out instead of aliasing into ridges, and its
// normal comes from the filtered neighbours a step away. The share of the
// block cut through goes to the fragment stage, which cuts the surface
// where it passes a half; a block cut right through sits at the height of
// the blocks round it so the surface stays flat up to that edge.
const VERTEX_HEADER = /* glsl */ `
${HEIGHT_READ}
uniform float carveStep;
uniform float carveCellSize;
attribute vec2 carveCell;
attribute float carveRole;
varying vec2 vGrooveCell;
varying float vGroovePaint;
varying float vGrooveHole;

/** Mean height above the top (x) and cut through share (y) of the block round cell c; x is 0 when all of it is cut. */
vec2 carveBlock(ivec2 c, bool under) {
  int s = max(int(carveStep), 1);
  int stride = (s + ${BLOCK_SAMPLES - 1}) / ${BLOCK_SAMPLES};
  int lo = -(s / 2);
  int hi = lo + s - 1;
  float sum = 0.0;
  float kept = 0.0;
  float n = 0.0;
  for (int dy = lo; dy <= hi; dy += stride) {
    for (int dx = lo; dx <= hi; dx += stride) {
      vec2 z = under ? carveFloorRaw(c + ivec2(dx, dy)) : carveTopRaw(c + ivec2(dx, dy));
      n += 1.0;
      if (z.y < 0.5) {
        sum += z.x;
        kept += 1.0;
      }
    }
  }
  return vec2(kept > 0.0 ? sum / kept - carveTop : 0.0, (n - kept) / n);
}
vec2 carveSurfaceAt(ivec2 c, bool under) {
  return carveBlock(c, under);
}
/** The vertex: height above the top (x), cut through share (y), and the normal. */
struct CarveVertex {
  float height;
  float hole;
  vec3 normal;
};
CarveVertex carveVertex(ivec2 c, bool under) {
  ivec2 last = carveSize() - 1;
  int s = max(int(carveStep), 1);
  ivec2 xl = ivec2(max(c.x - s, 0), c.y);
  ivec2 xr = ivec2(min(c.x + s, last.x), c.y);
  ivec2 yd = ivec2(c.x, max(c.y - s, 0));
  ivec2 yu = ivec2(c.x, min(c.y + s, last.y));
  vec2 here = carveSurfaceAt(c, under);
  vec2 left = carveSurfaceAt(xl, under);
  vec2 right = carveSurfaceAt(xr, under);
  vec2 down = carveSurfaceAt(yd, under);
  vec2 up = carveSurfaceAt(yu, under);
  float height = here.x;
  if (here.y >= 1.0) {
    float sum = 0.0;
    float n = 0.0;
    if (left.y < 1.0) { sum += left.x; n += 1.0; }
    if (right.y < 1.0) { sum += right.x; n += 1.0; }
    if (down.y < 1.0) { sum += down.x; n += 1.0; }
    if (up.y < 1.0) { sum += up.x; n += 1.0; }
    height = n > 0.0 ? sum / n : carveFloor;
  }
  // A neighbour mostly cut through says nothing about the slope: lean on this side alone.
  float hl = left.y > 0.5 ? height : left.x;
  float hr = right.y > 0.5 ? height : right.x;
  float hd = down.y > 0.5 ? height : down.x;
  float hu = up.y > 0.5 ? height : up.x;
  int xFrom = left.y > 0.5 ? c.x : xl.x;
  int xTo = right.y > 0.5 ? c.x : xr.x;
  int yFrom = down.y > 0.5 ? c.y : yd.y;
  int yTo = up.y > 0.5 ? c.y : yu.y;
  float dx = float(max(xTo - xFrom, 1)) * carveCellSize;
  float dy = float(max(yTo - yFrom, 1)) * carveCellSize;
  float slopeX = (hr - hl) / dx;
  float slopeY = (hu - hd) / dy;
  // Scene z runs against job y.
  return CarveVertex(height, here.y, normalize(vec3(-slopeX, 1.0, slopeY)));
}
bool carveIsSurface() {
  return abs( carveRole - ${ROLE.top.toFixed(1)} ) < 0.5 || abs( carveRole - ${ROLE.under.toFixed(1)} ) < 0.5;
}
bool carveIsUnder() {
  return carveRole > ${(ROLE.top + 0.5).toFixed(1)};
}
// Made once per vertex: the normal and the position stages both read it.
CarveVertex carveHere;
bool carveHaveHere = false;
CarveVertex carveThisVertex() {
  if (!carveHaveHere) {
    carveHere = carveVertex( ivec2( carveCell + 0.5 ), carveIsUnder() );
    carveHaveHere = true;
  }
  return carveHere;
}
/** The top (or underside) height of a wall's edge: the block's mean, the floor where it is cut through. */
float carveEdgeAt(ivec2 c, bool under) {
  vec2 z = carveSurfaceAt(c, under);
  return z.y > 0.5 ? carveFloor : z.x;
}
`;

const BEGIN_NORMAL = /* glsl */ `
vec3 objectNormal = vec3( normal );
if ( carveIsSurface() ) {
  CarveVertex carveV = carveThisVertex();
  objectNormal = carveIsUnder() ? -carveV.normal : carveV.normal;
}
#ifdef USE_TANGENT
  vec3 objectTangent = vec3( tangent.xyz );
#endif
`;

const BEGIN_VERTEX = /* glsl */ `
vec3 transformed = vec3( position );
{
  ivec2 carveAt = ivec2( carveCell + 0.5 );
  bool carveUnder = carveIsUnder();
  if ( carveIsSurface() ) {
    CarveVertex carveV = carveThisVertex();
    transformed.y = carveV.height;
    vGrooveHole = carveV.hole;
  } else {
    transformed.y = carveEdgeAt( carveAt, carveUnder );
    vGrooveHole = 0.0;
  }
  vGroovePaint = abs( carveRole - ${ROLE.top.toFixed(1)} ) < 0.5 ? 1.0 : 0.0;
  vGrooveCell = carveCell;
}
#ifdef USE_ALPHAHASH
  vPosition = vec3( position );
#endif
`;

const FRAGMENT_HEADER = /* glsl */ `
${HEIGHT_READ}
uniform float carveLitho;
uniform sampler2D carveMaterialMap;
uniform bool carveMaterialOn;
uniform int carveJobFinish;
vec3 carveGlow = vec3( 0.0 );
uniform int carvePaint;
uniform highp usampler2D carveIndex;
uniform highp sampler2D carvePalette;
uniform highp sampler2D carveFinishPalette;
uniform bool carveFinishOn;
uniform float carveCellSize;
uniform highp usampler2D carveCheck;
uniform bool carveCheckOn;
uniform highp sampler2D carveBurn;
uniform bool carveBurnOn;
varying vec2 vGrooveCell;
varying float vGroovePaint;
varying float vGrooveHole;

vec3 carveLinear(vec3 c) {
  return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}
`;

// The surface ends where the cut through share, interpolated between the
// sampled blocks, passes a half: the same contour the hole walls follow.
const HOLE_DISCARD = /* glsl */ `
if ( vGrooveHole > 0.5 ) {
  discard;
}
`;

// A material texture picture stretched over the job: the grid's cells run
// across the whole job, so the cell over the grid size is the picture's uv.
// Without a picture the job material's grain lies over its colour.
const MATERIAL_FRAGMENT = /* glsl */ `
if ( carveMaterialOn && vGroovePaint > 0.5 ) {
  vec2 carveUv = ( vGrooveCell + 0.5 ) / vec2( carveSize() );
  diffuseColor.rgb = carveLinear( texture2D( carveMaterialMap, carveUv ).rgb );
} else if ( carveJobFinish > 0 && vGroovePaint > 0.5 ) {
  diffuseColor.rgb *= carveFinishShade( carveJobFinish, ( vGrooveCell + 0.5 ) * carveCellSize );
}
`;

const PAINT_FRAGMENT = /* glsl */ `
if ( carvePaint != ${PAINT.none} && vGroovePaint > 0.5 ) {
  ivec2 carveAt = ivec2( vGrooveCell + 0.5 );
  uint carveEntry = texelFetch( carveIndex, carveAt, 0 ).r;
  int carveLast = textureSize( carvePalette, 0 ).x - 1;
  vec4 carveColour = carveEntry == 0u && carvePaint != ${PAINT.all} ? vec4( 0.0 ) : texelFetch( carvePalette, ivec2( min( int( carveEntry ), carveLast ), 0 ), 0 );
  if ( carveColour.a > 0.0 ) {
    diffuseColor.rgb = mix( diffuseColor.rgb, carveLinear( carveColour.rgb ), carveColour.a );
    // A preview material's grain and finish over its colour.
    if ( carveFinishOn ) {
      vec4 carveFin = texelFetch( carveFinishPalette, ivec2( min( int( carveEntry ), carveLast ), 0 ), 0 );
      if ( carveFin.a > 0.5 ) {
        diffuseColor.rgb *= carveFinishShade( int( carveFin.r * 255.0 + 0.5 ), ( vGrooveCell + 0.5 ) * carveCellSize );
        carveFinishRough = carveFin.g;
        carveFinishMetal = carveFin.b;
      }
    }
  }
}
`;

/** A '#rrggbb' colour as a GLSL vec3 in sRGB. */
function glslColour(hex: string): string {
  const channel = (at: number) => (parseInt(hex.slice(at, at + 2), 16) / 255).toFixed(4);
  return `vec3(${channel(1)}, ${channel(3)}, ${channel(5)})`;
}

// Laser marks of a toolpath preview: the surface charred toward
// LASER_CHAR as dark as the beam burnt each cell (filtered, so marks
// narrower than a pixel blend in rather than flicker), under the paint the
// preview check lays over it.
const BURN_FRAGMENT = /* glsl */ `
if ( carveBurnOn && vGroovePaint > 0.5 ) {
  float carveChar = texture2D( carveBurn, ( vGrooveCell + 0.5 ) / vec2( textureSize( carveBurn, 0 ) ) ).r;
  if ( carveChar > 0.0 ) {
    diffuseColor.rgb = mix( diffuseColor.rgb, carveLinear( ${glslColour(LASER_CHAR)} ), carveChar );
  }
}
`;

// The preview check: gouged cells red, leftover blue, undercuts violet,
// over whatever else paints the surface.
const CHECK_FRAGMENT = /* glsl */ `
if ( carveCheckOn && vGroovePaint > 0.5 ) {
  uint carveCode = texelFetch( carveCheck, ivec2( vGrooveCell + 0.5 ), 0 ).r;
  if ( carveCode != 0u ) {
    vec3 carveMark = carveCode == 1u ? ${glslColour(CHECK_COLOURS.gouge)}
      : carveCode == 2u ? ${glslColour(CHECK_COLOURS.leftover)}
      : ${glslColour(CHECK_COLOURS.undercut)};
    diffuseColor.rgb = mix( diffuseColor.rgb, carveLinear( carveMark ), 0.85 );
  }
}
`;

// Lithophane: the material as if lit from behind, brightest (white) where
// it is thinnest and its own colour at full thickness. The light falls off
// through the material faster the higher the strength, and shows as
// emitted light so the front lighting hardly matters.
const LITHO_FRAGMENT = /* glsl */ `
if ( carveLitho >= 0.0 && vGroovePaint > 0.5 ) {
  ivec2 carveAt = ivec2( vGrooveCell + 0.5 );
  vec2 carveZ = carveTopRaw( carveAt );
  vec2 carveUnder = carveFloorRaw( carveAt );
  float carveFull = max( -carveFloor, 1e-6 );
  float carveBase = carveUnder.y > 0.5 ? carveTop + carveFloor : carveUnder.x;
  float carveThick = carveZ.y > 0.5 ? 0.0 : clamp( ( carveZ.x - carveBase ) / carveFull, 0.0, 1.0 );
  float carveK = mix( 0.5, 8.0, clamp( carveLitho, 0.0, 1.0 ) );
  float carveLight = ( exp( -carveK * carveThick ) - exp( -carveK ) ) / ( 1.0 - exp( -carveK ) );
  carveGlow = mix( diffuseColor.rgb, vec3( 1.0 ), carveLight );
  diffuseColor.rgb *= 0.1;
}
`;

const GLOW_FRAGMENT = /* glsl */ `
#include <emissivemap_fragment>
totalEmissiveRadiance += carveGlow;
`;

export type SurfaceUniforms = Record<string, IUniform>;

/**
 * Makes `material` draw the surface grid lifted by the textures in
 * `uniforms` (shared, so later changes reach the compiled program). With
 * `paint` the fragment shader also paints component or machined colours,
 * and the Slice Model slice of `tint` when given.
 */
export function patchSurfaceMaterial<M extends Material>(
  material: M,
  uniforms: SurfaceUniforms,
  paint: boolean,
  tint?: SliceTintUniforms,
): M {
  material.onBeforeCompile = (shader: WebGLProgramParametersWithUniforms) => {
    if (paint && tint !== undefined) {
      withSliceTint(shader, tint);
    }
    Object.assign(shader.uniforms, uniforms);
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', `#include <common>\n${VERTEX_HEADER}`)
      .replace('#include <beginnormal_vertex>', BEGIN_NORMAL)
      .replace('#include <begin_vertex>', BEGIN_VERTEX);
    let fragment = shader.fragmentShader
      .replace('#include <common>', `#include <common>\n${FRAGMENT_HEADER}`)
      .replace(
        '#include <clipping_planes_fragment>',
        `#include <clipping_planes_fragment>\n${HOLE_DISCARD}`,
      );
    if (paint) {
      fragment = fragment
        .replace('#include <common>', `#include <common>\n${FINISH_FRAGMENT_HEADER}`)
        .replace('#include <roughnessmap_fragment>', FINISH_ROUGHNESS)
        .replace('#include <metalnessmap_fragment>', FINISH_METALNESS)
        .replace(
          '#include <color_fragment>',
          `#include <color_fragment>\n${MATERIAL_FRAGMENT}\n${PAINT_FRAGMENT}\n${BURN_FRAGMENT}\n${CHECK_FRAGMENT}\n${LITHO_FRAGMENT}`,
        )
        .replace('#include <emissivemap_fragment>', GLOW_FRAGMENT);
    }
    shader.fragmentShader = fragment;
  };
  const slice = paint && tint !== undefined ? '-slice' : '';
  material.customProgramCacheKey = () => (paint ? 'carve-surface-paint' : 'carve-surface') + slice;
  return material;
}
