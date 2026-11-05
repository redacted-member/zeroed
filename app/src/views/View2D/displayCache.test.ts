import { afterEach, describe, expect, it, vi } from 'vitest';

import { api } from '../../bridge/api';
import type { DisplayReply } from '../../bridge/document';
import { DisplayCache } from './displayCache';

const EMPTY: DisplayReply = { revision: 1, entities: [], removed: [], xy: new Float32Array(0) };

describe('display cache', () => {
  afterEach(() => vi.restoreAllMocks());

  it('fetches the finer curves a zoom asks for while another fetch runs', async () => {
    const asked: Array<number | undefined> = [];
    const replies: Array<() => void> = [];
    vi.spyOn(api.entities, 'display').mockImplementation((args) => {
      asked.push(args.tolerance);
      return new Promise((resolve) => replies.push(() => resolve(EMPTY)));
    });
    const cache = new DisplayCache();
    // The document loads at the start up zoom...
    const first = cache.sync(1, 0.3);
    // ...and the view zooms in to the job before that fetch is back.
    let zoomDone = false;
    const zoom = cache.sync(1, 0.05, true).then(() => {
      zoomDone = true;
    });
    replies.shift()?.();
    await first;
    await vi.waitFor(() => expect(asked).toEqual([0.3, 0.05]));
    // The zoom's caller hears back only once the finer curves are in.
    expect(zoomDone).toBe(false);
    replies.shift()?.();
    await zoom;
    expect(zoomDone).toBe(true);
  });

  it('runs one trailing fetch for several calls that waited', async () => {
    const asked: Array<number | undefined> = [];
    vi.spyOn(api.entities, 'display').mockImplementation((args) => {
      asked.push(args.tolerance);
      return Promise.resolve({ ...EMPTY, revision: asked.length === 1 ? 1 : 3 });
    });
    const cache = new DisplayCache();
    const calls = [cache.sync(1, 0.3), cache.sync(2, 0.2), cache.sync(3, 0.1)];
    await Promise.all(calls);
    expect(asked).toEqual([0.3, 0.1]);
    expect(cache.loadedRevision()).toBe(3);
  });
});
