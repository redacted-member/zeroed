// A cursor that follows what a click would do where the pointer is: each
// pointer move asks `probe` (usually the kernel), one request at a time,
// moves made meanwhile collapsing into the newest.

import type { Point } from '../bridge/document';

export class HoverProbe {
  private current: string;
  private busy = false;
  private next: Point | null = null;

  constructor(
    private readonly probe: (at: Point) => Promise<string>,
    private readonly fallback: string,
  ) {
    this.current = fallback;
  }

  /** The cursor for the last place probed. */
  get cursor(): string {
    return this.current;
  }

  /** The pointer is now at `at`. */
  move(at: Point): void {
    this.next = at;
    void this.pump();
  }

  /** Forgets the last answer (the pointer left, or the form changed). */
  reset(): void {
    this.next = null;
    this.current = this.fallback;
  }

  /** Resolves once no probe is waiting (for tests). */
  async settle(): Promise<void> {
    while (this.busy || this.next !== null) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  }

  private async pump(): Promise<void> {
    if (this.busy || this.next === null) {
      return;
    }
    const at = this.next;
    this.next = null;
    this.busy = true;
    try {
      this.current = await this.probe(at);
    } catch {
      this.current = this.fallback;
    } finally {
      this.busy = false;
    }
    void this.pump();
  }
}
