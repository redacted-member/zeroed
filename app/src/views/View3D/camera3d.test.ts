import { describe, expect, it } from 'vitest';

import {
  blendView,
  boundingSphere,
  cameraBasis,
  cameraPosition,
  clampPhi,
  fitRadius,
  fitView,
  FOV_DEG,
  nearestTurn,
  orthoHalfSize,
  sceneBoxOf,
  VIEW_PRESETS,
  type Vec3,
} from './camera3d';

function close(actual: Vec3, expected: Vec3): void {
  actual.forEach((value, i) => expect(value).toBeCloseTo(expected[i] ?? 0, 9));
}

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

describe('view presets', () => {
  it('looks down on the job from the top with job +Y up the screen', () => {
    const { theta, phi } = VIEW_PRESETS.top;
    const basis = cameraBasis(theta, phi);
    close(basis.back, [0, 1, 0]);
    close(basis.right, [1, 0, 0]);
    // Scene -z is job +Y.
    close(basis.up, [0, 0, -1]);
  });

  it('looks from the front, back, left and right with scene y up', () => {
    const cases: Array<[keyof typeof VIEW_PRESETS, Vec3, Vec3]> = [
      ['front', [0, 0, 1], [1, 0, 0]],
      ['back', [0, 0, -1], [-1, 0, 0]],
      ['left', [-1, 0, 0], [0, 0, 1]],
      ['right', [1, 0, 0], [0, 0, -1]],
    ];
    for (const [name, back, right] of cases) {
      const preset = VIEW_PRESETS[name];
      const basis = cameraBasis(preset.theta, preset.phi);
      close(basis.back, back);
      close(basis.right, right);
      close(basis.up, [0, 1, 0]);
    }
  });

  it('looks up from below with job -Y up the screen', () => {
    const basis = cameraBasis(VIEW_PRESETS.bottom.theta, VIEW_PRESETS.bottom.phi);
    close(basis.back, [0, -1, 0]);
    close(basis.up, [0, 0, 1]);
  });

  it('keeps an orthonormal basis at any angle', () => {
    for (const [theta, phi] of [
      [0.3, 0.7],
      [2.5, 1.9],
      [-1.2, 0],
      [4, Math.PI],
    ] as const) {
      const { back, right, up } = cameraBasis(theta, phi);
      expect(dot(back, right)).toBeCloseTo(0, 9);
      expect(dot(back, up)).toBeCloseTo(0, 9);
      expect(dot(right, up)).toBeCloseTo(0, 9);
      expect(Math.hypot(...up)).toBeCloseTo(1, 9);
    }
  });

  it('places the camera the radius away from the target', () => {
    const at = cameraPosition({ ...VIEW_PRESETS.front, radius: 50, target: [10, -2, -30] });
    close(at, [10, -2, 20]);
    expect(clampPhi(-1)).toBe(0);
    expect(clampPhi(4)).toBe(Math.PI);
  });
});

describe('framing', () => {
  it('sizes the orthographic frustum to match the perspective view at the target', () => {
    const { halfWidth, halfHeight } = orthoHalfSize(100, 2);
    expect(halfHeight).toBeCloseTo(100 * Math.tan((FOV_DEG / 2 / 180) * Math.PI), 9);
    expect(halfWidth).toBeCloseTo(halfHeight * 2, 9);
  });

  it('fits a sphere inside an orthographic view both ways', () => {
    for (const aspect of [0.5, 1, 2.5]) {
      const radius = fitRadius(40, aspect, 'orthographic', 1);
      const { halfWidth, halfHeight } = orthoHalfSize(radius, aspect);
      expect(Math.min(halfWidth, halfHeight)).toBeCloseTo(40, 6);
    }
  });

  it('fits a sphere inside the perspective cone on the narrow side', () => {
    const half = (FOV_DEG / 2 / 180) * Math.PI;
    // Wide view: the vertical field is the narrow one.
    expect(fitRadius(40, 2, 'perspective', 1)).toBeCloseTo(40 / Math.sin(half), 6);
    // Tall view: the horizontal field is narrower.
    const halfH = Math.atan(Math.tan(half) * 0.5);
    expect(fitRadius(40, 0.5, 'perspective', 1)).toBeCloseTo(40 / Math.sin(halfH), 6);
    // Margin scales linearly.
    expect(fitRadius(40, 2, 'perspective', 1.1)).toBeCloseTo(
      (44 / 40) * fitRadius(40, 2, 'perspective', 1),
      6,
    );
  });

  it('centres a fitted view on the box', () => {
    const box = { min: [0, -18, -200] as Vec3, max: [300, 0, 0] as Vec3 };
    const sphere = boundingSphere(box);
    close(sphere.centre, [150, -9, -100]);
    expect(sphere.radius).toBeCloseTo(Math.hypot(300, 18, 200) / 2, 9);
    const view = fitView(box, VIEW_PRESETS.iso, 1.5, 'perspective');
    close(view.target, sphere.centre);
    expect(view.theta).toBe(VIEW_PRESETS.iso.theta);
    expect(view.radius).toBeGreaterThan(sphere.radius);
  });
});

describe('blending views', () => {
  it('turns the short way round', () => {
    expect(nearestTurn(Math.PI * 1.9, 0)).toBeCloseTo(-Math.PI * 0.1, 9);
    const from = { theta: -Math.PI * 0.9, phi: 1, radius: 100, target: [0, 0, 0] as Vec3 };
    const to = { theta: Math.PI * 0.9, phi: 1, radius: 100, target: [0, 0, 0] as Vec3 };
    const mid = blendView(from, to, 0.5);
    expect(mid.theta).toBeCloseTo(-Math.PI, 9);
  });

  it('starts at the first view and ends at the second', () => {
    const from = { theta: 0, phi: 0.5, radius: 100, target: [0, 0, 0] as Vec3 };
    const to = { theta: 1, phi: 1.5, radius: 400, target: [10, 20, 30] as Vec3 };
    expect(blendView(from, to, 0)).toEqual(from);
    const end = blendView(from, to, 1);
    expect(end.theta).toBeCloseTo(1, 9);
    expect(end.radius).toBeCloseTo(400, 9);
    close(end.target, [10, 20, 30]);
    // Distance blends geometrically: halfway between 100 and 400 is 200.
    expect(blendView(from, to, 0.5).radius).toBeCloseTo(200, 9);
  });

  it('puts a job box in the scene with the material top at height 0, either Z zero', () => {
    // An issue 2 mm into 18 mm material, framed down from the top.
    const top = { min: [10, 20, -2] as const, max: [30, 40, 0] as const };
    expect(sceneBoxOf(top, 0)).toEqual({ min: [10, -2, -40], max: [30, 0, -20] });
    // The same spot with Z zero at the bottom: job Z 16 to 18.
    const bottom = { min: [10, 20, 16] as const, max: [30, 40, 18] as const };
    expect(sceneBoxOf(bottom, 18)).toEqual({ min: [10, -2, -40], max: [30, 0, -20] });
  });
});
