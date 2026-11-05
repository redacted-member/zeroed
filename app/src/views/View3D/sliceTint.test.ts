import { MeshStandardMaterial, type WebGLProgramParametersWithUniforms } from 'three';
import { describe, expect, it } from 'vitest';

import { patchFinishMaterial, finishUniforms } from './componentFinish';
import { jobUniforms, patchJobMaterial } from './jobShader';
import { setSliceTint, sliceTintAt, sliceTintUniforms } from './sliceTint';
import { patchSurfaceMaterial } from './surfaceShader';

function compile(material: MeshStandardMaterial): WebGLProgramParametersWithUniforms {
  const shader = {
    uniforms: {},
    vertexShader:
      '#include <common>\n#include <beginnormal_vertex>\n#include <begin_vertex>\n#include <project_vertex>',
    fragmentShader:
      '#include <common>\n#include <clipping_planes_fragment>\n#include <color_fragment>\n#include <roughnessmap_fragment>\n#include <metalnessmap_fragment>\n#include <emissivemap_fragment>',
  } as unknown as WebGLProgramParametersWithUniforms;
  material.onBeforeCompile(shader, undefined as never);
  return shader;
}

describe('slice colouring', () => {
  it('colours the slice red, above it green and leaves below it alone', () => {
    const band = { low: 6, high: 12 };
    expect(sliceTintAt(3, band)).toBeNull();
    expect(sliceTintAt(6, band)).toBe('slice');
    expect(sliceTintAt(12, band)).toBe('slice');
    expect(sliceTintAt(12.5, band)).toBe('above');
    expect(sliceTintAt(12.5, null)).toBeNull();
  });

  it('sets the shared uniforms', () => {
    const uniforms = sliceTintUniforms();
    setSliceTint(uniforms, { low: -20, high: -8 });
    expect(uniforms.carveSliceOn.value).toBe(true);
    expect(uniforms.carveSliceLow.value).toBe(-20);
    expect(uniforms.carveSliceHigh.value).toBe(-8);
    setSliceTint(uniforms, null);
    expect(uniforms.carveSliceOn.value).toBe(false);
  });

  it('reaches every model surface, after its other colours', () => {
    const tint = sliceTintUniforms();
    const materials = [
      patchJobMaterial(new MeshStandardMaterial(), jobUniforms(), tint),
      patchFinishMaterial(new MeshStandardMaterial(), finishUniforms(), tint),
      patchSurfaceMaterial(new MeshStandardMaterial(), {}, true, tint),
    ];
    for (const material of materials) {
      const shader = compile(material);
      expect(shader.uniforms['carveSliceOn']).toBe(tint.carveSliceOn);
      expect(shader.vertexShader).toContain('vGrooveSliceY = ( modelMatrix');
      const fragment = shader.fragmentShader;
      const tinted = fragment.indexOf('if ( carveSliceOn )');
      expect(tinted).toBeGreaterThan(fragment.indexOf('#include <color_fragment>'));
      // Last of the colour changes: no picture, paint or finish after it.
      expect(fragment.slice(tinted)).not.toMatch(/carveLinear|carveFinishShade|carveJobLinear/);
      expect(material.customProgramCacheKey()).toContain('slice');
    }
  });

  it('leaves materials without the slice as they were', () => {
    const shader = compile(patchJobMaterial(new MeshStandardMaterial(), jobUniforms()));
    expect(shader.fragmentShader).not.toContain('carveSliceOn');
    expect(
      patchSurfaceMaterial(new MeshStandardMaterial(), {}, false).customProgramCacheKey(),
    ).toBe('carve-surface');
  });
});
