// Create Text applies every change at once: the first valid text makes the
// entity, later changes lay it out again. One request is in flight at a
// time; changes made meanwhile collapse into the newest. Each edit passes
// back the revision the previous call answered, so the kernel folds the
// whole session into one undo step.

import type { PlainTextArgs } from '../../bridge/document';

export interface LiveTextIo {
  /** Makes the text (and selects it); answers its id and the revision after. */
  create: (args: PlainTextArgs) => Promise<{ id: number; revision: number }>;
  /** Lays text `id` out again; answers the new revision. */
  update: (id: number, args: PlainTextArgs, merge: number | null) => Promise<number>;
  /** Called when the text being edited changes (after a create). */
  onTarget: (id: number | null) => void;
  onError: (error: unknown) => void;
}

/** Whether two sets of text arguments lay out the same. */
export function sameText(a: PlainTextArgs | null, b: PlainTextArgs | null): boolean {
  return a !== null && b !== null && JSON.stringify(a) === JSON.stringify(b);
}

export class LiveText {
  private id: number | null = null;
  private revision: number | null = null;
  private applied: PlainTextArgs | null = null;
  private latest: PlainTextArgs | null = null;
  private busy = false;
  /** Moves on every retarget, so replies for an older text are ignored. */
  private generation = 0;
  private idle: Array<() => void> = [];

  constructor(private readonly io: LiveTextIo) {}

  /** The text being edited, or null before the first change makes one. */
  get target(): number | null {
    return this.id;
  }

  /**
   * Edits text `id` from now on (null: the next change makes new text).
   * `current` is its layout as loaded, which is not sent again.
   */
  retarget(id: number | null, current: PlainTextArgs | null = null): void {
    this.generation += 1;
    this.id = id;
    this.revision = null;
    this.applied = current;
    this.latest = null;
  }

  /** The layout the form asks for now. */
  apply(args: PlainTextArgs): void {
    if (sameText(args, this.applied) && this.latest === null) {
      return;
    }
    this.latest = args;
    void this.pump();
  }

  /** Resolves once every change asked for has been sent. */
  settled(): Promise<void> {
    if (!this.busy && this.latest === null) {
      return Promise.resolve();
    }
    return new Promise((resolve) => this.idle.push(resolve));
  }

  private async pump(): Promise<void> {
    if (this.busy) {
      return;
    }
    const args = this.latest;
    this.latest = null;
    if (args === null || sameText(args, this.applied)) {
      const waiting = this.idle;
      this.idle = [];
      waiting.forEach((resolve) => resolve());
      return;
    }
    this.busy = true;
    const generation = this.generation;
    try {
      if (this.id === null) {
        const made = await this.io.create(args);
        if (generation === this.generation) {
          this.id = made.id;
          this.revision = made.revision;
          this.applied = args;
          this.io.onTarget(made.id);
        }
      } else {
        const revision = await this.io.update(this.id, args, this.revision);
        if (generation === this.generation) {
          this.revision = revision;
          this.applied = args;
        }
      }
    } catch (error) {
      this.io.onError(error);
    } finally {
      this.busy = false;
    }
    void this.pump();
  }
}
