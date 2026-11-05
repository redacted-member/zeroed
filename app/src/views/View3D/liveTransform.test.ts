import { describe, expect, it } from 'vitest';

import type { ComponentTransformParams } from '../../bridge/model';
import { LiveTransform } from './liveTransform';
import { isIdentity } from './transformHandles';

const moveBy = (x: number): ComponentTransformParams => ({
  offset: { x, y: 0 },
  scaleX: 1,
  scaleY: 1,
  rotateDeg: 0,
});

describe('live component transform', () => {
  it('sends one request at a time, replacing the previous step', async () => {
    const sent: Array<[number, number | undefined]> = [];
    let revision = 10;
    const live = new LiveTransform(
      (params, replace) => {
        sent.push([params.offset.x, replace]);
        revision += 1;
        return Promise.resolve(revision);
      },
      () => Promise.resolve(),
      () => undefined,
      isIdentity,
    );
    live.update(moveBy(0));
    live.update(moveBy(1));
    live.update(moveBy(2));
    live.update(moveBy(3));
    await live.finish();
    expect(live.applied).toBe(true);
    // The first request goes at once; the moves made while it runs
    // collapse into the newest, which replaces the first step.
    expect(sent).toEqual([
      [1, undefined],
      [3, 11],
    ]);
  });

  it('sends nothing for a drag that never moved', async () => {
    let calls = 0;
    const live = new LiveTransform(
      () => {
        calls += 1;
        return Promise.resolve(1);
      },
      () => Promise.resolve(),
      () => undefined,
      isIdentity,
    );
    live.update(moveBy(0));
    await live.finish();
    expect(calls).toBe(0);
    expect(live.applied).toBe(false);
  });

  it('reports a failure and carries on', async () => {
    const errors: unknown[] = [];
    let first = true;
    const live = new LiveTransform(
      () => {
        if (first) {
          first = false;
          return Promise.reject(new Error('empty'));
        }
        return Promise.resolve(5);
      },
      () => Promise.resolve(),
      (error) => errors.push(error),
      isIdentity,
    );
    live.update(moveBy(1));
    live.update(moveBy(2));
    await live.finish();
    expect(errors).toHaveLength(1);
    expect(live.applied).toBe(true);
  });
});
