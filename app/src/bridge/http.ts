// Development client: talks to the real kernel run by `carve-devhost` over
// HTTP, so the UI can be exercised against real geometry in a plain browser.
// Used whenever the page runs in a plain browser (`pnpm dev`). Vite proxies
// `/kernel` to the dev host.

import { browserDialogs } from './browserDialogs';
import { blobResult, jsonResult, splitFrame, type Framed } from './frame';
import {
  KernelError,
  type BlobHeader,
  type BlobReply,
  type KernelActivity,
  type KernelClient,
} from './types';

async function post(name: string, args: unknown): Promise<Framed> {
  let response: Response;
  try {
    response = await fetch(`/kernel/call?name=${encodeURIComponent(name)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: args === undefined ? '' : JSON.stringify(args),
    });
  } catch (error) {
    throw new KernelError(name, String(error), 'transport');
  }
  if (!response.ok) {
    throw new KernelError(name, `the dev host answered ${response.status}`, 'transport');
  }
  return splitFrame(name, await response.arrayBuffer());
}

/** Asks the dev host about the running command, beside it. */
async function host<T>(path: string): Promise<T> {
  const response = await fetch(`/kernel/${path}`, { method: 'POST', body: '' });
  if (!response.ok) {
    throw new KernelError(path, `the dev host answered ${response.status}`, 'transport');
  }
  return (await response.json()) as T;
}

/** Creates the client used when the page runs against carve-devhost. */
export function createHttpKernel(): KernelClient {
  return {
    kind: 'http',
    call: async <T>(name: string, args?: unknown) => jsonResult<T>(name, await post(name, args)),
    callBlob: async <H extends BlobHeader>(name: string, args?: unknown): Promise<BlobReply<H>> =>
      blobResult<H>(name, await post(name, args)),
    cancel: async () => (await host<{ running: boolean }>('cancel')).running,
    activity: () => host<KernelActivity>('progress'),
    onMenuCommand: async () => () => {},
    onProgress: async () => () => {},
    ...browserDialogs,
  };
}
