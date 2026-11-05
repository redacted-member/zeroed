import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';

import type { MateRow, PieceRow, SolvePlaceReply, Triple } from '../bridge/assembly';
import { setKernelForTests } from '../bridge/client';
import { fakeKernel } from '../testing/fakeKernel';
import { shownPieces, solvesLive, useAssemblyStore } from './assemblyStore';

const piece = (sheet: number): PieceRow => ({
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
});

const mateRow = (suppressed = false): MateRow => ({
  id: 5,
  kind: 'against',
  a: { sheet: 1, part: 1 },
  b: { sheet: 2, part: 1 },
  value: 0,
  aligned: false,
  suppressed,
  state: 'solved',
  distanceError: 0,
  angleError: 0,
});

/** The solve of a drag of sheet 2's piece to `at`: it drops onto z 10, and sheet 1's piece stays. */
const solved = (at: Triple): SolvePlaceReply => ({
  places: [
    { piece: { sheet: 1, part: 1 }, position: [0, 0, 0], rotation: [0, 0, 0] },
    { piece: { sheet: 2, part: 1 }, position: [at[0], at[1], 10], rotation: [0, 0, 0] },
  ],
  conflicts: 0,
  revision: 1,
});

const flush = async () => {
  for (let i = 0; i < 6; i += 1) {
    await Promise.resolve();
  }
};

describe('a drag solved live', () => {
  let asked: Triple[] = [];
  let answers: ((reply: SolvePlaceReply) => void)[] = [];

  beforeAll(() => {
    setKernelForTests(
      fakeKernel((name, args) => {
        if (name === 'assembly.solvePlace') {
          const at = (args as { position: Triple }).position;
          asked.push(at);
          return new Promise<SolvePlaceReply>((resolve) => answers.push(resolve));
        }
        throw new Error(`unexpected ${name}`);
      }),
    );
  });
  afterAll(() => setKernelForTests(null));
  beforeEach(() => {
    asked = [];
    answers = [];
    useAssemblyStore.setState({
      active: true,
      pieces: [piece(1), piece(2)],
      mates: [mateRow()],
      live: null,
    });
  });

  it('asks one solve at a time, the newest place next, and shows each piece where it ends', async () => {
    const store = useAssemblyStore.getState();
    store.setLive({ key: '2:1', position: [1, 0, 40], rotation: [0, 0, 0] });
    store.setLive({ key: '2:1', position: [2, 0, 40], rotation: [0, 0, 0] });
    store.setLive({ key: '2:1', position: [3, 0, 40], rotation: [0, 0, 0] });
    expect(asked).toEqual([[1, 0, 40]]);
    // Before a solve comes back the dragged piece shows where the pointer has it.
    let shown = shownPieces(useAssemblyStore.getState().pieces, useAssemblyStore.getState().live);
    expect(shown[1]?.position).toEqual([3, 0, 40]);
    answers[0]?.(solved([1, 0, 40]));
    await flush();
    // The first solve shows; the newest place is asked next, the middle one skipped.
    shown = shownPieces(useAssemblyStore.getState().pieces, useAssemblyStore.getState().live);
    expect(shown[1]?.position).toEqual([1, 0, 10]);
    expect(asked).toEqual([
      [1, 0, 40],
      [3, 0, 40],
    ]);
    answers[1]?.(solved([3, 0, 40]));
    await flush();
    shown = shownPieces(useAssemblyStore.getState().pieces, useAssemblyStore.getState().live);
    expect(shown[1]?.position).toEqual([3, 0, 10]);
    expect(shown[0]?.position).toEqual([0, 0, 0]);
    expect(asked.length).toBe(2);
  });

  it('drops a solve that comes back after the drag ends, and solves nothing without mates', async () => {
    const store = useAssemblyStore.getState();
    store.setLive({ key: '2:1', position: [1, 0, 40], rotation: [0, 0, 0] });
    store.setLive(null);
    answers[0]?.(solved([1, 0, 40]));
    await flush();
    expect(useAssemblyStore.getState().live).toBeNull();
    useAssemblyStore.setState({ mates: [mateRow(true)] });
    expect(solvesLive(useAssemblyStore.getState().mates)).toBe(false);
    useAssemblyStore.getState().setLive({ key: '2:1', position: [4, 0, 40], rotation: [0, 0, 0] });
    expect(asked.length).toBe(1);
    const shown = shownPieces(useAssemblyStore.getState().pieces, useAssemblyStore.getState().live);
    expect(shown[1]?.position).toEqual([4, 0, 40]);
    useAssemblyStore.getState().setLive(null);
  });
});
