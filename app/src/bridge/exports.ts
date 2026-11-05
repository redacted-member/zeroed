// Model and view exports: the model as a mesh or a greyscale heightmap, and
// a picture of a view written by the kernel. Lengths are millimetres.

import { getKernel } from './client';
import type { BlobHeader } from './types';

/** Mesh formats the model can be written as. */
export type MeshFormat = 'stl' | 'obj' | 'pov';

/** How the back of an exported mesh is closed. */
export type BackFace = 'open' | 'inverted' | 'flat' | 'otherSide' | 'sides';

/** How the model is meshed: the base gap (mm), the back, and either a
 * tolerance (mm) or a triangle budget; every cell when neither is given. */
export interface MeshOptions {
  base?: number;
  back?: BackFace;
  tolerance?: number;
  maxTriangles?: number;
}

/** Triangles an export writes and its largest height difference from the model, mm. */
export interface MeshStats {
  triangles: number;
  deviation: number;
}

/** The triangles an export would write, nine floats each, job mm. */
export interface MeshPreview extends MeshStats {
  xyz: Float32Array;
}

/** What a written heightmap covers. */
export interface HeightmapInfo {
  width: number;
  height: number;
  /** Height of black. */
  minZ: number;
  /** Height of white. */
  maxZ: number;
  pixelSize: number;
}

export const exportsApi = {
  /** Writes the model, or the chosen components, as a mesh meshed and closed
   * as `options` say (an OBJ also gets a .mtl and a .jpg of the colours). */
  exportMesh: (path: string, format: MeshFormat, ids?: number[], options: MeshOptions = {}) =>
    getKernel().call<MeshStats & { format: MeshFormat }>('model.exportMesh', {
      path,
      format,
      ids,
      ...options,
    }),
  /** The triangle count and deviation an export with `options` would have. */
  meshStats: (ids?: number[], options: MeshOptions = {}) =>
    getKernel().call<MeshStats>('model.meshStats', { ids, ...options }),
  /** The triangles themselves, for the 3D view, with the stats. */
  meshPreview: async (ids?: number[], options: MeshOptions = {}): Promise<MeshPreview> => {
    const reply = await getKernel().callBlob<BlobHeader & MeshStats>('model.meshPreview', {
      ids,
      ...options,
    });
    const xyz = reply.sections['xyz'];
    return {
      triangles: reply.header.triangles,
      deviation: reply.header.deviation,
      xyz: xyz instanceof Float32Array ? xyz : new Float32Array(0),
    };
  },
  /** Writes the model over the job as a greyscale PNG of `bits` per pixel. */
  exportHeightmap: (path: string, bits: 8 | 16, ids?: number[]) =>
    getKernel().call<HeightmapInfo>('model.exportHeightmap', { path, bits, ids }),
  /** Writes a PNG (base64 or a data URL) as PNG, JPEG, BMP or GIF by the path's extension. */
  saveImage: (path: string, data: string) =>
    getKernel().call<Record<string, never>>('io.saveImage', { path, data }),
};
