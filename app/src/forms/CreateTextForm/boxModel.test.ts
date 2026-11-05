import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB } from '../../bridge/types';
import type { TextSpec } from '../../bridge/document';
import {
  autoLayoutSpec,
  boxBounds,
  boxErrors,
  boxFromDrag,
  boxState,
  initialAutoLayoutState,
  layoutBounds,
  stateFromLayout,
} from './boxModel';

const spec: TextSpec = {
  text: 'A',
  fontFamily: 'Arial',
  bold: false,
  italic: false,
  sizeMm: 0,
  alignment: 'centre',
  lineSpacing: 1,
  charSpacing: 0,
  kerning: true,
  wrapWidth: 40,
};

describe('text in a box model', () => {
  it('fills the selected vectors or else the material', () => {
    expect(layoutBounds([1, 2, 30, 20], DEFAULT_JOB)).toEqual([1, 2, 30, 20]);
    const job = layoutBounds(null, DEFAULT_JOB);
    expect(job[2] - job[0]).toBe(DEFAULT_JOB.width);
    expect(layoutBounds([1, 1, 1, 5], DEFAULT_JOB)).toEqual(job);
  });

  it('builds the spec with its box and keeps the settings for editing', () => {
    const state = { ...initialAutoLayoutState(), horizontal: 'wordSpacing' as const };
    const built = autoLayoutSpec(spec, [0, 0, 10, 10], state);
    expect(built.autoLayout).toEqual({
      bounds: [0, 0, 10, 10],
      margin: 'normal',
      vertical: 'none',
      horizontal: 'wordSpacing',
    });
    expect(built.wrapWidth).toBeUndefined();
    expect(built.sizeMm).toBe(10);
    expect(stateFromLayout(built.autoLayout ?? { ...state, bounds: [0, 0, 1, 1] })).toEqual(state);
  });

  it('edits the box by its size and keeps its corner', () => {
    const state = boxState([10, 20, 50, 40], 'mm');
    expect(state.width).toBe('40');
    expect(boxBounds({ ...state, height: '5' }, 'mm')).toEqual([10, 20, 50, 25]);
    expect(boxBounds({ ...state, width: '0' }, 'mm')).toBeNull();
    expect(boxErrors({ ...state, width: 'x' }, 'mm').width).toBeDefined();
  });

  it('draws a box from a drag and ignores a click', () => {
    expect(boxFromDrag({ x: 30, y: 5 }, { x: 10, y: 25 }, 1)).toEqual([10, 5, 30, 25]);
    expect(boxFromDrag({ x: 30, y: 5 }, { x: 30.2, y: 5.1 }, 1)).toBeNull();
  });
});
