// Generated action sequences through the stores (documentStore,
// toolpathStore, modelStore, assemblyStore): a seeded driver draws,
// selects, moves, deletes, adds and deletes layers, calculates and deletes
// toolpaths, makes and deletes components, opens the Assembly View, and
// undoes and redoes, all through the stores as the panels do. After each
// action, once every reply has come back, each store's mirror must say
// what the kernel says: the same objects, layers, selection, revision and
// undo labels, the same toolpaths with the same stale marks, the same
// components, and nothing selected that is gone.
//
// Without a dev host the stores talk to an in-memory kernel (below) that
// keeps objects, layers, selection, toolpaths, components and undo the way
// the real one does. With CARVE_DEVHOST_URL=http://127.0.0.1:<port> set
// they talk to the real kernel of a running carve-devhost instead.

import { afterAll, beforeAll, describe, expect, it } from 'vitest';

import { setKernelForTests } from '../bridge/client';
import { blobResult, jsonResult, splitFrame } from '../bridge/frame';
import type { BlobHeader, BlobReply, KernelClient } from '../bridge/types';
import { fakeKernel } from '../testing/fakeKernel';
import { useAssemblyStore } from './assemblyStore';
import { useDocumentStore } from './documentStore';
import { useModelStore } from './modelStore';
import { useToolpathStore } from './toolpathStore';

/** xorshift32, so a seed replays. */
function rng(seed: number): { below: (n: number) => number; chance: (p: number) => boolean } {
  let x = seed >>> 0 || 1;
  const next = (): number => {
    x ^= x << 13;
    x >>>= 0;
    x ^= x >>> 17;
    x ^= x << 5;
    x >>>= 0;
    return x;
  };
  return {
    below: (n) => (n <= 0 ? 0 : next() % n),
    chance: (p) => next() % 100 < p,
  };
}

// ------------------------------------------------------ in-memory kernel

interface Thing {
  id: number;
  layer: number;
  bbox: [number, number, number, number];
  revision: number;
}

interface Snapshot {
  things: Thing[];
  layers: { id: number; name: string }[];
  activeLayer: number;
  selection: number[];
  toolpaths: { id: number; name: string; sources: { id: number; revision: number }[] }[];
  components: { id: number; name: string }[];
}

/** The part of the kernel the stores use, kept as the real one keeps it:
 * one undo step per change, counters that never go back, the selection
 * outside the steps but never naming what is gone. */
