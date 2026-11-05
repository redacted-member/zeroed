import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Collision, Heightfield, SimCheck } from '../bridge/cam';
import { floatHeights } from '../bridge/heights';
import { previewIssues } from '../forms/PreviewToolpathsForm/issues';
import { DEFAULT_JOB, type JobSetup } from '../bridge/types';
import { wrappedScenePoint } from '../views/View3D/wrappedMesh';
import { useDocumentStore } from './documentStore';
import { useModelStore } from './modelStore';
import { shownPreview, usePreviewStore } from './previewStore';
import { useView3dStore } from './view3dStore';

const sim = vi.hoisted(() => ({
  run: vi.fn(),
  removeWaste: vi.fn(),
  start: vi.fn(),
  heightfield: vi.fn(),
  advance: vi.fn(),
  check: vi.fn(),
}));

vi.mock('../bridge/api', () => ({ api: { sim } }));

function field(): Heightfield {
  return {
    width: 2,
    height: 1,
    cellSize: 1,
    originX: 0.5,
    originY: 0.5,
    top: 0,
    bottom: -10,
    revision: 1,
    sections: [],
    z: floatHeights(new Float32Array([0, -3])),
    owner: new Uint16Array([0, 1]),
    floor: 'flat',
  };
}

const rapid: Collision = {
  toolpathId: 5,
  moveIndex: 3,
  x: 1,
  y: 0.5,
  z: -3,
  kind: 'rapid',
  severity: 'high',
};

const checked: SimCheck = {
  width: 2,
  height: 1,
  tolerance: 0.1,
  found: 1,
  map: new Uint8Array([0, 1]),
  issues: [
    {
      kind: 'gouge',
      severity: 'high',
      toolpathId: 5,
      moveIndex: 4,
      x: 1.5,
      y: 0.5,
      z: -3,
      depth: 1,
      area: 1,
    },
  ],
};

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe('preview store lithophane', () => {
  it('is off by default and keeps its strength from 0 to 100', () => {
    const store = usePreviewStore.getState();
    expect(store.lithophane).toBe(false);
    store.setLithophane(true);
    store.setLithoStrength(140);
    expect(usePreviewStore.getState().lithoStrength).toBe(100);
    usePreviewStore.getState().setLithoStrength(-3);
    expect(usePreviewStore.getState().lithoStrength).toBe(0);
    expect(usePreviewStore.getState().lithophane).toBe(true);
  });
});

