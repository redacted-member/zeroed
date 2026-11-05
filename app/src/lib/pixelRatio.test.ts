import { describe, expect, it } from 'vitest';

import { watchPixelRatio, type PixelRatioHost } from './pixelRatio';

function fakeHost() {
  const listeners = new Map<string, () => void>();
  const queries: string[] = [];
  const host = {
    devicePixelRatio: 1,
    matchMedia(query: string) {
      queries.push(query);
      return {
        addEventListener: (_: string, fn: () => void) => listeners.set(query, fn),
        removeEventListener: () => listeners.delete(query),
      };
    },
  };
  return {
    host: host as unknown as PixelRatioHost & { devicePixelRatio: number },
    listeners,
    queries,
  };
}

describe('pixel ratio watcher', () => {
  it('reports each change of display ratio and rearms for the new one', () => {
    const { host, listeners, queries } = fakeHost();
    const seen: number[] = [];
    const stop = watchPixelRatio((ratio) => seen.push(ratio), host);
    expect(queries).toEqual(['(resolution: 1dppx)']);
    host.devicePixelRatio = 2;
    listeners.get('(resolution: 1dppx)')?.();
    expect(seen).toEqual([2]);
    expect(queries[1]).toBe('(resolution: 2dppx)');
    stop();
    expect(listeners.size).toBe(0);
  });
});
