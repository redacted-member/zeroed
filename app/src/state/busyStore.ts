// The status bar's progress: shown once a kernel call has been running
// longer than a moment, with how far the kernel says it has got (read from
// the host while the call runs). Esc asks the running command to stop.
// While it shows, the window takes no other input, as a modal progress
// would: other keys and menu commands are dropped and the BusyShield
// covers the window against the pointer.

import { create } from 'zustand';

import { oldestCall, watchCalls } from '../bridge/activity';
import { getKernel } from '../bridge/client';

/** A call running longer than this shows the progress, milliseconds. */
export const BUSY_AFTER_MS = 300;
/** How often the progress is read while it shows, milliseconds. */
const POLL_MS = 100;

interface BusyState {
  /** A kernel call has been running longer than {@link BUSY_AFTER_MS}. */
  busy: boolean;
  /** How far it has got, 0 to 1; null while it does not say. */
  progress: number | null;
  /** Esc was pressed and the stop asked for. */
  stopping: boolean;
}

export const useBusyStore = create<BusyState>(() => ({
  busy: false,
  progress: null,
  stopping: false,
}));

/** Whether the calls in flight, the oldest started at `oldest`, count as
 * busy at `now`. */
export function isBusy(oldest: number | null, now: number): boolean {
  return oldest !== null && now - oldest >= BUSY_AFTER_MS;
}

/** The progress to show for the kernel's answer: null (no figure) until
 * it reports any. */
export function shownProgress(running: boolean, progress: number): number | null {
  return running && progress > 0 ? Math.min(1, progress) : null;
}

let timer: ReturnType<typeof setTimeout> | null = null;

function poll(): void {
  timer = null;
  const oldest = oldestCall();
  if (oldest === null) {
    useBusyStore.setState({ busy: false, progress: null, stopping: false });
    return;
  }
  const now = performance.now();
  if (!isBusy(oldest, now)) {
    timer = setTimeout(poll, BUSY_AFTER_MS - (now - oldest));
    return;
  }
  if (!useBusyStore.getState().busy) {
    useBusyStore.setState({ busy: true, progress: null });
  }
  const read = getKernel().activity?.();
  void read
    ?.then((activity) => {
      if (useBusyStore.getState().busy) {
        useBusyStore.setState({ progress: shownProgress(activity.running, activity.progress) });
      }
    })
    .catch(() => undefined);
  timer = setTimeout(poll, POLL_MS);
}

function onCalls(): void {
  if (timer === null) {
    poll();
  }
}

/** Whether the window takes input (keys other than Esc, menu commands). */
export function acceptsInput(): boolean {
  return !useBusyStore.getState().busy;
}

/** What a key pressed does: nothing special while the window takes input;
 * while busy Esc stops the command and every other key is dropped. */
export function keyAction(busy: boolean, key: string): 'pass' | 'block' | 'stop' {
  if (!busy) {
    return 'pass';
  }
  return key === 'Escape' ? 'stop' : 'block';
}

function onKey(event: KeyboardEvent): void {
  const action = keyAction(!acceptsInput(), event.key);
  if (action === 'pass') {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  if (action === 'block') {
    return;
  }
  // While a long command runs, Esc stops it rather than reaching the view.
  useBusyStore.setState({ stopping: true });
  void getKernel()
    .cancel(0)
    .catch(() => undefined);
}

if (typeof window !== 'undefined') {
  watchCalls(onCalls);
  window.addEventListener('keydown', onKey, true);
}
