import { describe, expect, it } from 'vitest';

import { applyMatrix } from '../../lib/geometry';
import {
  individualMatrices,
  initialSetSizeState,
  setSizeField,
  setSizeMatrixFromState,
  setSizeModeIcon,
  sizeStateFromSelection,
  validateSetSize,
} from './model';

const box: [number, number, number, number] = [10, 10, 30, 20];

describe('set size form model', () => {
  it('shows the current size', () => {
    const state = sizeStateFromSelection(initialSetSizeState('mm'), 'mm', box);
    expect(state.width).toBe('20');
    expect(state.height).toBe('10');
  });

  it('keeps the proportions when linked', () => {
    const state = sizeStateFromSelection(initialSetSizeState('mm'), 'mm', box);
    const wider = setSizeField(state, 'mm', box, 'width', '40');
    expect(wider.height).toBe('20');
    const free = setSizeField({ ...state, link: false }, 'mm', box, 'width', '40');
    expect(free.height).toBe('10');
  });

  it('scales about the anchor', () => {
    const state = {
      ...sizeStateFromSelection(initialSetSizeState('mm'), 'mm', box),
      width: '40',
      height: '10',
    };
    const m = setSizeMatrixFromState({ ...state, anchor: 'bottomLeft' }, 'mm', {
      angle: 0,
      extent: box,
    });
    expect(m === null ? null : applyMatrix(m, { x: 30, y: 20 })).toEqual({ x: 50, y: 20 });
  });

  it('sizes a rotated selection along its own frame', () => {
    // A 20 x 10 box turned by 90 degrees: the frame width runs up the job Y axis.
    const frame = { angle: 90, extent: [0, 0, 20, 10] as [number, number, number, number] };
    const state = sizeStateFromSelection(initialSetSizeState('mm'), 'mm', frame.extent);
    expect(state.width).toBe('20');
    const m = setSizeMatrixFromState(
      { ...state, width: '40', height: '10', link: false, anchor: 'bottomLeft' },
      'mm',
      frame,
    );
    // The frame corner (20, 0) sits at job (0, 20) and moves out to (0, 40).
    const moved = m === null ? null : applyMatrix(m, { x: 0, y: 20 });
    expect(moved === null ? null : [Math.round(moved.x * 1e6) / 1e6, moved.y]).toEqual([0, 40]);
  });

  it('rejects zero sizes', () => {
    expect(
      validateSetSize({ ...initialSetSizeState('mm'), width: '0', height: '5' }, 'mm').width,
    ).toBeDefined();
  });

  it('scales items individually about their own anchors', () => {
    const state = {
      ...initialSetSizeState('mm'),
      individually: true,
      anchor: 'bottomLeft' as const,
      width: '40',
      height: '10',
    };
    const out = individualMatrices(
      state,
      'mm',
      [0, 0, 20, 10],
      [
        { id: 1, bbox: [0, 0, 5, 5] },
        { id: 2, bbox: [10, 0, 20, 10] },
      ],
    );
    const second = out?.[1];
    // Twice as wide about its own bottom left corner.
    expect(second === undefined ? null : applyMatrix(second.matrix, { x: 20, y: 10 })).toEqual({
      x: 30,
      y: 10,
    });
  });

  it('starts with Auto Scale Z off, so heights stay unless asked', () => {
    expect(initialSetSizeState('mm').autoScaleZ).toBe(false);
  });
});

describe('setSizeModeIcon', () => {
  it('pictures the chosen mode with an icon that draws', async () => {
    const { loadExtraIcons, toolIcon } = await import('../../shell/ToolIcons/ToolIcons');
    await loadExtraIcons();
    expect(setSizeModeIcon(false)).toBe('sizeAsOne');
    expect(setSizeModeIcon(true)).toBe('sizeIndividually');
    expect(toolIcon(setSizeModeIcon(false))).not.toBeNull();
    expect(toolIcon(setSizeModeIcon(true))).not.toBeNull();
  });
});
