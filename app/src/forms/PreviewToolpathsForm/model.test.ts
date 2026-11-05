import { describe, expect, it } from 'vitest';

import type { Heightfield, SimPatch } from '../../bridge/cam';
import {
  codedHeights,
  floatHeights,
  HEIGHT_NAN,
  heightReader,
  type Heights,
} from '../../bridge/heights';
import {
  areaChoice,
  fromAreaChoice,
  selectedColour,
  visibleOnSides,
  applyPatch,
  machinedColours,
  SPEED_MAX,
  SPEED_MIN,
  animationRate,
  FRAME_MS,
  frameDistance,
  nextStepCost,
  paceDistance,
  toolpathColour,
  TOOLPATH_PALETTE,
  addTexture,
  addTextureCategory,
  addTextureValue,
  categoryName,
  textureOf,
  texturesByCategory,
  textureValue,
} from './model';

function mm(heights: Heights | 'flat' | undefined): number[] {
  if (heights === undefined || heights === 'flat') {
    return [];
  }
  const read = heightReader(heights);
  return Array.from({ length: heights.data.length }, (_, i) => read(i));
}

function field(width: number, height: number): Heightfield {
  return {
    width,
    height,
    cellSize: 1,
    originX: 0.5,
    originY: 0.5,
    top: 0,
    bottom: -10,
    revision: 1,
    sections: [],
    z: floatHeights(new Float32Array(width * height)),
    owner: new Uint16Array(width * height),
    floor: floatHeights(new Float32Array(width * height).fill(-10)),
  };
}

/** A field in 0.5 mm codes from -10 (code 0) up, the top at code 20. */
const CODE = { base: -10, step: 0.5 };
function codedField(width: number, height: number): Heightfield {
  return {
    ...field(width, height),
    z: codedHeights(new Uint16Array(width * height).fill(20), CODE),
    floor: 'flat',
  };
}

function codedPatch(z: number[], floor: number[]): SimPatch {
  return {
    rects: [{ x0: 1, y0: 0, width: z.length, height: 1 }],
    z: codedHeights(Uint16Array.from(z), CODE),
    owner: new Uint16Array(z.length).fill(1),
    floor: codedHeights(Uint16Array.from(floor), CODE),
  };
}

