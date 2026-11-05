import { describe, expect, it } from 'vitest';

import type { PieceRow } from '../../bridge/assembly';
import type { FitJoint } from '../../bridge/assemblyFit';
import { FIT_COLOURS, fitMeshes, jointFrame } from './fitOverlay';

const piece = (sheet: number): PieceRow => ({
  sheet,
  part: 1,
  parts: 1,
  sheetName: 'S',
  vectorName: null,
  model: false,
  position: [0, 0, 0],
  rotation: [0, 0, 0],
  size: [10, 10, 10],
  hasModel: false,
  hasToolpaths: true,
});

const joint: FitJoint = {
  a: { sheet: 1, part: 1 },
  b: { sheet: 2, part: 1 },
  status: 'interference',
  source: 'exact',
  cell: null,
  sides: [],
  min: -0.1,
  max: 0.2,
  overlapDepth: 0.1,
  overlapVolume: 1,
  bounds: [
    [0, 0, 0],
    [10, 2, 4],
  ],
  patches: [
    {
      status: 'interference',
      min: -0.1,
      max: -0.1,
      a: [0, 0, 0, 1, 0, 0, 0, 1, 0],
      b: [0, 0, 0, 1, 0, 0, 0, 1, 0],
    },
    { status: 'ok', min: 0.2, max: 0.2, a: [0, 0, 0, 1, 0, 0, 0, 1, 0], b: [] },
  ],
};

describe('fitMeshes', () => {
  it('draws the picked joint strong on both pieces, colored by each patch', () => {
    const meshes = fitMeshes([joint], [piece(1), piece(2)], 0, false);
    expect(meshes.length).toBe(3);
    expect(meshes.every((m) => m.strong)).toBe(true);
    expect(meshes[0]?.colour).toBe(FIT_COLOURS.interference);
    expect(meshes[2]?.colour).toBe(FIT_COLOURS.ok);
  });

  it('draws nothing unpicked unless Show Fit is on', () => {
    expect(fitMeshes([joint], [piece(1), piece(2)], null, false)).toEqual([]);
    const all = fitMeshes([joint], [piece(1), piece(2)], null, true);
    expect(all.length).toBe(3);
    expect(all.some((m) => m.strong)).toBe(false);
    expect(fitMeshes(null, [piece(1)], 0, true)).toEqual([]);
  });

  it('frames a joint at least 40 mm across, moved with its pieces', () => {
    const box = jointFrame(joint, [0, 0, 5]);
    expect(box.min).toEqual([-15, -19, -13]);
    expect(box.max).toEqual([25, 21, 27]);
  });
});
