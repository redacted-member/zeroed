import { MeshStandardMaterial, type WebGLProgramParametersWithUniforms } from 'three';
import { describe, expect, it } from 'vitest';

import { HEIGHT_NAN } from '../../bridge/heights';
import { patchSurfaceMaterial } from './surfaceShader';

function compiled(paint: boolean): WebGLProgramParametersWithUniforms {
  const uniforms = { carveLitho: { value: 0.5 } };
  const material = patchSurfaceMaterial(new MeshStandardMaterial(), uniforms, paint);
  const shader = {
    uniforms: {},
    vertexShader: '#include <common>\n#include <beginnormal_vertex>\n#include <begin_vertex>',
    fragmentShader:
      '#include <common>\n#include <clipping_planes_fragment>\n#include <color_fragment>\n#include <emissivemap_fragment>',
  } as unknown as WebGLProgramParametersWithUniforms;
  material.onBeforeCompile(shader, undefined as never);
  return shader;
}

describe('surface shader', () => {
  it('shades the painted surface as a lithophane when asked', () => {
    const shader = compiled(true);
    expect(shader.uniforms['carveLitho']).toEqual({ value: 0.5 });
    expect(shader.fragmentShader).toContain('carveLitho >= 0.0');
    expect(shader.fragmentShader).toContain('totalEmissiveRadiance += carveGlow;');
    // The glow comes after the colour it starts from.
    expect(shader.fragmentShader.indexOf('carveGlow = mix')).toBeGreaterThan(
      shader.fragmentShader.indexOf('#include <color_fragment>'),
    );
  });

  it('paints the check codes over the surface in the legend colors', () => {
    const shader = compiled(true);
    expect(shader.fragmentShader).toContain('uniform highp usampler2D carveCheck;');
    expect(shader.fragmentShader).toContain('if ( carveCheckOn && vGroovePaint > 0.5 )');
    // Gouges red: 0xd9 / 255.
    expect(shader.fragmentShader).toContain('carveCode == 1u ? vec3(0.8510, 0.2118, 0.2118)');
    // Over the paint, under the lithophane light.
    const check = shader.fragmentShader.indexOf('carveCheckOn &&');
    expect(check).toBeGreaterThan(shader.fragmentShader.indexOf('carvePaint != '));
    expect(check).toBeLessThan(shader.fragmentShader.indexOf('carveLitho >= 0.0'));
    expect(compiled(false).fragmentShader).not.toContain('carveCheckOn &&');
  });

  it('chars the surface where a laser burnt it, under the check colors', () => {
    const shader = compiled(true);
    expect(shader.fragmentShader).toContain('uniform highp sampler2D carveBurn;');
    expect(shader.fragmentShader).toContain('if ( carveBurnOn && vGroovePaint > 0.5 )');
    expect(shader.fragmentShader).toContain('texture2D( carveBurn,');
    const burn = shader.fragmentShader.indexOf('carveBurnOn &&');
    expect(burn).toBeGreaterThan(shader.fragmentShader.indexOf('carvePaint != '));
    expect(burn).toBeLessThan(shader.fragmentShader.indexOf('carveCheckOn &&'));
    expect(compiled(false).fragmentShader).not.toContain('carveBurnOn &&');
  });

  it('leaves the wireframe material unlit by the lithophane', () => {
    const shader = compiled(false);
    expect(shader.fragmentShader).not.toContain('totalEmissiveRadiance += carveGlow;');
  });

  it('lifts the grid by u16 codes decoded as base plus steps, the NaN code a hole', () => {
    const shader = compiled(true);
    for (const stage of [shader.vertexShader, shader.fragmentShader]) {
      expect(stage).toContain('uniform highp usampler2D carveTopCodes;');
      expect(stage).toContain('uniform highp usampler2D carveFloorCodes;');
      expect(stage).toContain(`q == ${HEIGHT_NAN}u ? vec2(0.0, 1.0)`);
      expect(stage).toContain('code.y + float(q) * code.z');
    }
    // The float textures stay for heights the kernel sends as f32.
    expect(shader.vertexShader).toContain('texelFetch(carveHeights, at, 0)');
    expect(shader.vertexShader).toContain('transformed.y = carveV.height;');
  });

  it('filters each sampled block and cuts the surface where half of it is cut through', () => {
    const shader = compiled(true);
    // The block round the cell, every cell up to eight a side.
    expect(shader.vertexShader).toContain('int stride = (s + 7) / 8;');
    expect(shader.vertexShader).toContain('int lo = -(s / 2);');
    expect(shader.vertexShader).toContain('sum / kept - carveTop');
    // The hole share goes to the fragment stage, which cuts at a half.
    expect(shader.vertexShader).toContain('vGrooveHole = carveV.hole;');
    expect(shader.fragmentShader).toContain('if ( vGrooveHole > 0.5 )');
    expect(shader.fragmentShader).not.toContain('vGrooveHole > 0.001');
  });

  it('lays the job material grain over the preview where no picture is', () => {
    const shader = compiled(true);
    expect(shader.fragmentShader).toContain('uniform int carveJobFinish;');
    expect(shader.fragmentShader).toContain(
      'carveFinishShade( carveJobFinish, ( vGrooveCell + 0.5 ) * carveCellSize )',
    );
  });
});
