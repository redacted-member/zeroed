import { describe, expect, it } from 'vitest';
import type { PaletteColours } from '../../bridge/componentTree';
import {
  FINISHES,
  finishLayers,
  finishPalette,
  finishSlot,
  patchFinishMaterial,
  vertexFinish,
  finishUniforms,
} from './componentFinish';
import { MeshStandardMaterial } from 'three';

function colours(materials: (string | null)[] | undefined): PaletteColours {
  return {
    width: 2,
    height: 1,
    // Entry 0 clear, 1 oak's colour, 2 a plain red.
    palette: new Uint8Array([0, 0, 0, 0, 185, 140, 87, 255, 255, 0, 0, 255]),
    index: new Uint8Array([1, 2]),
    any: true,
    materials,
  };
}

describe('component material finishes', () => {
  it('numbers the finishes from one', () => {
    expect(finishSlot('pine')).toBe(1);
    expect(finishSlot('aluminium')).toBe(FINISHES.length);
    expect(finishSlot(null)).toBe(0);
    expect(finishSlot('marble')).toBe(0);
  });

  it('gives each palette entry its material finish', () => {
    const palette = finishPalette(colours([null, 'oak', null]));
    expect(palette).not.toBeNull();
    expect(Array.from(palette?.slice(4, 8) ?? [])).toEqual([2, 184, 5, 255]);
    expect(palette?.[11]).toBe(0);
    expect(finishPalette(colours([null, null, null]))).toBeNull();
    expect(finishPalette(colours(undefined))).toBeNull();
  });

  it('puts the finish on the vertices of cells that have one', () => {
    const finish = vertexFinish(
      { width: 2, height: 1 },
      colours([null, 'aluminium', null]),
      [0, 1],
      [0],
      4,
    );
    expect(finish?.[0]).toBe(7);
    expect(finish?.[2]).toBeCloseTo(0.75, 2);
    expect(finish?.[3]).toBe(0);
    expect(
      vertexFinish({ width: 3, height: 1 }, colours([null, 'oak', null]), [0], [0], 1),
    ).toBeNull();
  });

  it('draws pictures that tile and keep the colour on average', () => {
    const size = 64;
    const layers = finishLayers(size);
    expect(layers.length).toBe(size * size * FINISHES.length);
    FINISHES.forEach((finish, layer) => {
      const base = layer * size * size;
      let sum = 0;
      let seam = 0;
      for (let j = 0; j < size; j += 1) {
        sum += Array.from(layers.slice(base + j * size, base + (j + 1) * size)).reduce(
          (a, b) => a + b,
          0,
        );
        seam = Math.max(
          seam,
          Math.abs((layers[base + j * size] ?? 0) - (layers[base + j * size + size - 1] ?? 0)),
        );
      }
      const mean = sum / (size * size);
      expect(Math.abs(mean - 128), finish).toBeLessThan(24);
      expect(seam, finish).toBeLessThan(40);
    });
    const acrylic = FINISHES.indexOf('acrylic') * size * size;
    expect(layers[acrylic]).toBe(128);
  });

  it('patches the vertex coloured material to read the finish', () => {
    const material = patchFinishMaterial(new MeshStandardMaterial(), finishUniforms());
    const shader = {
      uniforms: {},
      vertexShader: '#include <common>\n#include <begin_vertex>',
      fragmentShader:
        '#include <common>\n#include <color_fragment>\n#include <roughnessmap_fragment>\n#include <metalnessmap_fragment>',
    };
    material.onBeforeCompile(shader as never, undefined as never);
    expect(shader.vertexShader).toContain('vGrooveFinish = carveFinish');
    expect(shader.fragmentShader).toContain('carveFinishShade');
    expect(shader.fragmentShader).toContain('roughnessFactor = carveFinishRough');
    expect(shader.fragmentShader).toContain('metalnessFactor = carveFinishMetal');
    expect(Object.keys(shader.uniforms)).toContain('carveFinishMap');
  });
});
