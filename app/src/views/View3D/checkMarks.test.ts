import { describe, expect, it } from 'vitest';

import { MARK_WEIGHT, markAttribute, markedColours, vertexBurn, vertexCodes } from './checkMarks';
import { srgbToLinear } from './componentColours';

// A 4 by 3 field, codes row by row from the lowest Y.
const codes = new Uint8Array([0, 2, 0, 0, 0, 0, 0, 3, 0, 1, 0, 0]);

describe('check marks on the CPU surface', () => {
  it('gives every vertex of a full grid its own cell', () => {
    const grid = { rows: [0, 1, 2], columns: [0, 1, 2, 3], rowsAreY: true };
    expect(Array.from(vertexCodes(codes, 4, 3, grid) ?? [])).toEqual(Array.from(codes));
  });

  it('keeps the most telling code of the cells a sampled vertex stands for', () => {
    // Every other cell: the vertex at (0, 0) stands for cells (0..1, 0..1)
    // with leftover; the one at (0, 2) for row 2 with a gouge.
    const grid = { rows: [0, 2], columns: [0, 2, 3], rowsAreY: true };
    expect(Array.from(vertexCodes(codes, 4, 3, grid) ?? [])).toEqual([2, 0, 3, 1, 0, 0]);
    // A gouge wins over leftover and an undercut in one span.
    const coarse = { rows: [0, 2], columns: [0, 2], rowsAreY: true };
    const all = new Uint8Array([2, 3, 0, 1, 0, 0, 0, 0, 0]);
    expect(vertexCodes(all, 3, 3, coarse)?.[0]).toBe(1);
  });

  it('reads rows along X for a rotary job turning about X', () => {
    const grid = { rows: [0, 1, 2, 3], columns: [0, 1, 2], rowsAreY: false };
    // Vertex (row x, column y) shows cell (x, y).
    const out = vertexCodes(codes, 4, 3, grid) ?? new Uint8Array();
    expect(out[1 * 3 + 2]).toBe(1);
    expect(out[3 * 3 + 1]).toBe(3);
    expect(out[1 * 3 + 0]).toBe(2);
  });

  it('shows nothing when nothing is marked or the codes do not fit', () => {
    const grid = { rows: [0, 1, 2], columns: [0, 1, 2, 3], rowsAreY: true };
    expect(vertexCodes(new Uint8Array(12), 4, 3, grid)).toBeNull();
    expect(vertexCodes(codes, 5, 3, grid)).toBeNull();
  });

  it('marks vertices in linear color over the material, the rest clear', () => {
    const marks = markAttribute(new Uint8Array([1, 0]), 3);
    // Gouge red (#d93636) at the weight the GPU surface mixes with.
    expect(marks[0]).toBeCloseTo(srgbToLinear(0xd9), 5);
    expect(marks[3]).toBeCloseTo(1 - MARK_WEIGHT, 5);
    // Unmarked vertices, and those past the codes (cap hubs), stay clear.
    expect(marks[7]).toBe(1);
    expect(marks[11]).toBe(1);
  });

  it('mixes the marks into painted vertex colors', () => {
    const base = new Float32Array([1, 1, 1, 0.5, 0.5, 0.5]);
    const out = markedColours(base, new Uint8Array([2, 0]));
    const blue = srgbToLinear(0xe0);
    expect(out[2]).toBeCloseTo(1 + (blue - 1) * MARK_WEIGHT, 5);
    expect(Array.from(out.slice(3))).toEqual([0.5, 0.5, 0.5]);
    // The colors given are left alone.
    expect(base[2]).toBe(1);
  });

  it('chars the vertices a laser burnt, the darkest cell a vertex stands for', () => {
    const burn = new Uint8Array([0, 40, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0]);
    const grid = { rows: [0, 2], columns: [0, 2, 3], rowsAreY: true };
    expect(Array.from(vertexBurn(burn, 4, 3, grid) ?? [])).toEqual([40, 0, 0, 255, 0, 0]);
    expect(vertexBurn(new Uint8Array(12), 4, 3, grid)).toBeNull();
    expect(vertexBurn(burn, 5, 3, grid)).toBeNull();
  });

  it('mixes the char in as dark as the beam burnt, under a check mark', () => {
    const chars = new Uint8Array([255, 51, 0, 255]);
    const marks = markAttribute(new Uint8Array([0, 0, 0, 1]), 4, chars);
    // Black char covers all of the material, a fifth as dark a fifth of it.
    expect(marks[0]).toBeCloseTo(srgbToLinear(0x20), 5);
    expect(marks[3]).toBeCloseTo(0, 5);
    expect(marks[7]).toBeCloseTo(0.8, 5);
    expect(marks[11]).toBe(1);
    // The check's gouge shows over the char.
    expect(marks[12]).toBeCloseTo(srgbToLinear(0xd9), 5);
    expect(marks[15]).toBeCloseTo(1 - MARK_WEIGHT, 5);
    const painted = markedColours(new Float32Array([1, 1, 1, 1, 1, 1]), null, chars);
    expect(painted[0]).toBeCloseTo(srgbToLinear(0x20), 5);
    expect(painted[3]).toBeCloseTo(1 + (srgbToLinear(0x20) - 1) * 0.2, 5);
  });
});
