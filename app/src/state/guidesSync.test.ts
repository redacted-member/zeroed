import { afterEach, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import { fakeKernel } from '../testing/fakeKernel';
import { useDocumentStore } from './documentStore';
import { guidesForJob, guidesFromJob, watchGuides } from './guidesSync';
import { useViewStore } from './viewStore';

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

afterEach(() => {
  setKernelForTests(null);
  useViewStore.getState().clearGuides();
});

describe('guides kept with the job', () => {
  it('turn into the file form and back', () => {
    const lines = guidesForJob([{ id: 7, axis: 'y', position: 42, locked: true }]);
    expect(lines).toEqual([{ axis: 'y', position: 42, locked: true }]);
    expect(guidesFromJob(lines, 3)).toEqual([{ id: 3, axis: 'y', position: 42, locked: true }]);
  });

  it('are sent to the job when they change and replaced when another job opens', async () => {
    let kept: unknown = [];
    setKernelForTests(
      fakeKernel((name, args) => {
        if (name === 'doc.guides.set') {
          kept = (args as { guides: unknown }).guides;
          return { dirty: true };
        }
        if (name === 'doc.guides.get') {
          return { guides: [] };
        }
        throw new Error(name);
      }),
    );
    const stop = watchGuides();
    useViewStore.getState().addGuide('y', 42);
    await settle();
    expect(kept).toEqual([{ axis: 'y', position: 42, locked: false }]);
    expect(useDocumentStore.getState().dirty).toBe(true);
    // A new job has no guides: the view drops the old job's.
    useDocumentStore.getState().jobReplaced();
    await settle();
    await settle();
    expect(useViewStore.getState().guides).toEqual([]);
    stop();
  });
});
