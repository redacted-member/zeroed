import { describe, expect, it } from 'vitest';

import { PreviewSession } from './previewSession';

describe('PreviewSession', () => {
  it('passes each call the revision the previous one answered', async () => {
    const seen: (number | undefined)[] = [];
    const session = new PreviewSession(async () => undefined);
    let revision = 10;
    const call = async (replace: number | undefined) => {
      seen.push(replace);
      revision += 1;
      return { revision };
    };
    await session.run(call);
    await session.run(call);
    expect(seen).toEqual([undefined, 11]);
    expect(session.previewing).toBe(true);
    await session.keep();
    await session.run(call);
    expect(seen).toEqual([undefined, 11, undefined]);
  });

  it('keeps only the newest call while one runs', async () => {
    const order: string[] = [];
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const session = new PreviewSession(async () => undefined);
    const first = session.run(async () => {
      order.push('first');
      await gate;
      return { revision: 1 };
    });
    void session.run(async () => {
      order.push('second');
      return { revision: 2 };
    });
    void session.run(async () => {
      order.push('third');
      return { revision: 3 };
    });
    release();
    await first;
    expect(order).toEqual(['first', 'third']);
  });

  it('reverts the previewed step on discard and reports failures', async () => {
    const reverted: number[] = [];
    const errors: unknown[] = [];
    const session = new PreviewSession(
      async (revision) => {
        reverted.push(revision);
      },
      (error) => errors.push(error),
    );
    await session.discard();
    expect(reverted).toEqual([]);
    await session.run(async () => ({ revision: 7 }));
    await session.discard();
    expect(reverted).toEqual([7]);
    expect(session.previewing).toBe(false);
    await session.run(async () => {
      throw new Error('no');
    });
    expect(errors).toHaveLength(1);
    expect(session.previewing).toBe(false);
  });
});