function memoryKernel(): KernelClient {
  const empty = (): Snapshot => ({
    things: [],
    layers: [{ id: 1, name: 'Layer 1' }],
    activeLayer: 1,
    selection: [],
    toolpaths: [],
    components: [],
  });
  let state = empty();
  let next = 2;
  let revision = 0;
  let dirty = false;
  let stamp = 1;
  const undo: { label: string; state: Snapshot }[] = [];
  const redo: { label: string; state: Snapshot }[] = [];
  const copy = (s: Snapshot): Snapshot => structuredClone(s);
  const prune = () => {
    state.selection = state.selection.filter((id) => state.things.some((t) => t.id === id));
  };
  const step = (label: string, change: () => void) => {
    const before = copy(state);
    change();
    undo.push({ label, state: before });
    redo.length = 0;
    revision += 1;
    dirty = true;
    stamp += 1;
    return { revision };
  };
  const refuse = (text: string): never => {
    throw new Error(text);
  };
  const thing = (id: number) => state.things.find((t) => t.id === id) ?? refuse(`no entity ${id}`);
  const answer = (name: string, raw: unknown): unknown => {
    const args = (raw ?? {}) as Record<string, unknown>;
    const ids = (args['ids'] as number[] | undefined) ?? [];
    switch (name) {
      case 'doc.new':
        state = empty();
        next = 2;
        undo.length = 0;
        redo.length = 0;
        revision += 1;
        dirty = false;
        stamp += 1;
        return { revision };
      case 'doc.info':
        return { revision, path: null, dirty, activeSide: 'top' };
      case 'job.get':
        return useDocumentStore.getState().job;
      case 'layers.list':
        return state.layers.map((l) => ({
          ...l,
          colour: '#000000',
          visible: true,
          locked: false,
          active: l.id === state.activeLayer,
          count: state.things.filter((t) => t.layer === l.id).length,
        }));
      case 'layers.add': {
        const id = next++;
        return {
          ...step('Add Layer', () => {
            state.layers.push({ id, name: `Layer ${id}` });
            if (args['active'] === true) {
              state.activeLayer = id;
            }
          }),
          id,
        };
      }
      case 'layers.delete': {
        const id = args['id'] as number;
        if (state.layers.length === 1) {
          refuse('the last layer cannot be deleted');
        }
        const moveTo = args['moveTo'] as number | undefined;
        return step('Delete Layer', () => {
          state.layers = state.layers.filter((l) => l.id !== id);
          if (moveTo === undefined) {
            state.things = state.things.filter((t) => t.layer !== id);
            prune();
          } else {
            state.things = state.things.map((t) =>
              t.layer === id ? { ...t, layer: moveTo, revision: t.revision + 1 } : t,
            );
          }
          if (state.activeLayer === id) {
            state.activeLayer = state.layers[0]?.id ?? 1;
          }
        });
      }
      case 'layers.setActive':
        return step('Active Layer', () => {
          state.activeLayer = args['id'] as number;
        });
      case 'sheets.list':
        return [{ id: 1, name: 'Sheet 1', active: true }];
      case 'entities.list':
        return state.things.map((t) => ({
          id: t.id,
          kind: 'path',
          layer: t.layer,
          bbox: t.bbox,
          closed: true,
          revision: t.revision,
        }));
      case 'entities.create': {
        const id = next++;
        const entity = args['entity'] as {
          layer: number;
          path: { subpaths: { start: { x: number; y: number } }[] };
        };
        const start = entity.path.subpaths[0]?.start ?? { x: 0, y: 0 };
        return {
          ...step('Create', () => {
            state.things.push({
              id,
              layer: entity.layer,
              bbox: [start.x, start.y, start.x + 10, start.y + 10],
              revision: 1,
            });
          }),
          id,
        };
      }
      case 'entities.delete':
        if (ids.length === 0) {
          refuse('select at least one object to delete');
        }
        ids.forEach(thing);
        return step('Delete', () => {
          state.things = state.things.filter((t) => !ids.includes(t.id));
          prune();
        });
      case 'entities.transform': {
        const m = args['matrix'] as number[];
        ids.forEach(thing);
        return step('Move', () => {
          state.things = state.things.map((t) =>
            ids.includes(t.id)
              ? {
                  ...t,
                  bbox: [
                    t.bbox[0] + (m[4] ?? 0),
                    t.bbox[1] + (m[5] ?? 0),
                    t.bbox[2] + (m[4] ?? 0),
                    t.bbox[3] + (m[5] ?? 0),
                  ],
                  revision: t.revision + 1,
                }
              : t,
          );
        });
      }
      case 'entities.setLayer':
        ids.forEach(thing);
        return step('Move to Layer', () => {
          state.things = state.things.map((t) =>
            ids.includes(t.id)
              ? { ...t, layer: args['layer'] as number, revision: t.revision + 1 }
              : t,
          );
        });
      case 'selection.set':
        state.selection = ids.filter((id) => state.things.some((t) => t.id === id));
        return { ids: state.selection, bbox: null };
      case 'selection.add':
        state.selection = [
          ...state.selection,
          ...ids.filter(
            (id) => !state.selection.includes(id) && state.things.some((t) => t.id === id),
          ),
        ];
        return { ids: state.selection, bbox: null };
      case 'selection.clear':
        state.selection = [];
        return { ids: [], bbox: null };
      case 'selection.get':
        return { ids: state.selection, bbox: null };
      case 'undo':
      case 'redo': {
        const from = name === 'undo' ? undo : redo;
        const to = name === 'undo' ? redo : undo;
        const taken = from.pop();
        if (taken === undefined) {
          return { revision, label: null };
        }
        const selection = state.selection;
        to.push({ label: taken.label, state: copy(state) });
        state = copy(taken.state);
        state.selection = selection;
        prune();
        revision += 1;
        dirty = true;
        stamp += 1;
        return { revision, label: taken.label };
      }
      case 'undo.info':
        return {
          canUndo: undo.length > 0,
          canRedo: redo.length > 0,
          undoLabel: undo.at(-1)?.label ?? null,
          redoLabel: redo.at(-1)?.label ?? null,
        };
      case 'undo.beginGroup':
        return {};
      case 'undo.endGroup':
        return { merged: false };
      case 'cam.list':
        return state.toolpaths.map((t) => ({
          id: t.id,
          name: t.name,
          kind: 'profile',
          visible: true,
          stale: t.sources.some(
            (s) => state.things.find((x) => x.id === s.id)?.revision !== s.revision,
          ),
          toolName: 'End Mill',
          toolNumber: 1,
          estimatedSeconds: 1,
          moves: 1,
        }));
      case 'cam.compute': {
        if (ids.length === 0) {
          refuse('select the vectors to machine before calculating');
        }
        const sources = ids.map((id) => ({ id, revision: thing(id).revision }));
        const id = next++;
        return {
          ...step('Calculate Toolpath', () => {
            state.toolpaths.push({ id, name: `Profile ${id}`, sources });
          }),
          id,
          warnings: [],
        };
      }
      case 'cam.recalculate': {
        const id = args['id'] as number;
        const toolpath = state.toolpaths.find((t) => t.id === id) ?? refuse(`no toolpath ${id}`);
        const sources = toolpath.sources.map((s) => ({ id: s.id, revision: thing(s.id).revision }));
        return {
          ...step('Recalculate', () => {
            state.toolpaths = state.toolpaths.map((t) => (t.id === id ? { ...t, sources } : t));
          }),
          id,
          warnings: [],
        };
      }
      case 'cam.delete':
        return step('Delete Toolpath', () => {
          state.toolpaths = state.toolpaths.filter((t) => !ids.includes(t.id));
        });
      case 'cam.depthCheck':
        return { all: false, toolpaths: [] };
      case 'cam.groups':
        return [];
      case 'cam.tiling.get':
        return null;
      case 'model.list':
        return {
          stamp,
          components: state.components.map((c) => ({
            ...c,
            visible: true,
            combine: 'add',
            level: 1,
          })),
          levels: [{ id: 1, name: 'Level 1', visible: true, combine: 'add', groups: [], clip: [] }],
          activeLevel: 1,
        };
      case 'model.createShape': {
        if (ids.length === 0) {
          refuse('select the closed vectors to build the shape from');
        }
        ids.forEach(thing);
        const id = next++;
        return {
          ...step('Create Shape', () => {
            state.components.push({ id, name: `Shape ${id}` });
          }),
          id,
        };
      }
      case 'model.delete':
        return step('Delete Components', () => {
          state.components = state.components.filter((c) => !ids.includes(c.id));
        });
      default:
        throw new Error(`the in-memory kernel has no ${name}`);
    }
  };
  const blob = (name: string): BlobReply<BlobHeader> => {
    const header = { sections: [], width: 1, height: 1, any: false, min: 0, max: 0, revision };
    if (name === 'cam.moves') {
      return {
        header: { ...header, toolpaths: [] } as BlobHeader,
        sections: { xyz: new Float32Array(0), kind: new Uint8Array(0), width: new Float32Array(0) },
      };
    }
    if (name === 'model.composite') {
      return { header, sections: { z: Float32Array.from([0]) } };
    }
    if (name === 'model.colors') {
      return { header, sections: { palette: new Uint8Array(4), index: new Uint8Array(1) } };
    }
    throw new Error(`the in-memory kernel has no ${name}`);
  };
  return {
    ...fakeKernel(answer),
    callBlob: async <H extends BlobHeader>(name: string) =>
      (await Promise.resolve(blob(name))) as unknown as BlobReply<H>,
  };
}

