import { describe, expect, it } from 'vitest';

import { oldestCall, serial } from './activity';

function later<T>(ms: number, value: T, log: string[], name: string): () => Promise<T> {
  return () => {
    log.push(`start ${name}`);
    return new Promise((resolve) =>
      setTimeout(() => {
        log.push(`end ${name}`);
        resolve(value);
      }, ms),
    );
  };
}

describe('kernel call order', () => {
  // The real app ran 60 calls sent together in a shuffled order (each
  // request runs on a host thread of its own).
  it('runs calls one at a time in the order they were made', async () => {
    const log: string[] = [];
    const slow = serial(later(30, 'a', log, 'a'));
    const quick = serial(later(0, 'b', log, 'b'));
    const failing = serial(() => {
      log.push('start c');
      return Promise.reject(new Error('refused'));
    });
    const after = serial(later(0, 'd', log, 'd'));
    expect(log).toEqual([]);
    expect(await slow).toBe('a');
    expect(await quick).toBe('b');
    await expect(failing).rejects.toThrow('refused');
    expect(await after).toBe('d');
    expect(log).toEqual(['start a', 'end a', 'start b', 'end b', 'start c', 'start d', 'end d']);
  });

  it('times a call from when it starts, not from when it was queued', async () => {
    const log: string[] = [];
    const first = serial(later(40, 1, log, 'first'));
    let queuedAt = 0;
    const second = serial(() => {
      queuedAt = oldestCall() ?? -1;
      return Promise.resolve(2);
    });
    const firstStart = await new Promise<number | null>((resolve) =>
      setTimeout(() => resolve(oldestCall()), 0),
    );
    await first;
    await second;
    expect(firstStart).not.toBeNull();
    expect(queuedAt - (firstStart ?? 0)).toBeGreaterThanOrEqual(39);
    expect(oldestCall()).toBeNull();
  });
});
