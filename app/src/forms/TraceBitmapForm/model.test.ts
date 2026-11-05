import { describe, expect, it } from 'vitest';

import {
  colourHex,
  initialColourState,
  pickColour,
  regionFromDrag,
  reducedPixels,
  regionPaths,
  resetLinked,
  toggleLinked,
  tracedColours,
  withColourCount,
  fadeOf,
  initialTraceState,
  opacityOf,
  selectedBitmap,
  shownName,
  traceErrors,
  traceParamsOf,
  cornerFitFromSmoothness,
  smoothnessFromCornerFit,
} from './model';

describe('tracing a placed bitmap', () => {
  it('traces the one selected bitmap', () => {
    const entities = [
      { id: 1, kind: 'path' },
      { id: 2, kind: 'bitmap' },
      { id: 3, kind: 'bitmap' },
    ];
    expect(selectedBitmap(entities, [1, 2])).toBe(2);
    expect(selectedBitmap(entities, [2, 3])).toBeNull();
    expect(selectedBitmap(entities, [1])).toBeNull();
  });

  it('needs only the trace settings', () => {
    const state = initialTraceState();
    expect(traceErrors(state)).toEqual({});
    expect(traceParamsOf(state)).toEqual({
      threshold: 128,
      invert: false,
      noise: 4,
      cornerFit: 'curves',
      smoothness: 1,
    });
    expect(traceParamsOf({ ...state, threshold: '300' })).toBeNull();
    expect(traceErrors({ ...state, noise: '-1' }).noise).toBeDefined();
  });

  it('shows only the file name of a path', () => {
    expect(shownName('/images/logo.png')).toBe('logo.png');
    expect(shownName('C:\\scans\\logo.tif')).toBe('logo.tif');
    expect(shownName(null)).toBeNull();
  });

  it('converts Bitmap Fading to opacity and back', () => {
    expect(opacityOf(0)).toBe(1);
    expect(opacityOf(75)).toBeCloseTo(0.25, 9);
    expect(opacityOf(150)).toBe(0);
    expect(fadeOf(0.25)).toBe(75);
    expect(fadeOf(1)).toBe(0);
  });
});

describe('colour tracing', () => {
  it('picks, links and resets colours, and a new count starts over', () => {
    let state = initialColourState();
    expect(tracedColours(state)).toEqual([]);
    state = pickColour(state, 3);
    state = toggleLinked(state, 5);
    state = toggleLinked(state, 1);
    expect(tracedColours(state)).toEqual([1, 3, 5]);
    state = toggleLinked(state, 5);
    expect(tracedColours(state)).toEqual([1, 3]);
    expect(tracedColours(resetLinked(state))).toEqual([3]);
    const fewer = withColourCount(state, 4);
    expect(fewer).toEqual({ count: 4, trace: null, linked: [] });
    expect(withColourCount(state, 99).count).toBe(16);
    expect(withColourCount(state, 1).count).toBe(2);
    expect(withColourCount(state, 16)).toBe(state);
  });

  it('turns a drag into a trace area and a click into none', () => {
    expect(regionFromDrag({ x: 10, y: 20 }, { x: 4, y: 2 }, 0.5)).toEqual([4, 2, 10, 20]);
    expect(regionFromDrag({ x: 10, y: 20 }, { x: 10.1, y: 25 }, 0.5)).toBeNull();
    expect(regionPaths(null)).toEqual([]);
    expect(regionPaths([0, 0, 5, 5])).toHaveLength(1);
  });

  it('writes palette colours as hex', () => {
    expect(colourHex([255, 16, 0])).toBe('#ff1000');
  });

  it('shows traced colours in the trace colour and keeps transparency', () => {
    const palette: Array<[number, number, number]> = [
      [255, 0, 0],
      [0, 0, 255],
      [0, 255, 0],
    ];
    const state = toggleLinked(pickColour(initialColourState(), 0), 1);
    const pixels = reducedPixels(new Uint8Array([0, 1, 2, 255]), palette, state);
    expect([...pixels.slice(0, 4)]).toEqual([255, 0, 0, 255]);
    expect([...pixels.slice(4, 8)]).toEqual([255, 0, 0, 255]);
    expect([...pixels.slice(8, 12)]).toEqual([0, 255, 0, 255]);
    expect([...pixels.slice(12, 16)]).toEqual([0, 0, 0, 0]);
  });
});

describe('Corner Fit slider', () => {
  it('runs from loose to tight on a log scale and back', () => {
    expect(smoothnessFromCornerFit(0)).toBe(4);
    expect(smoothnessFromCornerFit(100)).toBe(0.25);
    expect(smoothnessFromCornerFit(50)).toBe(1);
    expect(cornerFitFromSmoothness(1)).toBe(50);
    expect(cornerFitFromSmoothness(4)).toBe(0);
    expect(cornerFitFromSmoothness(null)).toBe(50);
    expect(cornerFitFromSmoothness(100)).toBe(0);
  });
});
