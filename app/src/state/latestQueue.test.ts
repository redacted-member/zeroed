import { afterEach, describe, expect, it, vi } from 'vitest';

import { IdleSettler, LatestQueue } from './latestQueue';

/** A promise and the functions that settle it. */
function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  let reject: (error: unknown) => void = () => undefined;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

const flush = () => new Promise<void>((done) => setTimeout(done, 0));

describe('LatestQueue', () => {
  it('runs one at a time and only the newest waiting request next', async () => {
    const calls: number[] = [];
    const answers = new Map<number, ReturnType<typeof deferred<number>>>();
    const delivered: number[] = [];
    const pendingAtDelivery: boolean[] = [];
    const queue: LatestQueue<number, number> = new LatestQueue<number, number>(
      (n) => {
        calls.push(n);
        const d = deferred<number>();
        answers.set(n, d);
        return d.promise;
      },
      (result) => {
        delivered.push(result);
        pendingAtDelivery.push(queue.pending);
      },
    );
    queue.push(1);
    queue.push(2);
    queue.push(3);
    queue.push(4);
    expect(calls).toEqual([1]);
    answers.get(1)?.resolve(10);
    await flush();
    expect(delivered).toEqual([10]);
    // 2 and 3 were replaced while 1 ran.
    expect(calls).toEqual([1, 4]);
    answers.get(4)?.resolve(40);
    await flush();
    expect(delivered).toEqual([10, 40]);
    expect(pendingAtDelivery).toEqual([true, false]);
    expect(queue.pending).toBe(false);
  });

  it('reports a failure and goes on with the next request', async () => {
    const errors: unknown[] = [];
    const delivered: number[] = [];
    const first = deferred<number>();
    const queue = new LatestQueue<number, number>(
      (n) => (n === 1 ? first.promise : Promise.resolve(n * 10)),
      (r) => delivered.push(r),
      (e) => errors.push(e),
    );
    queue.push(1);
    queue.push(2);
    first.reject(new Error('refused'));
    await flush();
    await flush();
    expect(errors).toHaveLength(1);
    expect(delivered).toEqual([20]);
  });

  it('ignores the running answer and drops the waiting one after cancel', async () => {
    const delivered: number[] = [];
    const first = deferred<number>();
    const calls: number[] = [];
    const queue = new LatestQueue<number, number>(
      (n) => {
        calls.push(n);
        return first.promise;
      },
      (r) => delivered.push(r),
    );
    queue.push(1);
    queue.push(2);
    queue.cancel();
    first.resolve(10);
    await flush();
    expect(delivered).toEqual([]);
    expect(calls).toEqual([1]);
  });
});

describe('IdleSettler', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('settles the newest change after the pause, not before', () => {
    vi.useFakeTimers();
    const settled: number[] = [];
    const settler = new IdleSettler<number>((n) => settled.push(n), 400);
    settler.change(1);
    vi.advanceTimersByTime(300);
    settler.change(2);
    vi.advanceTimersByTime(399);
    expect(settled).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(settled).toEqual([2]);
    vi.advanceTimersByTime(1000);
    expect(settled).toEqual([2]);
  });

  it('settles at once on release, and only once', () => {
    vi.useFakeTimers();
    const settled: number[] = [];
    const settler = new IdleSettler<number>((n) => settled.push(n), 400);
    settler.change(5);
    settler.release();
    expect(settled).toEqual([5]);
    vi.advanceTimersByTime(400);
    settler.release();
    expect(settled).toEqual([5]);
  });

  it('forgets the waiting change on cancel', () => {
    vi.useFakeTimers();
    const settled: number[] = [];
    const settler = new IdleSettler<number>((n) => settled.push(n), 400);
    settler.change(7);
    settler.cancel();
    vi.advanceTimersByTime(400);
    expect(settled).toEqual([]);
  });
});
