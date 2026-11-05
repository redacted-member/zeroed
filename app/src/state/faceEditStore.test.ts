import { describe, expect, it, vi } from 'vitest';

import { leaveFace, useFaceEditStore } from './faceEditStore';

describe('leaveFace', () => {
  it('goes back where the face was opened from', () => {
    const back = vi.fn();
    const fallback = vi.fn();
    useFaceEditStore
      .getState()
      .setEditing({ design: 1, panel: 'door#0', label: 'Door 1, front face', back });
    leaveFace(fallback);
    expect(back).toHaveBeenCalledTimes(1);
    expect(fallback).not.toHaveBeenCalled();
    expect(useFaceEditStore.getState().editing).toBeNull();
  });

  it('goes to the Assembly View otherwise', () => {
    const fallback = vi.fn();
    useFaceEditStore.getState().setEditing({ design: 1, panel: 'door#0', label: 'Door 1' });
    leaveFace(fallback);
    expect(fallback).toHaveBeenCalledTimes(1);
    expect(useFaceEditStore.getState().editing).toBeNull();
  });
});
