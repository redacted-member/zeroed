// Requests where only the newest matters: one runs at a time, and while it
// runs the newest waiting one replaces any older one (the slider's fast
// preview). An idle timer adds a settled request after a pause, or at once
// on release (the full regenerate).

/** Runs `run` one request at a time; only the newest waiting request runs next. */
export class LatestQueue<T, R> {
  private busy = false;
  private waiting: { args: T } | null = null;
  private serial = 0;

  constructor(
    private readonly run: (args: T) => Promise<R>,
    private readonly deliver: (result: R, args: T) => void,
    private readonly fail: (error: unknown) => void = () => undefined,
  ) {}

  /** Asks for `args`; an older request still waiting is dropped. */
  push(args: T): void {
    if (this.busy) {
      this.waiting = { args };
      return;
    }
    void this.start(args);
  }

  /** Drops the waiting request and ignores the answer of the running one. */
  cancel(): void {
    this.waiting = null;
    this.serial += 1;
  }

  /** Whether a request is running or waiting. */
  get pending(): boolean {
    return this.busy || this.waiting !== null;
  }

  private async start(args: T): Promise<void> {
    this.busy = true;
    const serial = this.serial;
    try {
      const result = await this.run(args);
      // Done unless a newer request waits, so `pending` reads right while delivering.
      this.busy = this.waiting !== null;
      if (serial === this.serial) {
        this.deliver(result, args);
      }
    } catch (error) {
      this.busy = this.waiting !== null;
      if (serial === this.serial) {
        this.fail(error);
      }
    } finally {
      this.busy = false;
      const next = this.waiting;
      this.waiting = null;
      if (next !== null) {
        void this.start(next.args);
      }
    }
  }
}

/**
 * Calls `settle` with the newest change once changes pause for `idleMs`,
 * or at once on `release` (a slider let go, Enter, focus leaving).
 */
export class IdleSettler<T> {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private latest: { args: T } | null = null;

  constructor(
    private readonly settle: (args: T) => void,
    private readonly idleMs: number,
  ) {}

  /** A change: settles after the pause unless another change comes first. */
  change(args: T): void {
    this.latest = { args };
    this.clearTimer();
    this.timer = setTimeout(() => this.flush(), this.idleMs);
  }

  /** Settles the newest change now (nothing when none is waiting). */
  release(): void {
    this.flush();
  }

  /** Forgets the waiting change. */
  cancel(): void {
    this.clearTimer();
    this.latest = null;
  }

  private flush(): void {
    this.clearTimer();
    const latest = this.latest;
    this.latest = null;
    if (latest !== null) {
      this.settle(latest.args);
    }
  }

  private clearTimer(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }
}