describe('toolpath preview model', () => {
  it('speeds up as the slider goes up', () => {
    expect(animationRate(SPEED_MIN)).toBeLessThan(animationRate(SPEED_MAX));
    expect(animationRate(0)).toBe(animationRate(SPEED_MIN));
    expect(animationRate(99)).toBe(animationRate(SPEED_MAX));
  });

  it('moves the tool by the time since the last step, at an even speed', () => {
    const rate = animationRate(5);
    expect(frameDistance(5, FRAME_MS)).toBeCloseTo((rate * FRAME_MS) / 1000);
    // Two short steps cover what one step twice as long does.
    expect(frameDistance(5, 8) * 2).toBeCloseTo(frameDistance(5, 16));
    // A long stall does not make the tool leap ahead.
    expect(frameDistance(5, 1000)).toBeCloseTo(frameDistance(5, FRAME_MS * 2));
    expect(frameDistance(5, -3)).toBe(0);
  });

  it('shortens the steps to what the kernel cuts within a frame', () => {
    const wanted = frameDistance(5, FRAME_MS);
    // A quick kernel keeps the chosen speed.
    expect(paceDistance(5, FRAME_MS, 0.001)).toBeCloseTo(wanted);
    expect(paceDistance(5, FRAME_MS, null)).toBeCloseTo(wanted);
    // A slow one gets steps it can cut in about a frame, never below a
    // quarter of a frame's worth.
    expect(paceDistance(5, FRAME_MS, 0.5)).toBeCloseTo(FRAME_MS / 0.5);
    expect(paceDistance(5, FRAME_MS, 1000)).toBeCloseTo(wanted / 4);
    // The cost follows the steps, smoothed.
    expect(nextStepCost(null, 10, 20)).toBeCloseTo(0.5);
    expect(nextStepCost(0.5, 30, 20)).toBeCloseTo(0.75);
    expect(nextStepCost(0.5, 10, 0)).toBe(0.5);
  });

  it('patches the changed cells into the field', () => {
    const before = field(4, 3);
    const after = applyPatch(before, {
      rects: [
        { x0: 1, y0: 1, width: 2, height: 2 },
        { x0: 3, y0: 0, width: 1, height: 1 },
      ],
      z: floatHeights(new Float32Array([-1, -2, -3, -4, -5])),
      owner: new Uint16Array([1, 1, 2, 2, 3]),
      // No underside sent: every cell's is uncut.
      floor: floatHeights(new Float32Array(0)),
    });
    expect(after).not.toBe(before);
    expect(mm(after.z)).toEqual([0, 0, 0, -5, 0, -1, -2, 0, 0, -3, -4, 0]);
    expect(after.owner[10]).toBe(2);
    expect(after.owner[3]).toBe(3);
    // A patch that does not fit is left out.
    const same = applyPatch(after, {
      rects: [{ x0: 3, y0: 0, width: 2, height: 1 }],
      z: floatHeights(new Float32Array(2)),
      owner: new Uint16Array(2),
      floor: floatHeights(new Float32Array(2)),
    });
    expect(same).toBe(after);
  });

  it('copies the codes of a patch under the same code straight in', () => {
    const before = codedField(4, 1);
    const codes = before.z.data;
    const after = applyPatch(before, codedPatch([12, HEIGHT_NAN], [0, HEIGHT_NAN]));
    expect(after.z.data).toBe(codes);
    expect(Array.from(codes)).toEqual([20, 12, HEIGHT_NAN, 20]);
    expect(mm(after.z)).toEqual([0, -4, Number.NaN, 0]);
    // A floor at the bottom and open under the hole keeps the underside flat.
    expect(after.floor).toBe('flat');
  });

  it('encodes a patch of floats into the field codes', () => {
    const after = applyPatch(codedField(3, 1), {
      ...codedPatch([0], [0]),
      z: floatHeights(Float32Array.from([-2.9])),
      floor: floatHeights(Float32Array.from([-10])),
    });
    expect(Array.from(after.z.data)).toEqual([20, 14, 20]);
  });

  it('gives a flat underside heights of its own once a patch raises it', () => {
    const after = applyPatch(codedField(3, 1), codedPatch([20], [4]));
    expect(after.floor).not.toBe('flat');
    expect(mm(after.floor)).toEqual([-10, -8, -10]);
  });

  it('patches the laser char of a preview into the field, keeping its array', () => {
    const before = codedField(4, 1);
    expect(before.burn).toBeUndefined();
    // A patch without marks leaves the field without them.
    expect(applyPatch(before, codedPatch([20], [0])).burn).toBeUndefined();
    const marked = applyPatch(before, {
      ...codedPatch([20, 20], [0, 0]),
      burn: Uint8Array.from([128, 255]),
    });
    expect(Array.from(marked.burn ?? [])).toEqual([0, 128, 255, 0]);
    // Later patches write into the same array, so only they upload.
    const burn = marked.burn;
    const again = applyPatch(marked, {
      rects: [{ x0: 3, y0: 0, width: 1, height: 1 }],
      z: codedHeights(Uint16Array.from([20]), CODE),
      owner: Uint16Array.from([1]),
      floor: codedHeights(Uint16Array.from([0]), CODE),
      burn: Uint8Array.from([64]),
    });
    expect(again.burn).toBe(burn);
    expect(Array.from(again.burn ?? [])).toEqual([0, 128, 255, 64]);
  });

  it('paints machined cells by toolpath or with one colour', () => {
    const f = field(2, 2);
    f.owner.set([0, 1, 2, 2]);
    expect(machinedColours(f, 'material', '#ffffff', () => null)).toBeNull();
    const global = machinedColours(f, 'global', '#ff0000', () => null);
    expect(Array.from(global?.rgba ?? [])).toEqual([
      0, 0, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
    ]);
    const byToolpath = machinedColours(f, 'toolpath', '#ff0000', (owner) =>
      owner === 1 ? '#00ff00' : null,
    );
    expect(Array.from(byToolpath?.rgba.subarray(4, 12) ?? [])).toEqual([
      0, 255, 0, 255, 0, 0, 0, 0,
    ]);
    expect(machinedColours(f, 'toolpath', '#ff0000', () => null)).toBeNull();
  });

  it('gives toolpaths palette colours unless one was chosen', () => {
    expect(toolpathColour(0, 7, {})).toBe(TOOLPATH_PALETTE[0]);
    expect(toolpathColour(TOOLPATH_PALETTE.length, 7, {})).toBe(TOOLPATH_PALETTE[0]);
    expect(toolpathColour(0, 7, { 7: '#123456' })).toBe('#123456');
  });
});

