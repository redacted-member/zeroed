// The material texture picture a job's appearance asks for, loaded into a
// three.js texture for the surface shader.

import { NoColorSpace, Texture } from 'three';

import { api } from '../../bridge/formApi';
import type { MaterialTexture } from '../../bridge/prefs';
import type { JobAppearance } from '../../bridge/types';

/** The picture file of the texture the appearance names, or null. */
export function texturePathFor(
  appearance: JobAppearance | undefined,
  textures: readonly MaterialTexture[] | undefined,
): string | null {
  const id = appearance?.texture ?? 0;
  if (id === 0) {
    return null;
  }
  return textures?.find((t) => t.id === id)?.path ?? null;
}

/**
 * Reads and decodes a texture picture. Rows are flipped so the picture's
 * bottom edge lies along the job's bottom edge.
 */
export async function loadMaterialTexture(path: string): Promise<Texture> {
  const blob = await api.sim.texture(path);
  const bitmap = await createImageBitmap(blob, { imageOrientation: 'flipY' });
  const texture = new Texture(bitmap);
  // The shader converts the picture's sRGB values itself.
  texture.colorSpace = NoColorSpace;
  texture.flipY = false;
  texture.needsUpdate = true;
  return texture;
}
