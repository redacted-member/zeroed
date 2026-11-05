import { describe, expect, it } from 'vitest';

import { useUiStore } from '../state/uiStore';
import { showDrawingView } from './useDrawController';

describe('showDrawingView', () => {
  it('brings the 2D view up in place of the 3D view when a drawing form opens', () => {
    useUiStore.setState({ viewMode: '3d' });
    showDrawingView();
    expect(useUiStore.getState().viewMode).toBe('2d');
  });

  it('leaves a split layout and the 2D view as they are', () => {
    useUiStore.setState({ viewMode: 'split' });
    showDrawingView();
    expect(useUiStore.getState().viewMode).toBe('split');
    useUiStore.setState({ viewMode: '2d' });
    showDrawingView();
    expect(useUiStore.getState().viewMode).toBe('2d');
  });
});
