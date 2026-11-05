import { describe, expect, it } from 'vitest';

import {
  codedHeights,
  copyHeights,
  decodeHeight,
  encodeHeight,
  encodes,
  fillNan,
  floatHeights,
  floorReader,
  HEIGHT_NAN,
  HEIGHT_TOP_CODE,
  heightReader,
  shiftHeights,
  solidFloor,
  type Heights,
} from './heights';

const CODE = { base: -18, step: 0.25 };

function mm(heights: Heights): number[] {
  const read = heightReader(heights);
  return Array.from({ length: heights.data.length }, (_, i) => read(i));
}

describe('height codes', () => {
  it('decodes a code as base plus steps and the top code as NaN', () => {
    expect(decodeHeight(CODE, 0)).toBe(-18);
    expect(decodeHeight(CODE, 8)).toBe(-16);
    expect(decodeHeight(CODE, HEIGHT_TOP_CODE)).toBeCloseTo(-18 + HEIGHT_TOP_CODE * 0.25);
    expect(decodeHeight(CODE, HEIGHT_NAN)).toBeNaN();
  });

  it('encodes to the nearest code, clamped into range, NaN to the NaN code', () => {
    expect(encodeHeight(CODE, -16.1)).toBe(8);
    expect(encodeHeight(CODE, -100)).toBe(0);
    expect(encodeHeight(CODE, 1e9)).toBe(HEIGHT_TOP_CODE);
    expect(encodeHeight(CODE, Number.NaN)).toBe(HEIGHT_NAN);
    expect(encodeHeight({ base: 3, step: 0 }, 7)).toBe(0);
    for (const h of [-18, -17.3, 0, 12.75]) {
      expect(Math.abs(decodeHeight(CODE, encodeHeight(CODE, h)) - h)).toBeLessThanOrEqual(0.125);
    }
  });

  it('knows which heights round by at most half a step', () => {
    expect(encodes(CODE, -18.1)).toBe(true);
    expect(encodes(CODE, -18.2)).toBe(false);
    expect(encodes(CODE, Number.NaN)).toBe(true);
    expect(encodes({ base: 0, step: 0 }, 0)).toBe(true);
    expect(encodes({ base: 0, step: 0 }, 0.1)).toBe(false);
  });
});

describe('height reader', () => {
  it('reads codes and floats alike, NaN outside the grid', () => {
    const coded = codedHeights(Uint16Array.from([0, 4, HEIGHT_NAN]), CODE);
    expect(mm(coded)).toEqual([-18, -17, Number.NaN]);
    expect(heightReader(coded)(3)).toBeNaN();
    const plain = floatHeights(Float32Array.from([1.5, Number.NaN]));
    expect(mm(plain)).toEqual([1.5, Number.NaN]);
    expect(heightReader(plain)(-1)).toBeNaN();
  });

  it('moves coded heights by their base without copying the codes', () => {
    const coded = codedHeights(Uint16Array.from([0, 4]), CODE);
    const moved = shiftHeights(coded, 18);
    expect(moved.data).toBe(coded.data);
    expect(mm(moved)).toEqual([0, 1]);
    expect(mm(shiftHeights(floatHeights(Float32Array.from([1])), 2))).toEqual([3]);
  });

  it('fills NaN cells with a code when the height has one, floats otherwise', () => {
    const coded = codedHeights(Uint16Array.from([0, HEIGHT_NAN]), CODE);
    const filled = fillNan(coded, -17);
    expect(filled.code).toBe(CODE);
    expect(mm(filled)).toEqual([-18, -17]);
    const far = fillNan(coded, -40);
    expect(far.code).toBeNull();
    expect(mm(far)).toEqual([-18, -40]);
    const whole = codedHeights(Uint16Array.from([1]), CODE);
    expect(fillNan(whole, 0)).toBe(whole);
  });
});

describe('copying heights', () => {
  it('copies codes straight across under the same code', () => {
    const into = codedHeights(new Uint16Array(4), CODE);
    copyHeights(into, 1, codedHeights(Uint16Array.from([7, 8, 9]), { ...CODE }), 1, 2);
    expect(Array.from(into.data)).toEqual([0, 8, 9, 0]);
  });

  it('re-encodes across codes and decodes into floats', () => {
    const into = codedHeights(new Uint16Array(2), CODE);
    copyHeights(
      into,
      0,
      codedHeights(Uint16Array.from([2, HEIGHT_NAN]), { base: -17, step: 1 }),
      0,
      2,
    );
    expect(mm(into)).toEqual([-15, Number.NaN]);
    const floats = floatHeights(new Float32Array(2));
    copyHeights(floats, 0, into, 0, 2);
    expect(Array.from(floats.data)).toEqual([-15, Number.NaN]);
  });
});

describe('underside', () => {
  const field = {
    z: codedHeights(Uint16Array.from([72, HEIGHT_NAN, 40]), CODE),
    floor: 'flat' as const,
    bottom: -18,
  };

  it('reads a flat underside as the bottom, open where the top is', () => {
    const read = floorReader(field);
    expect([0, 1, 2].map((i) => read?.(i))).toEqual([-18, Number.NaN, -18]);
    expect(floorReader({ z: field.z, bottom: -18 })).toBeNull();
  });

  it('makes the flat underside into heights in the same code', () => {
    const floor = solidFloor(field);
    expect(floor.code).toBe(CODE);
    expect(Array.from(floor.data)).toEqual([0, HEIGHT_NAN, 0]);
  });
});