describe('preview store checks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sim.run.mockResolvedValue({
      width: 2,
      height: 1,
      cellSize: 1,
      collisions: [rapid],
      seconds: 9,
    });
    sim.heightfield.mockImplementation(() => Promise.resolve(field()));
    sim.check.mockResolvedValue(checked);
    sim.start.mockResolvedValue({
      width: 2,
      height: 1,
      cellSize: 1,
      revision: 1,
      toolpaths: [{ id: 5, name: 'Pocket', tool: [[3, 0]], shank: null, holder: [12, 25, 60] }],
    });
    sim.advance.mockResolvedValue({
      step: {
        done: false,
        tool: { x: 1, y: 0.5, z: -3, toolpathId: 5 },
        collisions: [rapid],
        progress: 0.4,
        seconds: 0,
      },
      patch: { rects: [], z: floatHeights(new Float32Array(0)), owner: new Uint16Array(0) },
    });
    usePreviewStore.setState({ animate: false, check: null, collisions: [], tolerance: 0.1 });
  });

  it('checks a finished preview against the design at the tolerance', async () => {
    await usePreviewStore.getState().run([5]);
    await flush();
    const state = usePreviewStore.getState();
    expect(sim.check).toHaveBeenCalledWith(0.1);
    expect(state.check).toBe(checked);
    expect(previewIssues(state.collisions, state.check).map((i) => i.kind)).toEqual([
      'rapid',
      'gouge',
    ]);
    // A new tolerance checks again; the same one does not.
    state.setTolerance(0.25);
    state.setTolerance(0.25);
    await flush();
    expect(sim.check).toHaveBeenLastCalledWith(0.25);
    expect(sim.check).toHaveBeenCalledTimes(2);
  });

  it('reads the turned preview when the other side is shown, not marking it out of date', async () => {
    await usePreviewStore.getState().run([5]);
    await flush();
    const reads = sim.heightfield.mock.calls.length;
    const before = useDocumentStore.getState();
    useDocumentStore.setState({ activeSide: 'bottom', revision: before.revision + 1 });
    await flush();
    expect(sim.heightfield.mock.calls.length).toBe(reads + 1);
    expect(usePreviewStore.getState().outOfDate).toBe(false);
    // An edit still marks it.
    useDocumentStore.setState({ revision: before.revision + 2 });
    expect(usePreviewStore.getState().outOfDate).toBe(true);
    useDocumentStore.setState({ activeSide: before.activeSide });
    await flush();
  });

  it('steps aside for the model once a component changes, until the preview runs again', async () => {
    await usePreviewStore.getState().run([5]);
    await flush();
    const stamp = useModelStore.getState().stamp;
    expect(shownPreview(usePreviewStore.getState())).not.toBeNull();
    // A model change (Create Shape, Apply) shows the model in the 3D view;
    // the preview itself is kept for the form.
    useModelStore.setState({ stamp: (stamp ?? 0) + 1 });
    expect(usePreviewStore.getState().modelChanged).toBe(true);
    expect(shownPreview(usePreviewStore.getState())).toBeNull();
    expect(usePreviewStore.getState().heightfield).not.toBeNull();
    // Running the preview again shows it.
    await usePreviewStore.getState().run([5]);
    await flush();
    expect(shownPreview(usePreviewStore.getState())).not.toBeNull();
    useModelStore.setState({ stamp });
  });

  it('frames an issue and runs the animation to it, paused there', async () => {
    await usePreviewStore.getState().run([5]);
    await flush();
    const state = usePreviewStore.getState();
    const [first] = previewIssues(state.collisions, state.check);
    expect(first).toBeDefined();
    if (first === undefined) {
      return;
    }
    await state.goToIssue(first);
    const after = usePreviewStore.getState();
    expect(useView3dStore.getState().request?.kind).toBe('frame');
    expect(sim.start).toHaveBeenCalledWith('standard', [5], undefined, false);
    expect(sim.advance).toHaveBeenCalledWith({
      toMove: { toolpathId: 5, moveIndex: 3, x: 1, y: 0.5, z: -3 },
    });
    expect(after.selectedIssue).toBe(first.key);
    expect(after.animating).toBe(true);
    expect(after.playing).toBe(false);
    // The tool shows there with the holder the checks use.
    expect(after.tool?.holder).toEqual([12, 25, 60]);
    // The rapid met again on the way is not listed twice.
    expect(after.collisions).toHaveLength(1);
  });

  it('frames a rotary issue where it lies on the wrapped blank', async () => {
    const flat = useDocumentStore.getState().job;
    const rotary: JobSetup = { ...DEFAULT_JOB, kind: 'rotary' };
    useDocumentStore.setState({ job: rotary });
    try {
      await usePreviewStore.getState().run([5]);
      await flush();
      const state = usePreviewStore.getState();
      const [first] = previewIssues(state.collisions, state.check);
      if (first === undefined || state.heightfield === null) {
        throw new Error('an issue on a preview');
      }
      await state.goToIssue(first);
      const request = useView3dStore.getState().request;
      if (request?.kind !== 'frame') {
        throw new Error('a frame request');
      }
      const [x, y, z] = wrappedScenePoint(rotary, state.heightfield.top, first);
      const centre = [0, 1, 2].map(
        (k) => ((request.box.min[k] ?? 0) + (request.box.max[k] ?? 0)) / 2,
      );
      // Scene (x, y, z) is job space (x, -z, y) to the frame.
      expect(centre[0]).toBeCloseTo(x);
      expect(centre[1]).toBeCloseTo(-z);
      expect(centre[2]).toBeCloseTo(y);
    } finally {
      useDocumentStore.setState({ job: flat });
    }
  });

  it('makes a Preview Selected run on an earlier preview again to reach an issue', async () => {
    const state = usePreviewStore.getState();
    await state.run([5]);
    await state.run([6], false, true);
    await flush();
    const [area] = previewIssues([], checked);
    if (area === undefined) {
      throw new Error('an issue');
    }
    const issue = (toolpathId: number) => ({ ...area, toolpathId });
    // An issue of the first run: that run animated on fresh material, the
    // kept run made once the animation finishes.
    sim.run.mockClear();
    sim.advance.mockResolvedValueOnce({
      step: { done: true, tool: null, collisions: [], progress: 1, seconds: 0 },
      patch: { rects: [], z: floatHeights(new Float32Array(0)), owner: new Uint16Array(0) },
    });
    await usePreviewStore.getState().goToIssue(issue(5));
    expect(sim.start).toHaveBeenLastCalledWith('standard', [5], undefined, false);
    await flush();
    expect(sim.run).toHaveBeenCalledWith('standard', [6], true);
    expect(usePreviewStore.getState().canUndo).toBe(true);
    // An issue of the kept run: the first run made at once, the kept run
    // animated on it.
    sim.run.mockClear();
    await usePreviewStore.getState().goToIssue(issue(6));
    expect(sim.run).toHaveBeenCalledWith('standard', [5], false);
    expect(sim.start).toHaveBeenLastCalledWith('standard', [6], false, true);
    expect(usePreviewStore.getState().animating).toBe(true);
  });
});
