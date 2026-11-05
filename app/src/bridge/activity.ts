// Kernel calls run one at a time in the order they were made, and which one
// is running and since when, so the status bar can show progress once it
// has been running a while. Every client the page uses goes through
// `tracked`. The hosts run each request on a thread of its own, so calls
// sent together would otherwise reach the kernel in any order.

import type { BlobHeader, BlobReply, KernelClient } from './types';

type Listener = () => void;

let nextCall = 1;
const started = new Map<number, number>();
const listeners = new Set<Listener>();

function changed(): void {
  for (const listener of listeners) {
    listener();
  }
}

/** Settles once every call made so far has settled. */
let tail: Promise<unknown> = Promise.resolve();

async function track<T>(run: () => Promise<T>): Promise<T> {
  const id = nextCall;
  nextCall += 1;
  started.set(id, performance.now());
  changed();
  try {
    return await run();
  } finally {
    started.delete(id);
    changed();
  }
}

/** Runs `run` once every call made before it has settled; its time counts
 * from when it starts, not from when it was queued. */
export function serial<T>(run: () => Promise<T>): Promise<T> {
  const turn = tail.then(() => track(run));
  tail = turn.catch(() => undefined);
  return turn;
}

/** When the running call started (a `performance.now()` time), or null
 * with none running. */
export function oldestCall(): number | null {
  let oldest: number | null = null;
  for (const at of started.values()) {
    oldest = oldest === null ? at : Math.min(oldest, at);
  }
  return oldest;
}

/** Calls `listener` whenever a call starts or ends; answers the unsubscribe. */
export function watchCalls(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** `client` with its calls run in order and tracked. */
export function tracked(client: KernelClient): KernelClient {
  return {
    ...client,
    call: <T>(name: string, args?: unknown) => serial(() => client.call<T>(name, args)),
    callBlob: <H extends BlobHeader>(name: string, args?: unknown): Promise<BlobReply<H>> =>
      serial(() => client.callBlob<H>(name, args)),
  };
}
