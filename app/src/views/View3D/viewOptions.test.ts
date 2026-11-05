import { describe, expect, it } from 'vitest';

import { Vector3 } from 'three';

import { DEFAULT_JOB } from '../../bridge/types';
import { SHADOW_DEPTH, stepShadowDepth } from '../../lib/shadowDepth';
import { followDirection, LIGHTING, shadowLighting } from './sceneExtras';
import { markerPlacement, shownJob } from './viewOptions';

describe('view options', () => {
  it('lays a rotary job flat only when wrapping is off', () => {
    const job = { ...DEFAULT_JOB, kind: 'rotary' as const };
    expect(shownJob(job, true).kind).toBe('rotary');
    expect(shownJob(job, false).kind).toBe('singleSided');
    const flat = DEFAULT_JOB;
    expect(shownJob(flat, false)).toBe(flat);
  });

  it('puts the modeling plane at the depth it is given', () => {
    expect(markerPlacement(DEFAULT_JOB, 6).planeY).toBe(-6);
  });

  it('puts the origin arrows at Z zero', () => {
    const job = { ...DEFAULT_JOB, zZero: 'bottom' as const, thickness: 12 };
    expect(markerPlacement(job, 0).originY).toBe(-12);
    expect(markerPlacement({ ...job, zZero: 'top' }, 0).originY).toBe(-0);
  });

  it('points a following light from the camera, a little above it', () => {
    const d = followDirection(new Vector3(0, 0, 10), new Vector3(0, 0, 0));
    expect(d.z).toBeGreaterThan(0.9);
    expect(d.y).toBeGreaterThan(0);
    expect(LIGHTING.shadow.sky).toBeLessThan(LIGHTING.normal.sky);
  });

  it('steps the shadow depth between normal and the deepest shading', () => {
    expect(shadowLighting(SHADOW_DEPTH.initial)).toEqual(LIGHTING.shadow);
    const deeper = shadowLighting(stepShadowDepth(SHADOW_DEPTH.initial, 1));
    expect(deeper.sky).toBeLessThan(LIGHTING.shadow.sky);
    expect(deeper.sun).toBeGreaterThan(LIGHTING.shadow.sun);
    expect(shadowLighting(stepShadowDepth(SHADOW_DEPTH.initial, -1)).sky).toBeGreaterThan(
      LIGHTING.shadow.sky,
    );
    let depth: number = SHADOW_DEPTH.initial;
    for (let i = 0; i < 20; i += 1) {
      depth = stepShadowDepth(depth, 1);
    }
    expect(depth).toBe(SHADOW_DEPTH.max);
    for (let i = 0; i < 20; i += 1) {
      depth = stepShadowDepth(depth, -1);
    }
    expect(depth).toBe(SHADOW_DEPTH.min);
  });
});
