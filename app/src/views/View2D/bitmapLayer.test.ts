import { describe, expect, it } from 'vitest';

import { imageAffine } from './bitmapLayer';

function apply(m: number[], x: number, y: number): [number, number] {
  const [a = 0, b = 0, c = 0, d = 0, e = 0, f = 0] = m;
  return [a * x + c * y + e, b * x + d * y + f];
}

describe('imageAffine', () => {
  it('puts the top left pixel at the upper left corner of the quad', () => {
    // A 40 x 20 mm placement of a 400 x 200 pixel image at 10, 5.
    const run = new Float32Array([10, 5, 50, 5, 50, 25, 10, 25, 10, 5]);
    const m = imageAffine(run, 400, 200);
    expect(m).not.toBeNull();
    const affine = m ?? [];
    expect(apply(affine, 0, 0)).toEqual([10, 25]);
    expect(apply(affine, 400, 0)).toEqual([50, 25]);
    expect(apply(affine, 0, 200)).toEqual([10, 5]);
  });

  it('follows a rotated quad', () => {
    // Turned a quarter turn: lower left at 100,50, the image X axis along +Y.
    const run = new Float32Array([100, 50, 100, 70, 90, 70, 90, 50]);
    const affine = imageAffine(run, 20, 10) ?? [];
    expect(apply(affine, 0, 0)).toEqual([90, 50]);
    expect(apply(affine, 20, 10)).toEqual([100, 70]);
  });

  it('needs four corners and a real image', () => {
    expect(imageAffine(new Float32Array([0, 0, 1, 0, 1, 1]), 1, 1)).toBeNull();
    expect(imageAffine(new Float32Array(8), 0, 1)).toBeNull();
  });
});
