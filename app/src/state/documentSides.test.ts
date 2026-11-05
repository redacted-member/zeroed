import { afterEach, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import type { Side } from '../bridge/types';
import { fakeKernel } from '../testing/fakeKernel';
import { useDocumentStore } from './documentStore';

afterEach(() => setKernelForTests(null));

/** A kernel holding a double sided job with the journal the real one keeps:
 * each step notes the side it was made on, a side switch is no step, and
 * Undo turns to the side of the step it takes back. */
function sidedKernel() {
  const state = {
    side: 'top' as Side,
    revision: 1,
    dirty: false,
    entities: { top: [1], bottom: [2] } as Record<Side, number[]>,
    undo: [
      { label: 'Rect', side: 'top' as Side, id: 1 },
      { label: 'Rect', side: 'bottom' as Side, id: 2 },
    ],
  };
  const undoInfo = () => ({
    canUndo: state.undo.length > 0,
    canRedo: false,
    undoLabel: state.undo[state.undo.length - 1]?.label ?? null,
    redoLabel: null,
  });
  setKernelForTests(
    fakeKernel((name, args) => {
      switch (name) {
        case 'doc.info':
          return {
            revision: state.revision,
            path: '/jobs/sign.carve',
            dirty: state.dirty,
            activeLayer: 1,
            activeSheet: 1,
            activeSide: state.side,
            counts: { entities: state.entities[state.side].length, components: 0, toolpaths: 0 },
          };
        case 'job.get':
          return { kind: 'doubleSided' };
        case 'layers.list':
        case 'sheets.list':
          return [];
        case 'entities.list':
          return state.entities[state.side].map((id) => ({ id }));
        case 'selection.get':
          return { ids: [], bbox: null };
        case 'undo.info':
          return undoInfo();
        case 'job.setSide':
          state.side = (args as { side: Side }).side;
          state.revision += 1;
          return { revision: state.revision, side: state.side };
        case 'undo': {
          const step = state.undo.pop();
          if (step !== undefined) {
            state.side = step.side;
            state.entities[step.side] = state.entities[step.side].filter((id) => id !== step.id);
            state.revision += 1;
            state.dirty = true;
          }
          return { revision: state.revision, label: step?.label ?? null };
        }
        default:
          throw new Error(name);
      }
    }),
  );
}

describe('switching the side of a two sided job', () => {
  it('is a view change: Undo still offers the last edit and the job is unchanged', async () => {
    sidedKernel();
    const store = useDocumentStore.getState();
    await store.refresh();
    await store.setSide('bottom');
    let state = useDocumentStore.getState();
    expect(state.activeSide).toBe('bottom');
    expect(state.entities.map((e) => e.id)).toEqual([2]);
    expect(state.undo.undoLabel).toBe('Rect');
    expect(state.dirty).toBe(false);
    await store.setSide('top');
    await store.setSide('bottom');
    await store.setSide('top');
    expect(useDocumentStore.getState().undo.canUndo).toBe(true);
    // Undo takes back the bottom rectangle, the last edit, and shows its side.
    await store.undoLast();
    state = useDocumentStore.getState();
    expect(state.lastError).toBeNull();
    expect(state.activeSide).toBe('bottom');
    expect(state.entities).toEqual([]);
    // The next Undo takes back the top rectangle on the top side.
    await store.undoLast();
    state = useDocumentStore.getState();
    expect(state.activeSide).toBe('top');
    expect(state.entities).toEqual([]);
    expect(state.undo.canUndo).toBe(false);
  });
});
