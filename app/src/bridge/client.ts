// Picks the kernel client that suits the host (the desktop shell, or the dev
// host over HTTP in a browser) and hands the same interface to the rest of the
// front end.

import { tracked } from './activity';
import { createHttpKernel } from './http';
import { createTauriKernel } from './tauri';
import type { KernelClient } from './types';

/** True when the page runs inside the desktop shell. */
export function isTauriHost(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

let client: KernelClient | null = null;

/** The kernel client for this session. Created once. */
export function getKernel(): KernelClient {
  if (client === null) {
    client = tracked(isTauriHost() ? createTauriKernel() : createHttpKernel());
  }
  return client;
}

/** Replaces the kernel client (null restores the host default). Unit tests
 * use it with testing/fakeKernel. */
export function setKernelForTests(fake: KernelClient | null): void {
  client = fake;
}
