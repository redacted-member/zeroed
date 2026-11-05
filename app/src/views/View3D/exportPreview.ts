// The Export Model form's calculated mesh as the 3D view draws it: the
// triangles moved into the scene where the relief shows (x -> x, job y ->
// -z, job z -> y, heights from the model's zero plane), or for a model
// wrapped round a rotary blank, placed as the view wraps the blank.

import type { JobSetup } from '../../bridge/types';
import { isRotary, rotaryOf } from '../../lib/rotary';
import type { ShownMesh } from '../../state/meshPreviewStore';
import type { ImportScene } from './importPreview';

/** The colour of the calculated mesh, a light wood. */
const MESH_COLOUR = [0.8, 0.74, 0.64] as const;

/** The scene for a calculated mesh; `zeroPlaneY` is the scene height of the
 * model's zero plane. */
export function exportScene(
  mesh: ShownMesh,
  job: JobSetup,
  zeroPlaneY: number,
  wireframe: boolean,
): ImportScene {
  const { xyz } = mesh;
  const positions = new Float32Array(xyz.length);
  const wrapped = mesh.back === 'sides' && isRotary(job);
  const rotary = rotaryOf(job);
  const radius = rotary.diameter / 2;
  for (let i = 0; i + 2 < xyz.length; i += 3) {
    const x = xyz[i] ?? 0;
    const y = xyz[i + 1] ?? 0;
    const z = xyz[i + 2] ?? 0;
    if (wrapped) {
      // Round the axis: the top of the blank at scene height nought.
      positions[i] = x;
      positions[i + 1] = z - radius;
      positions[i + 2] = rotary.axis === 'x' ? y : -y;
    } else {
      positions[i] = x;
      positions[i + 1] = zeroPlaneY + z;
      positions[i + 2] = -y;
    }
  }
  const colours = new Float32Array(xyz.length);
  for (let i = 0; i + 2 < colours.length; i += 3) {
    colours[i] = MESH_COLOUR[0];
    colours[i + 1] = MESH_COLOUR[1];
    colours[i + 2] = MESH_COLOUR[2];
  }
  return { positions, colours, plane: null, outline: null, wireframe };
}
