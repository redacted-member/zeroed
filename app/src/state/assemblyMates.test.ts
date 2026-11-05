import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';

import type { MateRow, PieceRow } from '../bridge/assembly';
import { setKernelForTests } from '../bridge/client';
import { fakeKernel } from '../testing/fakeKernel';
import { isShown, NO_MATE, useAssemblyStore, withFace } from './assemblyStore';

const piece = (sheet: number, grounded = false): PieceRow => ({
  sheet,
  part: 1,
  parts: 1,
  sheetName: `Sheet ${sheet}`,
  vectorName: null,
  model: false,
  position: [0, 0, 0],
  rotation: [0, 0, 0],
  size: [100, 50, 10],
  hasModel: false,
  hasToolpaths: true,
  grounded,
});

const mateRow = (id: number): MateRow => ({
  id,
  kind: 'flush',
  a: { sheet: 1, part: 1 },
  b: { sheet: 2, part: 1 },
  value: 0,
  aligned: false,
  suppressed: false,
  state: 'solved',
  distanceError: 0,
  angleError: 0,
});

describe('mates and piece state in the store', () => {
  let calls: { name: string; args: unknown }[] = [];

  beforeAll(() => {
    setKernelForTests(
      fakeKernel((name, args) => {
        calls.push({ name, args });
        if (name === 'assembly.matePieces') {
          return {
            revision: 3,
            mate: 17,
            state: 'solved',
            moved: { sheet: 2, part: 1 },
            position: [1, 2, 3],
            rotation: [0, 0, 0],
            turn: 0,
          };
        }
        if (name.startsWith('assembly.')) {
          return { revision: 4 };
        }
        throw new Error(`unexpected ${name}`);
      }),
    );
  });
  afterAll(() => setKernelForTests(null));
  beforeEach(() => {
    calls = [];
    useAssemblyStore.setState({
      active: false,
      pieces: [piece(1, true), piece(2)],
      mates: [mateRow(17)],
      selectedMate: null,
      mate: NO_MATE,
    });
  });

  const sent = (name: string) => calls.filter((c) => c.name === name).map((c) => c.args);

  it('makes a mate of the picked kind, then changes that mate when applied again', async () => {
    const store = useAssemblyStore.getState();
    store.setMateOptions({ kind: 'flush', offset: 2 });
    store.startMate();
    await useAssemblyStore.getState().pickFace({ sheet: 1, part: 1, point: [1, 1, 10] });
    await useAssemblyStore.getState().pickFace({ sheet: 2, part: 1, point: [2, 2, 10] });
    const first = sent('assembly.matePieces')[0];
    expect(first).toMatchObject({ kind: 'flush', offset: 2, replace: null });
    const state = useAssemblyStore.getState();
    expect(state.mate.last).toBe(17);
    expect(state.selectedMate).toBe(17);
    expect(state.pieces[1]?.position).toEqual([1, 2, 3]);
    await useAssemblyStore.getState().applyMate();
    expect(sent('assembly.matePieces')[1]).toMatchObject({ kind: 'flush', replace: 17 });
    // New picks make a new mate.
    const again = withFace(
      { ...useAssemblyStore.getState().mate, step: 'moving' },
      {
        sheet: 2,
        part: 1,
        point: [0, 0, 0],
      },
    );
    expect(again.mate.last).toBeNull();
  });

  it('edits, suppresses and deletes a mate by its id', async () => {
    useAssemblyStore.getState().selectMate(17);
    await useAssemblyStore.getState().editMate(17, { value: 5, kind: 'distance' });
    await useAssemblyStore.getState().editMate(17, { suppressed: true });
    expect(sent('assembly.editMate')).toEqual([
      { id: 17, value: 5, kind: 'distance' },
      { id: 17, suppressed: true },
    ]);
    await useAssemblyStore.getState().deleteMate(17);
    expect(sent('assembly.deleteMate')).toEqual([{ id: 17 }]);
    expect(useAssemblyStore.getState().selectedMate).toBeNull();
  });

  it('hides and grounds a piece by its key, and shows every piece', async () => {
    const store = useAssemblyStore.getState();
    await store.setPieceState('hide', '2:1');
    await store.setPieceState('ground', '2:1');
    await store.setPieceState('showAll', null);
    await store.setPieceState('hide', '9:9');
    expect(sent('assembly.pieceState')).toEqual([
      { action: 'hide', piece: { sheet: 2, part: 1, component: null } },
      { action: 'ground', piece: { sheet: 2, part: 1, component: null } },
      { action: 'showAll', piece: null },
    ]);
  });

  it('isolates as a view state: the saved hidden flags stay and leaving shows them again', async () => {
    const hidden = { ...piece(3), hidden: true };
    const rows = [piece(1), piece(2), hidden];
    useAssemblyStore.setState({ pieces: rows });
    useAssemblyStore.getState().isolate('2:1');
    expect(sent('assembly.pieceState')).toEqual([]);
    const isolated = useAssemblyStore.getState().isolated;
    expect(rows.map((p) => isShown(p, isolated))).toEqual([false, true, false]);
    // An isolated piece shows even when it is saved hidden.
    expect(isShown(hidden, '3:1')).toBe(true);
    useAssemblyStore.getState().isolate(null);
    expect(rows.map((p) => isShown(p, useAssemblyStore.getState().isolated))).toEqual([
      true,
      true,
      false,
    ]);
    // Show All ends Isolate as well as showing the hidden pieces.
    useAssemblyStore.getState().isolate('1:1');
    await useAssemblyStore.getState().setPieceState('showAll', null);
    expect(useAssemblyStore.getState().isolated).toBeNull();
  });
});
