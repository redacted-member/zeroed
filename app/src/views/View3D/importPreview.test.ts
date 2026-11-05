import { describe, expect, it } from 'vitest';

import type { MeshPreview } from '../../bridge/meshImport';
import {
  importScene,
  planeNormal,
  rotaryToScene,
  segmentPositions,
  segmentShown,
  type ImportPreviewOptions,
} from './importPreview';

function preview(): MeshPreview {
  // Two triangles: one low (z 0..1), one high (z 4), in two segments.
  return {
    size: [10, 10, 4],
    centre: { x: 5, y: 5 },
    xyz: new Float32Array([0, 0, 0, 10, 0, 1, 0, 10, 0, 0, 0, 4, 10, 0, 4, 0, 10, 4]),
    flags: new Uint8Array([1, 0]),
    segment: new Uint16Array([0, 1]),
    segments: [
      { id: 'r1', leaves: ['r1'], size: [10, 10, 1] },
      { id: 'r0', leaves: ['r0'], size: [10, 10, 1] },
    ],
  };
}

const OPTIONS: ImportPreviewOptions = {
  step: 'orientate',
  undercuts: false,
  zeroPlane: null,
  discardBelow: true,
  layout: false,
  selected: 'r',
  plane: null,
  thickness: 18,
  material: null,
};

describe('import preview scene', () => {
  it('puts the model top at the material top and flips Y into scene Z', () => {
    const scene = importScene(preview(), OPTIONS);
    expect(scene.positions.length).toBe(18);
    expect(Array.from(scene.positions.slice(0, 3), (v) => v + 0)).toEqual([0, -4, 0]);
    expect(scene.positions[10]).toBe(0);
    expect(scene.plane).toBeNull();
  });

  it('stands a model taller than the material out above the top', () => {
    const scene = importScene(preview(), { ...OPTIONS, thickness: 3 });
    // The bottom sits at the material bottom, 3 down; the top 1 above.
    expect(scene.positions[1]).toBeCloseTo(-3);
    expect(scene.positions[10]).toBeCloseTo(1);
  });

  it('outlines the material block when given one', () => {
    expect(importScene(preview(), OPTIONS).outline).toBeNull();
    const scene = importScene(preview(), { ...OPTIONS, material: [0, 0, 300, 200] });
    expect(scene.outline?.length).toBe(72);
    expect(Math.min(...(scene.outline ?? []))).toBe(-200);
  });

  it('tints undercuts only when asked', () => {
    const plain = importScene(preview(), OPTIONS);
    const tinted = importScene(preview(), { ...OPTIONS, undercuts: true });
    expect(tinted.colours[0]).not.toBe(plain.colours[0]);
    expect(tinted.colours[9]).toBe(plain.colours[9]);
  });

  it('greys what the zero plane discards and shows the plane', () => {
    const scene = importScene(preview(), { ...OPTIONS, step: 'position', zeroPlane: 2 });
    expect(scene.colours[0]).not.toBe(scene.colours[9]);
    expect(scene.plane?.length).toBe(18);
    // The plane sits 2 above the bottom, 2 below the top.
    expect(scene.plane?.[1]).toBeCloseTo(-2);
  });

  it('shows only the segments under the chosen node while editing', () => {
    const scene = importScene(preview(), { ...OPTIONS, step: 'segment', selected: 'r0' });
    expect(scene.positions.length).toBe(9);
    expect(segmentShown(['r01', 'r1'], 'r0')).toBe(true);
    expect(segmentShown(['r1'], 'r0')).toBe(false);
    const laid = importScene(preview(), {
      ...OPTIONS,
      step: 'segment',
      layout: true,
      selected: 'r0',
    });
    expect(laid.positions.length).toBe(18);
  });

  it('turns the cut plane normal like the kernel', () => {
    expect(planeNormal({ point: [0, 0, 0], angleX: 0, angleY: 0 }).map((v) => v + 0)).toEqual([
      0, 0, 1,
    ]);
    const n = planeNormal({ point: [0, 0, 0], angleX: 90, angleY: 0 });
    expect(n[1]).toBeCloseTo(-1);
    const m = planeNormal({ point: [0, 0, 0], angleX: 0, angleY: 90 });
    expect(m[0]).toBeCloseTo(1);
  });
});

describe('rotary import preview', () => {
  it('lays the model round the axis and draws the cylinder and the axis', () => {
    expect(rotaryToScene('x', 10, 5, 2, 3)).toEqual([5, -7, 2]);
    expect(rotaryToScene('y', 10, 5, 2, 3)).toEqual([2, -7, -5]);
    const preview = {
      size: [0, 0, 0] as [number, number, number],
      centre: { x: 0, y: 0 },
      xyz: new Float32Array([0, 0, 4, 10, 0, 4, 10, 1, 4]),
      flags: new Uint8Array(1),
      segment: new Uint16Array(1),
      segments: [],
      cylinder: { length: 10, diameter: 8 },
    };
    const scene = importScene(preview, { ...OPTIONS, rotary: { axis: 'x', radius: 10 } });
    expect(Array.from(scene.positions.slice(0, 3))).toEqual([0, -6, 0]);
    expect(scene.guides?.axis.length).toBe(6);
    expect(scene.guides?.cylinder.length).toBeGreaterThan(0);
  });
});

describe('segment rings', () => {
  it('takes the triangles of one segment', () => {
    const preview = {
      size: [0, 0, 0] as [number, number, number],
      centre: { x: 0, y: 0 },
      xyz: new Float32Array(18),
      flags: new Uint8Array(2),
      segment: new Uint16Array([0, 1]),
      segments: [
        { id: 'r0', leaves: ['r0'], size: [1, 1, 1] as [number, number, number] },
        { id: 'r1', leaves: ['r1'], size: [1, 1, 1] as [number, number, number] },
      ],
    };
    const positions = new Float32Array(Array.from({ length: 18 }, (_, i) => i));
    expect(Array.from(segmentPositions(positions, preview, 'r1'))).toEqual([
      9, 10, 11, 12, 13, 14, 15, 16, 17,
    ]);
  });
});
