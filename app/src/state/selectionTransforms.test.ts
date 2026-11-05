import { afterEach, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import type { Matrix } from '../bridge/document';
import { fakeKernel } from '../testing/fakeKernel';
import { useDocumentStore } from './documentStore';
import { onSelectionTransformed } from './selectionTransforms';

const MOVE: Matrix = [1, 0, 0, 1, 10, 5];

/** A kernel that takes transforms and copies, and refuses the rest (the
 * refresh after each, which the store shrugs off). */
function kernel(calls: string[]) {
  return fakeKernel((name) => {
    calls.push(name);
    if (name.startsWith('undo.') && name !== 'undo.info') {
      return {};
    }
    if (name === 'entities.transform' || name === 'ops.move') {
      return { revision: 2 };
    }
    if (name === 'model.copyMatrix') {
      return { revision: 2, ids: [] };
    }
    throw new Error(`not in this test: ${name}`);
  });
}

describe('selection transforms', () => {
  afterEach(() => setKernelForTests(null));

  it('tell the view of a whole selection move, copy or nudge, not of one item', async () => {
    const calls: string[] = [];
    setKernelForTests(kernel(calls));
    useDocumentStore.setState({
      selection: { ids: [1, 2], bbox: [0, 0, 10, 10] },
    });
    const seen: Matrix[] = [];
    const stop = onSelectionTransformed((m) => seen.push(m));
    const store = useDocumentStore.getState();
    await store.transformSelection(MOVE);
    await store.transformSelectionCopy(MOVE);
    await store.moveSelection(3, 4);
    // A form moving one object of several (Set Size per item, Align) is
    // not the selection moving.
    await store.transformSelection(MOVE, [1]);
    await store.transformSelectionCopy(MOVE, [1]);
    stop();
    await store.transformSelection(MOVE);
    expect(seen).toEqual([MOVE, MOVE, [1, 0, 0, 1, 3, 4]]);
    expect(calls).toContain('entities.transform');
    expect(calls).toContain('model.copyMatrix');
  });
});
