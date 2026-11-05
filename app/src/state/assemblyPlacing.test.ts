import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';

import type { ComponentRelief, DropLanding, PieceRow, Triple } from '../bridge/assembly';
import type { Heightfield } from '../bridge/cam';
import { setKernelForTests } from '../bridge/client';
import { fakeKernel } from '../testing/fakeKernel';
import { componentPieceKey, placingGhost, useAssemblyStore, type Placing } from './assemblyStore';

const componentPiece = (component: number): PieceRow => ({
  sheet: 1,
  part: 1,
  component,
  parts: 1,
  sheetName: 'Sheet 1',
  vectorName: null,
  model: false,
  position: [0, 0, 3],
  rotation: [0, 0, 90],
  size: [40, 20, 6],
  hasModel: false,
  hasToolpaths: false,
});

const field = {} as Heightfield;
const relief = { size: [10, 10, 4] } as unknown as ComponentRelief;
const onPart: DropLanding = { point: [1, 2, 3], onto: { sheet: 1, part: 2, component: null } };
const onGround: DropLanding = { point: [100, 50, 0], onto: null };
const landed = { position: [5, 6, 7] as Triple, rotation: [0, 90, 0] as Triple };

const placing = (at: DropLanding | null, pose: Placing['pose'] = null): Placing => ({
  component: 5,
  dragging: false,
  at,
  pose,
});

const flush = async () => {
  for (let i = 0; i < 12; i += 1) {
    await Promise.resolve();
  }
};

describe('the ghost of a component being placed', () => {
  it('shows nothing until the pointer is over the view and the relief is in', () => {
    expect(placingGhost(null, [], {}, {})).toBeNull();
    expect(placingGhost(placing(null), [], {}, { 5: relief })).toBeNull();
    expect(placingGhost(placing(onGround), [], {}, {})).toBeNull();
  });

  it('rests a new component unturned on the point until the kernel says where it lands', () => {
    const ghost = placingGhost(placing(onGround), [], {}, { 5: relief });
    expect(ghost?.position).toEqual([100, 50, 2]);
    expect(ghost?.rotation).toEqual([0, 0, 0]);
    expect(ghost?.field).toBe(relief);
    const answered = placingGhost(placing(onGround, landed), [], {}, { 5: relief });
    expect(answered?.position).toEqual(landed.position);
    expect(answered?.rotation).toEqual(landed.rotation);
  });

  it('shows a component that is a piece already turned as it is, with its own relief', () => {
    const piece = componentPiece(5);
    const at: DropLanding = { point: [0, 0, 10], onto: null };
    const ghost = placingGhost(placing(at), [piece], { [componentPieceKey(5)]: field }, {});
    expect(ghost?.field).toBe(field);
    expect(ghost?.rotation).toEqual([0, 0, 90]);
    ghost?.position.forEach((v, i) => expect(v).toBeCloseTo([0, 0, 13][i] ?? NaN, 9));
  });
});

describe('landing a component', () => {
  let calls: { name: string; args: unknown }[] = [];
  let refuse = false;

  beforeAll(() => {
    setKernelForTests(
      fakeKernel((name, args) => {
        calls.push({ name, args });
        if (name === 'assembly.pieces') {
          return { pieces: useAssemblyStore.getState().pieces, components: [] };
        }
        if (name === 'assembly.dropPlace') {
          if (refuse) {
            throw new Error('that piece is grounded; unground it to move it');
          }
          return { ...landed, revision: 1 };
        }
        return { revision: 1 };
      }),
    );
  });
  afterAll(() => setKernelForTests(null));
  beforeEach(() => {
    calls = [];
    refuse = false;
    useAssemblyStore.setState({
      active: true,
      pieces: [componentPiece(7)],
      reliefs: {},
      placing: null,
      error: null,
    });
  });

  it('asks the kernel where it lands, one request at a time, the newest waiting', async () => {
    const store = useAssemblyStore.getState();
    store.startPlacing(5, false);
    expect(useAssemblyStore.getState().placing).toEqual(placing(null));
    store.movePlacing(onGround);
    store.movePlacing({ point: [9, 9, 0], onto: null });
    store.movePlacing(onPart);
    expect(useAssemblyStore.getState().placing?.at).toEqual(onPart);
    await flush();
    const asked = calls.filter((c) => c.name === 'assembly.dropPlace').map((c) => c.args);
    expect(asked).toEqual([
      { component: 5, point: [100, 50, 0] },
      { component: 5, point: [1, 2, 3], onto: { sheet: 1, part: 2, component: null } },
    ]);
    expect(useAssemblyStore.getState().placing?.pose).toEqual(landed);
    // Off the view the ghost hides and the answer goes.
    store.movePlacing(null);
    expect(useAssemblyStore.getState().placing?.pose).toBeNull();
  });

  it('adds a new component where the kernel says it lands, flush and clear', async () => {
    await useAssemblyStore.getState().dropComponent(5, onPart);
    await flush();
    expect(useAssemblyStore.getState().placing).toBeNull();
    expect(calls.find((c) => c.name === 'assembly.dropPlace')?.args).toEqual({
      component: 5,
      point: [1, 2, 3],
      onto: { sheet: 1, part: 2, component: null },
    });
    expect(calls.find((c) => c.name === 'assembly.addComponentPieces')?.args).toEqual({
      components: [5],
      pose: landed,
    });
  });

  it('moves a component that is a piece already where the kernel says, and picks it', async () => {
    await useAssemblyStore.getState().dropComponent(7, onGround);
    const placed = calls.find((c) => c.name === 'assembly.placePiece')?.args as {
      position: Triple;
      rotation: Triple;
    };
    expect(placed.position).toEqual(landed.position);
    expect(placed.rotation).toEqual(landed.rotation);
    expect(useAssemblyStore.getState().selected).toBe(componentPieceKey(7));
    expect(calls.some((c) => c.name === 'assembly.addComponentPieces')).toBe(false);
  });

  it('says why when the kernel refuses the drop, and moves nothing', async () => {
    refuse = true;
    await useAssemblyStore.getState().dropComponent(7, onGround);
    expect(useAssemblyStore.getState().error).toMatch(/grounded/);
    expect(calls.some((c) => c.name === 'assembly.placePiece')).toBe(false);
  });

  it('cancels without a call', () => {
    const store = useAssemblyStore.getState();
    store.startPlacing(7, true);
    store.cancelPlacing();
    expect(useAssemblyStore.getState().placing).toBeNull();
    expect(calls).toEqual([]);
  });
});
