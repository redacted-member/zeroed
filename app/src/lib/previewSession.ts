// A form's live preview of a model change: each call replaces the step the
// previous call made (the kernel takes it back when nothing else changed
// since), so however often a slider moves the form leaves one undo step.
// Calls never overlap: while one runs, only the newest waiting call is kept.

import type { Revision } from '../bridge/document';

/** A previewing kernel call, given the revision of the step it replaces. */
export type PreviewCall = (replace: number | undefined) => Promise<Revision>;

export class PreviewSession {
  /** Revision the last previewed step answered, or null when none is up. */
  private token: number | null = null;
  private running: Promise<void> | null = null;
  private waiting: PreviewCall | null = null;

  constructor(
    private readonly revert: (revision: number) => Promise<unknown>,
    private readonly onError: (error: unknown) => void = () => undefined,
  ) {}

  /** Whether a previewed step is up and not yet kept. */
  get previewing(): boolean {
    return this.token !== null;
  }

  /** Runs `call` now, or after the one running, replacing any waiting call. */
  run(call: PreviewCall): Promise<void> {
    this.waiting = call;
    if (this.running === null) {
      this.running = this.drain().finally(() => {
        this.running = null;
      });
    }
    return this.running;
  }

  private async drain(): Promise<void> {
    while (this.waiting !== null) {
      const call = this.waiting;
      this.waiting = null;
      try {
        const reply = await call(this.token ?? undefined);
        this.token = reply.revision;
      } catch (error) {
        this.onError(error);
      }
    }
  }

  /** Keeps the previewed step; the next call makes a new one. */
  async keep(): Promise<void> {
    await this.running;
    this.token = null;
  }

  /** Takes the previewed step back, if it is still the newest. */
  async discard(): Promise<void> {
    this.waiting = null;
    await this.running;
    const token = this.token;
    this.token = null;
    if (token !== null) {
      try {
        await this.revert(token);
      } catch (error) {
        this.onError(error);
      }
    }
  }
}
