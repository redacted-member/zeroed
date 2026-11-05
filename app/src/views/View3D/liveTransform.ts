// A live component drag in the 3D view: each pointer move asks the kernel
// for the change so far (a transform, or new heights), one request at a time (moves made meanwhile
// collapse into the newest), and every request after the first replaces
// the step the previous one made, so the drag ends as one undo step.

/** Sends one change, replacing the step made at `replace`; answers the
 * new revision. */
export type SendChange<T> = (params: T, replace: number | undefined) => Promise<number>;

export class LiveTransform<T> {
  private latest: T | null = null;
  private busy = false;
  private revision: number | undefined = undefined;
  private idle: Array<() => void> = [];

  constructor(
    private readonly send: SendChange<T>,
    private readonly after: () => Promise<void>,
    private readonly onError: (error: unknown) => void,
    /** True for a change that leaves the component as it was. */
    private readonly isNoop: (params: T) => boolean,
  ) {}

  /** The change the drag asks for now. */
  update(params: T): void {
    this.latest = params;
    void this.pump();
  }

  /** Resolves once every asked for transform has been applied. */
  finish(): Promise<void> {
    if (!this.busy && this.latest === null) {
      return Promise.resolve();
    }
    return new Promise((resolve) => this.idle.push(resolve));
  }

  /** Whether the drag has changed the model yet. */
  get applied(): boolean {
    return this.revision !== undefined;
  }

  private async pump(): Promise<void> {
    if (this.busy) {
      return;
    }
    const params = this.latest;
    this.latest = null;
    if (params === null) {
      const waiting = this.idle;
      this.idle = [];
      waiting.forEach((resolve) => resolve());
      return;
    }
    if (this.revision === undefined && this.isNoop(params)) {
      void this.pump();
      return;
    }
    this.busy = true;
    try {
      this.revision = await this.send(params, this.revision);
      await this.after();
    } catch (error) {
      this.onError(error);
    } finally {
      this.busy = false;
    }
    void this.pump();
  }
}