// ----------------------------------------------- the real kernel, if any

/** A client of a running dev host, counting the calls under way. */
function devhostKernel(base: string): KernelClient {
  const post = async (name: string, args: unknown) => {
    // The dev host reads and writes the user's own preferences file.
    if (name.startsWith('prefs.set') || name.startsWith('prefs.reset')) {
      throw new Error(`${name} is not sent to the dev host from tests`);
    }
    const response = await fetch(`${base}/kernel/call?name=${encodeURIComponent(name)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: args === undefined ? '' : JSON.stringify(args),
    });
    return splitFrame(name, await response.arrayBuffer());
  };
  return {
    ...fakeKernel(() => undefined),
    kind: 'http',
    call: async <T>(name: string, args?: unknown) => jsonResult<T>(name, await post(name, args)),
    callBlob: async <H extends BlobHeader>(name: string, args?: unknown) =>
      blobResult<H>(name, await post(name, args)),
  };
}

/** Counts the calls under way, so a check waits for every reply. */
function counted(client: KernelClient): { client: KernelClient; idle: () => boolean } {
  let open = 0;
  const track = async <T>(work: () => Promise<T>): Promise<T> => {
    open += 1;
    try {
      return await work();
    } finally {
      open -= 1;
    }
  };
  return {
    client: {
      ...client,
      call: <T>(name: string, args?: unknown) => track(() => client.call<T>(name, args)),
      callBlob: <H extends BlobHeader>(name: string, args?: unknown) =>
        track(() => client.callBlob<H>(name, args)),
    },
    idle: () => open === 0,
  };
}

// ------------------------------------------------------------- the driver

const devhost = process.env['CARVE_DEVHOST_URL'];

const TOOL = {
  id: 0,
  name: 'End Mill 6 mm',
  number: 1,
  toolType: { kind: 'endMill', diameter: 6 },
  units: 'mm',
  passDepth: 3,
  stepover: 2.4,
  stepoverPercent: 40,
  spindleSpeed: 18000,
  feedRate: 2400,
  plungeRate: 600,
  feedUnits: 'mmPerMin',
  flutes: 2,
  clearance: 0,
  notes: '',
  cuttingData: [],
};

function square(x: number, y: number, size: number) {
  const p = (px: number, py: number) => ({ x: px, y: py });
  return {
    subpaths: [
      {
        start: p(x, y),
        closed: true,
        segments: [
          { kind: 'line' as const, to: p(x + size, y) },
          { kind: 'line' as const, to: p(x + size, y + size) },
          { kind: 'line' as const, to: p(x, y + size) },
        ],
      },
    ],
  };
}

describe('generated action sequences through the stores', () => {
  let idle: () => boolean = () => true;
  let client: KernelClient;

  beforeAll(() => {
    const made = counted(devhost === undefined ? memoryKernel() : devhostKernel(devhost));
    client = made.client;
    idle = made.idle;
    setKernelForTests(client);
  });
  afterAll(() => setKernelForTests(null));

  /** Waits until no reply is outstanding, twice over, so the refreshes a
   * reply starts have come back too. */
  const settle = async () => {
    for (let quiet = 0; quiet < 3;) {
      await new Promise((resolve) => setTimeout(resolve, devhost === undefined ? 0 : 5));
      quiet = idle() ? quiet + 1 : 0;
    }
  };

  /** Every store against the kernel. */
  const check = async (at: string) => {
    await settle();
    const [info, layers, entities, selection, undo, toolpaths, model] = await Promise.all([
      client.call<{ revision: number; dirty: boolean }>('doc.info'),
      client.call<{ id: number; active: boolean }[]>('layers.list'),
      client.call<{ id: number; layer: number; revision: number }[]>('entities.list'),
      client.call<{ ids: number[] }>('selection.get'),
      client.call<{ undoLabel: string | null; redoLabel: string | null }>('undo.info'),
      client.call<{ id: number; stale: boolean }[]>('cam.list'),
      client.call<{ components: { id: number }[] }>('model.list'),
    ]);
    const doc = useDocumentStore.getState();
    expect(doc.revision, at).toBe(info.revision);
    expect(doc.dirty, at).toBe(info.dirty);
    expect(
      doc.layers.map((l) => [l.id, l.active]),
      at,
    ).toEqual(layers.map((l) => [l.id, l.active]));
    expect(
      doc.entities.map((e) => [e.id, e.layer, e.revision]),
      at,
    ).toEqual(entities.map((e) => [e.id, e.layer, e.revision]));
    expect(doc.selection.ids, at).toEqual(selection.ids);
    const shown = new Set(doc.entities.map((e) => e.id));
    expect(
      doc.selection.ids.filter((id) => !shown.has(id)),
      `${at}: the selection names only objects on show`,
    ).toEqual([]);
    expect([doc.undo.undoLabel, doc.undo.redoLabel], at).toEqual([undo.undoLabel, undo.redoLabel]);
    const paths = useToolpathStore.getState();
    expect(
      paths.toolpaths.map((t) => [t.id, t.stale]),
      at,
    ).toEqual(toolpaths.map((t) => [t.id, t.stale]));
    if (paths.selectedId !== null) {
      expect(
        toolpaths.map((t) => t.id),
        at,
      ).toContain(paths.selectedId);
    }
    const relief = useModelStore.getState();
    expect(
      relief.components.map((c) => c.id),
      at,
    ).toEqual(model.components.map((c) => c.id));
    if (relief.selectedId !== null) {
      expect(
        model.components.map((c) => c.id),
        at,
      ).toContain(relief.selectedId);
    }
    const assembly = useAssemblyStore.getState();
    if (assembly.active && devhost !== undefined) {
      const pieces = await client.call<{ pieces: unknown[] }>('assembly.pieces');
      expect(assembly.pieces.length, at).toBe(pieces.pieces.length);
    }
  };

  const run = async (seed: number, steps: number) => {
    const r = rng(seed);
    const doc = () => useDocumentStore.getState();
    await client.call('doc.new');
    doc().jobReplaced();
    await doc().refresh();
    const log: string[] = [];
    for (let i = 0; i < steps; i += 1) {
      const ids = doc().entities.map((e) => e.id);
      const layers = doc().layers.map((l) => l.id);
      const toolpaths = useToolpathStore.getState().toolpaths.map((t) => t.id);
      const components = useModelStore.getState().components.map((c) => c.id);
      const pick = <T>(items: T[]): T | undefined => items[r.below(items.length)];
      const some = (items: number[]) => items.filter(() => r.chance(40));
      const roll = r.below(100);
      let action: string;
      if (roll < 20) {
        action = 'draw';
        await doc().createPath(square(r.below(250), r.below(150), 5 + r.below(40)));
      } else if (roll < 30) {
        action = 'select';
        await doc().select(some(ids), r.chance(70) ? 'set' : 'add');
      } else if (roll < 36) {
        action = 'delete';
        await doc().deleteSelection();
      } else if (roll < 42) {
        action = 'move';
        await doc().moveSelection(r.below(20) - 10, r.below(20) - 10);
      } else if (roll < 46) {
        action = 'add layer';
        await doc().addLayer(undefined, undefined, { active: r.chance(50) });
      } else if (roll < 49) {
        const id = pick(layers);
        action = `delete layer ${id}`;
        if (id !== undefined) {
          await doc().deleteLayer(
            id,
            r.chance(50) ? pick(layers.filter((l) => l !== id)) : undefined,
          );
        }
      } else if (roll < 52) {
        const id = pick(layers);
        action = `to layer ${id}`;
        if (id !== undefined) {
          await doc().moveSelectionToLayer(id);
        }
      } else if (roll < 66) {
        action = 'undo';
        await doc().undoLast();
      } else if (roll < 74) {
        action = 'redo';
        await doc().redoLast();
      } else if (roll < 80) {
        action = 'calculate';
        const selected = doc().selection.ids;
        await useToolpathStore.getState().calculate({
          strategy: 'profile',
          tool: TOOL as never,
          params: { cutDepth: 3, side: 'outside' } as never,
          ids: selected,
        });
      } else if (roll < 83) {
        const id = pick(toolpaths);
        action = `recalculate ${id}`;
        if (id !== undefined) {
          await useToolpathStore.getState().recalculate(id);
        }
      } else if (roll < 86) {
        const id = pick(toolpaths);
        action = `delete toolpath ${id}`;
        if (id !== undefined) {
          await useToolpathStore.getState().remove(id);
        }
      } else if (roll < 91) {
        action = 'create shape';
        // As Create Shape's form does: the kernel, then the mirrors follow
        // the revision.
        try {
          await client.call('model.createShape', {
            ids: doc().selection.ids,
            params: { profile: 'flat', height: 3 },
          });
        } catch {
          // Refused: nothing selected, or not closed.
        }
        await doc().refresh();
      } else if (roll < 94) {
        const id = pick(components);
        action = `delete component ${id}`;
        if (id !== undefined) {
          await useModelStore.getState().remove(id);
        }
      } else if (roll < 97 && devhost !== undefined) {
        action = 'assembly view';
        if (useAssemblyStore.getState().active) {
          useAssemblyStore.getState().close();
        } else {
          await useAssemblyStore.getState().open();
        }
      } else {
        action = 'clear selection';
        await doc().clearSelection();
      }
      log.push(action);
      await check(`seed ${seed} step ${i} (${log.slice(-6).join(', ')})`);
    }
  };

  const seeds = devhost === undefined ? [1, 2, 3, 4] : [1, 2, 3, 4, 5, 6, 7, 8];
  for (const seed of seeds) {
    it(`keeps every mirror equal to the kernel, seed ${seed}`, async () => {
      await run(seed, devhost === undefined ? 80 : 150);
    }, 900_000);
  }
});
