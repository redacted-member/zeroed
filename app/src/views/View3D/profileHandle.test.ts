import { describe, expect, it } from 'vitest';
import type { ComponentSummary, ShapeParams } from '../../bridge/model';
import { frameFor, layoutFrame } from './handleFrame';
import { valueFields } from './handleValues';
import {
  dragAngle,
  profileAngleOf,
  profileArgs,
  profileHeightOf,
  profilePointOf,
  typedAngle,
} from './profileHandle';

const params: ShapeParams = {
  profile: 'angular',
  angle: 45,
  height: 3,
  limit: true,
  baseHeight: 0,
  smoothTop: 0.5,
  smoothBottom: 0.5,
  sharpCorners: true,
  blendInner: false,
  scale: true,
  scaleHeight: 4,
};

function shape(overrides: Partial<ComponentSummary> = {}): ComponentSummary {
  return {
    id: 7,
    name: 'Roof',
    visible: true,
    combine: 'add',
    heightScale: 1,
    level: 1,
    maxHeight: 6,
    baseHeight: 2,
    bbox: [0, 0, 100, 40],
    shape: { params, vectors: [3] },
    fade: null,
    tilt: null,
    ...overrides,
  };
}

describe('the profile angle handle', () => {
  it('shows on round and angled shapes only', () => {
    expect(profileAngleOf(shape())).toBe(45);
    expect(
      profileAngleOf(shape({ shape: { params: { ...params, profile: 'round' }, vectors: [3] } })),
    ).toBe(45);
    expect(
      profileAngleOf(shape({ shape: { params: { ...params, profile: 'smooth' }, vectors: [3] } })),
    ).toBeNull();
    expect(profileAngleOf(shape({ shape: null }))).toBeNull();
  });

  it('stands in front of the front edge half way up the shape', () => {
    expect(profilePointOf([0, 0, 100, 40])).toEqual({ x: 50, y: -12 });
    expect(profileHeightOf({ base: 2, shape: 4, scale: 1 })).toBe(4);
    const frame = frameFor(shape(), 0, null);
    expect(frame?.profile).toBe(45);
    const laid = frame === null ? null : layoutFrame((x, y, h) => ({ x, y: y + h }), frame);
    expect(laid?.handles.find((h) => h.handle.kind === 'profile')?.at).toEqual({ x: 50, y: -8 });
    expect(frame === null ? null : valueFields({ kind: 'profile' }, frame)).toEqual([
      { key: 'profile', value: 45, unit: 'angle' },
    ]);
  });

  it('steepens going up and flattens going down, within the range', () => {
    const box = [0, 0, 100, 40] as const;
    expect(dragAngle(45, 0, box)).toBe(45);
    expect(dragAngle(45, 10, box)).toBeGreaterThan(45);
    expect(dragAngle(45, -10, box)).toBeLessThan(45);
    expect(dragAngle(45, 500, box)).toBe(90);
    expect(dragAngle(45, -500, box)).toBe(1);
    expect(typedAngle(30)).toBe(30);
    expect(typedAngle(0)).toBeNull();
    expect(typedAngle(95)).toBeNull();
  });

  it('makes the shape again with only the angle changed', () => {
    const args = profileArgs(shape(), 60, 'mm');
    expect(args).toMatchObject({
      ids: [3],
      target: 7,
      combine: 'add',
      baseHeight: 2,
      name: 'Roof',
      params: {
        profile: 'angular',
        angle: 60,
        limit: true,
        height: 3,
        scale: true,
        scaleHeight: 4,
      },
    });
    expect(profileArgs(shape({ shape: null }), 60, 'mm')).toBeNull();
  });
});
