import { MeshStandardMaterial, type WebGLProgramParametersWithUniforms } from 'three';
import { describe, expect, it } from 'vitest';

import { flatJobCoords, jobUniforms, patchJobMaterial } from './jobShader';

describe('job shader coordinates', () => {
  it('maps flat scene points across the job and down the material', () => {
    const box = { minX: 0, minY: 0, maxX: 100, maxY: 50 };
    // Scene x is job x, scene z minus job y, scene y the height below the top.
    const coords = flatJobCoords([0, 0, 0, 100, -20, -50, 50, -10, -25], box, 0, -20);
    expect(Array.from(coords)).toEqual([0, 0, 2, 1, 1, 1, 0.5, 0.5, 1.5]);
  });

  it('adds the mesh offset first', () => {
    const box = { minX: -50, minY: -25, maxX: 50, maxY: 25 };
    const coords = flatJobCoords([-50, 9, 25], box, 0, -18, [0, -9, 0]);
    expect(Array.from(coords)).toEqual([0, 0, 2]);
  });

  it('lays the job material grain over the block when no picture is chosen', () => {
    const uniforms = jobUniforms();
    const material = patchJobMaterial(new MeshStandardMaterial(), uniforms);
    const shader = {
      uniforms: {},
      vertexShader: '#include <common>\n#include <begin_vertex>',
      fragmentShader:
        '#include <common>\n#include <color_fragment>\n#include <emissivemap_fragment>',
    } as unknown as WebGLProgramParametersWithUniforms;
    material.onBeforeCompile(shader, undefined as never);
    expect(shader.uniforms['carveJobFinish']).toBe(uniforms.carveJobFinish);
    expect(shader.uniforms['carveFinishMap']).toBeDefined();
    expect(shader.fragmentShader).toContain('carveFinishShade( carveJobFinish');
    // The grain is drawn only where no texture picture is.
    expect(shader.fragmentShader.indexOf('carveJobFinish > 0')).toBeGreaterThan(
      shader.fragmentShader.indexOf('if ( carveJobMapOn )'),
    );
  });

  it('mixes the preview check mark over the material before the lithophane', () => {
    const material = patchJobMaterial(new MeshStandardMaterial(), jobUniforms());
    const shader = {
      uniforms: {},
      vertexShader: '#include <common>\n#include <begin_vertex>',
      fragmentShader:
        '#include <common>\n#include <color_fragment>\n#include <emissivemap_fragment>',
    } as unknown as WebGLProgramParametersWithUniforms;
    material.onBeforeCompile(shader, undefined as never);
    expect(shader.vertexShader).toContain('attribute vec4 carveMark;');
    const mix = shader.fragmentShader.indexOf('vGrooveMark.rgb, 1.0 - vGrooveMark.a');
    expect(mix).toBeGreaterThan(shader.fragmentShader.indexOf('if ( carveJobMapOn )'));
    expect(mix).toBeLessThan(shader.fragmentShader.indexOf('carveJobLitho >= 0.0'));
  });
});
