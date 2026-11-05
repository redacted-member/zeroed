import { describe, expect, it } from 'vitest';

import {
  BUSY_AFTER_MS,
  acceptsInput,
  isBusy,
  keyAction,
  shownProgress,
  useBusyStore,
} from './busyStore';

describe('busy progress', () => {
  it('shows once the oldest call has run a while', () => {
    expect(isBusy(null, 1000)).toBe(false);
    expect(isBusy(1000, 1000 + BUSY_AFTER_MS - 1)).toBe(false);
    expect(isBusy(1000, 1000 + BUSY_AFTER_MS)).toBe(true);
  });

  it('gives a figure only once the kernel reports one', () => {
    expect(shownProgress(true, 0)).toBeNull();
    expect(shownProgress(false, 0.5)).toBeNull();
    expect(shownProgress(true, 0.25)).toBe(0.25);
    expect(shownProgress(true, 1.5)).toBe(1);
  });

  // A long command blocks the window as a modal progress does: only Esc
  // reaches it, and it asks the command to stop.
  it('takes no keys but Esc while busy', () => {
    expect(keyAction(false, 'Delete')).toBe('pass');
    expect(keyAction(false, 'Escape')).toBe('pass');
    expect(keyAction(true, 'Delete')).toBe('block');
    expect(keyAction(true, 'z')).toBe('block');
    expect(keyAction(true, 'Escape')).toBe('stop');
    useBusyStore.setState({ busy: false });
    expect(acceptsInput()).toBe(true);
    useBusyStore.setState({ busy: true });
    expect(acceptsInput()).toBe(false);
    useBusyStore.setState({ busy: false });
  });
});
