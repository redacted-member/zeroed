// A click in the 2D view picks what it hits by asking the kernel, which
// takes a moment. A key pressed in that moment (an arrow nudge, Delete, a
// transform key) must act on what the click picked, not on the selection
// before it, so keys wait for the picks in flight.

let inFlight = 0;
let settled: Promise<void> = Promise.resolve();

/** Tracks a pick (hit test and selection) until it settles. */
export function trackPick<T>(work: Promise<T>): Promise<T> {
  inFlight += 1;
  const done = work.then(
    () => undefined,
    () => undefined,
  );
  const previous = settled;
  settled = Promise.all([previous, done]).then(() => undefined);
  void done.then(() => {
    inFlight -= 1;
  });
  return work;
}

/** True while a click's pick is still on its way. */
export function picking(): boolean {
  return inFlight > 0;
}

/** Settles once every pick started so far has. */
export function afterPicks(): Promise<void> {
  return settled;
}