describe('previewing the sides of a double sided job', () => {
  const list = [
    { id: 1, visible: true, side: 'top' as const },
    { id: 2, visible: true, side: 'bottom' as const },
    { id: 3, visible: false, side: 'top' as const },
    { id: 4, visible: true },
  ];

  it('takes the visible toolpaths of the side shown, or of both', () => {
    expect(visibleOnSides(list, 'top', false)).toEqual([1, 4]);
    expect(visibleOnSides(list, 'bottom', false)).toEqual([2]);
    expect(visibleOnSides(list, 'bottom', true)).toEqual([1, 2, 4]);
  });

  it('adds textures under a chosen or newly named category', () => {
    expect(addTextureCategory(addTextureValue('Stone'))).toBe('Stone');
    expect(addTextureCategory('texture:3')).toBeNull();
    expect(categoryName('  Metals ')).toBe('Metals');
    expect(categoryName('   ')).toBeNull();
    expect(categoryName(null)).toBeNull();
    const added = addTexture([], '/p/Granite.png', 'Stone');
    expect(added.textures[0]?.category).toBe('Stone');
  });

  it('lists material textures by category and adds pictures named after their files', () => {
    expect(textureOf(textureValue(7))).toBe(7);
    expect(textureOf('oak')).toBeNull();
    const first = addTexture([], '/pictures/Cherry Grain.jpg', 'My Textures');
    expect(first.id).toBe(1);
    expect(first.textures[0]).toEqual({
      id: 1,
      name: 'Cherry Grain',
      category: 'My Textures',
      path: '/pictures/Cherry Grain.jpg',
    });
    const more = addTexture(
      [...first.textures, { id: 5, name: 'Slate', category: 'Stone', path: '/s.png' }],
      'C:\\textures\\burl.png',
      'My Textures',
    );
    expect(more.id).toBe(6);
    expect(more.textures[2]?.name).toBe('burl');
    expect(texturesByCategory(more.textures).map((g) => [g.category, g.textures.length])).toEqual([
      ['My Textures', 2],
      ['Stone', 1],
    ]);
  });

  it('reads the Machined Area Color radios', () => {
    expect(areaChoice('global', false)).toBe('global');
    expect(areaChoice('toolpath', true)).toBe('lithophane');
    expect(fromAreaChoice('lithophane')).toEqual({ fill: 'material', lithophane: true });
    expect(fromAreaChoice('surface')).toEqual({ fill: 'surface', lithophane: false });
  });

  it('shows the selected toolpath in the Toolpath Color swatch', () => {
    const list = [{ id: 4 }, { id: 9 }];
    expect(selectedColour(list, null, {})).toBeNull();
    expect(selectedColour(list, 7, {})).toBeNull();
    expect(selectedColour(list, 9, {})).toBe(TOOLPATH_PALETTE[1]);
    expect(selectedColour(list, 9, { 9: 'none' })).toBe('none');
  });
});
