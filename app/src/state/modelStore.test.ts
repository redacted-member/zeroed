import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import type { BlobHeader, BlobReply } from '../bridge/types';
import { fakeKernel } from '../testing/fakeKernel';
import { useModelStore } from './modelStore';

describe('model store refresh', () => {
  let stamp = 1;
  const fetched: string[] = [];
  let during: (() => void) | null = null;
  const blob = (name: string): BlobReply<BlobHeader> => {
    fetched.push(name);
    during?.();
    const header = { width: 2, height: 1, any: true, min: 0, max: 1, sections: [] };
    return name === 'model.composite'
      ? { header, sections: { z: Float32Array.from([0, 1]) } }
      : {
          header,
          sections: { palette: new Uint8Array(8), index: Uint8Array.from([0, 1]) },
        };
  };
  beforeAll(() => {
    const kernel = fakeKernel((name) => {
      if (name === 'model.list') {
        return {
          stamp,
          components: [
            { id: 1, name: 'a' },
            { id: 2, name: 'b' },
          ],
          levels: [],
          activeLevel: null,
        };
      }
      throw new Error(`unexpected ${name}`);
    });
    setKernelForTests({
      ...kernel,
      callBlob: async <H extends BlobHeader>(name: string) =>
        (await Promise.resolve(blob(name))) as unknown as BlobReply<H>,
    });
  });
  afterAll(() => setKernelForTests(null));

  it('fetches the composite and colours once however many ask at once', async () => {
    const { refresh } = useModelStore.getState();
    await Promise.all([refresh(), refresh(), refresh()]);
    expect(fetched).toEqual(['model.composite', 'model.colors']);
    expect(useModelStore.getState().stamp).toBe(1);

    // A model change seen by two mirrors at once.
    stamp = 2;
    fetched.length = 0;
    await Promise.all([refresh(), refresh()]);
    expect(fetched).toEqual(['model.composite', 'model.colors']);
    expect(useModelStore.getState().stamp).toBe(2);
    expect(useModelStore.getState().colors?.index).toEqual(Uint8Array.from([0, 1]));

    // Nothing changed: no large fetch.
    fetched.length = 0;
    await refresh();
    expect(fetched).toEqual([]);
  });

  it('sees a change made while a refresh was under way', async () => {
    const { refresh } = useModelStore.getState();
    fetched.length = 0;
    const first = refresh();
    stamp = 3;
    const second = refresh();
    await Promise.all([first, second]);
    expect(useModelStore.getState().stamp).toBe(3);
    expect(fetched).toEqual(['model.composite', 'model.colors']);
  });

  it('keeps a selection made while the model was being fetched', async () => {
    const { refresh, select } = useModelStore.getState();
    select(1);
    stamp = 4;
    during = () => select(2);
    await refresh();
    during = null;
    expect(useModelStore.getState().selectedId).toBe(2);
  });
});
