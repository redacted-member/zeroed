// A kernel client for unit tests: `answer` replies to the commands a test
// needs, every other call fails loudly. Tests install it with
// `setKernelForTests` and remove it afterwards.

import type { KernelClient } from '../bridge/types';

export function fakeKernel(answer: (name: string, args: unknown) => unknown): KernelClient {
  const unused = () => Promise.reject(new Error('not available in unit tests'));
  const noop = () => Promise.resolve(() => undefined);
  return {
    kind: 'test',
    // Async, so an answer that throws becomes a refused call.
    call: async <T>(name: string, args?: unknown) => answer(name, args) as T,
    callBlob: unused,
    cancel: () => Promise.resolve(false),
    onMenuCommand: noop,
    onProgress: noop,
    chooseOpenPath: () => Promise.resolve(null),
    chooseSavePath: () => Promise.resolve(null),
    confirm: () => Promise.resolve(true),
  };
}
