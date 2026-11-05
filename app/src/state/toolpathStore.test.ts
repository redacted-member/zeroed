import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';

import type {
  DepthCheckReply,
  DrillParams,
  Tool,
  ToolpathRequest,
  ToolpathSummary,
} from '../bridge/cam';
import { setKernelForTests } from '../bridge/client';
import type { BlobHeader, BlobReply, KernelWarning } from '../bridge/types';
import { fakeKernel } from '../testing/fakeKernel';
import { useDocumentStore } from './documentStore';
import { usePrefsStore } from './prefsStore';
import { useToolpathStore } from './toolpathStore';
import { useUiStore } from './uiStore';

describe('toolpath store recalculation', () => {
  let list: Partial<ToolpathSummary>[] = [];
  let depth: DepthCheckReply = { all: false, toolpaths: [] };
  /** Replies to cam.recalculate by id: warnings, or an error to throw. */
  let recalculated: Record<number, KernelWarning[] | Error> = {};
  /** Undo group and recalculation calls, in order. */
  let calls: string[] = [];

  beforeAll(() => {
    const kernel = fakeKernel((name, args) => {
      switch (name) {
        case 'cam.list':
          return list;
        case 'cam.groups':
          return [];
        case 'cam.tiling.get':
          return null;
        case 'cam.depthCheck':
          return depth;
        case 'undo.beginGroup':
          calls.push(`begin ${(args as { label?: string }).label ?? ''}`);
          return {};
        case 'undo.endGroup':
          calls.push('end');
          return { merged: true };
        case 'cam.recalculate': {
          const id = (args as { id: number }).id;
          calls.push(`recalculate ${id}`);
          const reply = recalculated[id] ?? [];
          if (reply instanceof Error) {
            throw reply;
          }
          return { id, revision: 1, warnings: reply };
        }
        case 'cam.compute':
          return { id: 9, revision: 1, warnings: [] };
        case 'prefs.set': {
          const prefs = usePrefsStore.getState().prefs;
          const patch = args as { view3d?: Partial<typeof prefs.view3d> };
          return {
            ...prefs,
            view3d: { ...prefs.view3d, ...patch.view3d },
            path: null,
            setAside: null,
          };
        }
        case 'doc.info':
          return { revision: 1, path: null, dirty: false };
        case 'job.get':
          return useDocumentStore.getState().job;
        case 'layers.list':
        case 'sheets.list':
        case 'entities.list':
          return [];
        case 'selection.get':
          return { ids: [], bbox: null };
        case 'undo.info':
          return { undo: null, redo: null };
        default:
          throw new Error(`unexpected ${name}`);
      }
    });
    setKernelForTests({
      ...kernel,
      callBlob: <H extends BlobHeader>() =>
        Promise.resolve({
          header: { revision: 1, toolpaths: [] },
          sections: {},
        } as unknown as BlobReply<H>),
    });
    const prefs = usePrefsStore.getState().prefs;
    usePrefsStore.setState({
      prefs: { ...prefs, toolpaths: { ...prefs.toolpaths, askBeforeRecalculating: false } },
    });
  });
  afterAll(() => setKernelForTests(null));
  beforeEach(() => {
    useDocumentStore.setState({ lastError: null });
    useToolpathStore.setState({ warnings: [] });
    depth = { all: false, toolpaths: [] };
    recalculated = {};
    calls = [];
  });

  it('recalculates several toolpaths as one undo step, one as its own', async () => {
    list = [
      { id: 1, name: 'Laser Cut 1', stale: true },
      { id: 2, name: 'Laser Picture 1', stale: true },
    ];
    await useToolpathStore.getState().recalculateMany([1, 2]);
    expect(calls).toEqual(['begin Recalculate Toolpaths', 'recalculate 1', 'recalculate 2', 'end']);
    calls = [];
    // A failure still closes the group.
    recalculated = { 2: new Error('no vectors') };
    await useToolpathStore.getState().recalculateMany([1, 2]);
    expect(calls.at(-1)).toBe('end');
    calls = [];
    await useToolpathStore.getState().recalculate(1);
    expect(calls).toEqual(['recalculate 1']);
  });

  it('keeps what the recalculation warned about beside the depth check', async () => {
    list = [
      { id: 1, name: 'Pocket 1', stale: true },
      { id: 2, name: 'Profile 1', stale: false },
    ];
    recalculated = {
      1: [{ code: 'fix.repaired', params: { count: 1 }, message: 'English fallback.' }],
    };
    depth = { all: false, toolpaths: [{ id: 2, name: 'Profile 1', depth: 12, thickness: 10 }] };
    await useToolpathStore.getState().followJobChange();
    const warnings = useToolpathStore.getState().warnings;
    expect(warnings.map((w) => w.toolpath)).toEqual(['Pocket 1', null]);
    expect(warnings[0]?.messages[0]).toContain('repaired');
  });

  it('keeps the error of a toolpath that failed when a later one succeeds', async () => {
    list = [
      { id: 1, name: 'Pocket 1', stale: true },
      { id: 2, name: 'Profile 1', stale: true },
    ];
    recalculated = { 1: new Error('the tool is too big for this pocket'), 2: [] };
    await useToolpathStore.getState().recalculateMany([1, 2]);
    expect(useDocumentStore.getState().lastError).toContain('the tool is too big for this pocket');
    expect(useDocumentStore.getState().lastError).toContain('Pocket 1');
  });

  it('skips the depth check after a change that cannot change depths', async () => {
    list = [{ id: 2, name: 'Profile 1', stale: false }];
    depth = { all: false, toolpaths: [{ id: 2, name: 'Profile 1', depth: 12, thickness: 10 }] };
    const job = useDocumentStore.getState().job;
    await useToolpathStore.getState().followJobChange({ ...job, xyOrigin: 'centre' });
    expect(useToolpathStore.getState().warnings).toEqual([]);
    await useToolpathStore.getState().followJobChange({ ...job, thickness: job.thickness + 5 });
    expect(useToolpathStore.getState().warnings).toHaveLength(1);
  });

  it('draws the toolpaths in the 3D view it opens after a calculation', async () => {
    const prefs = usePrefsStore.getState().prefs;
    usePrefsStore.setState({
      prefs: {
        ...prefs,
        toolpaths: { ...prefs.toolpaths, autoOpen3d: true },
        view3d: { ...prefs.view3d, showToolpaths: false },
      },
    });
    useUiStore.setState({ viewMode: '2d' });
    const request = {
      strategy: 'drill',
      tool: {} as Tool,
      params: {} as DrillParams,
      ids: [1],
    } as ToolpathRequest;
    expect(await useToolpathStore.getState().calculate(request)).toBe(true);
    expect(useUiStore.getState().viewMode).toBe('3d');
    // The preference is saved without holding the calculation up.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(usePrefsStore.getState().prefs.view3d.showToolpaths).toBe(true);
  });
});
