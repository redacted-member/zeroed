import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';

import type { PieceRow } from '../bridge/assembly';
import type { VerifyFitReply } from '../bridge/assemblyFit';
import { setKernelForTests } from '../bridge/client';
import { fakeKernel } from '../testing/fakeKernel';
import {
  clampFraction,
  defaultFit,
  fitTarget,
  placesKey,
  useAssemblyFitStore,
} from './assemblyFitStore';
import { useAssemblyStore } from './assemblyStore';

const piece = (sheet: number, x = 0): PieceRow => ({
  sheet,
  part: 1,
  parts: 1,
  sheetName: `Sheet ${sheet}`,
  vectorName: null,
  model: false,
  position: [x, 0, 0],
  rotation: [0, 0, 0],
  size: [100, 50, 10],
  hasModel: false,
  hasToolpaths: true,
});

describe('fit defaults', () => {
  it('starts on 0.2 mm plus or minus 0.1, or 0.008 inch plus or minus 0.004', () => {
    expect(defaultFit('mm')).toEqual({ target: 0.2, tolerance: 0.1 });
    expect(defaultFit('inch').target).toBeCloseTo(0.2032, 6);
    expect(defaultFit('inch').tolerance).toBeCloseTo(0.1016, 6);
    expect(fitTarget({ target: 0.3, tolerance: null }, 'mm')).toEqual({
      target: 0.3,
      tolerance: 0.1,
    });
  });

  it('keys the places the fit was measured on and holds the explode to 0..1', () => {
    expect(placesKey([piece(1)])).not.toBe(placesKey([piece(1, 5)]));
    expect(clampFraction(1.5)).toBe(1);
    expect(clampFraction(-1)).toBe(0);
    expect(clampFraction(Number.NaN)).toBe(0);
  });
});

describe('fit store', () => {
  let calls: { target: number; tolerance: number; leaveOut: unknown[] }[] = [];
  let measured: unknown[] = [];
  const reply: VerifyFitReply = {
    joints: [],
    explode: [{ piece: { sheet: 2, part: 1 }, offset: [0, 0, 30] }],
    exact: [{ sheet: 1, part: 1 }],
    target: 0.2,
    tolerance: 0.1,
  };

  beforeAll(() => {
    setKernelForTests(
      fakeKernel((name, args) => {
        if (name === 'assembly.verifyFit') {
          calls.push(args as { target: number; tolerance: number; leaveOut: unknown[] });
          return reply;
        }
        if (name === 'assembly.measure') {
          measured.push(args);
          return {
            a: { point: [0, 0, 0], normal: [1, 0, 0], exact: true },
            b: { point: [0.2, 0, 0], normal: [-1, 0, 0], exact: true },
            distance: 0.2,
            delta: [0.2, 0, 0],
            parallel: true,
            faceDistance: 0.2,
            angle: 180,
          };
        }
        throw new Error(`unexpected ${name}`);
      }),
    );
  });
  afterAll(() => setKernelForTests(null));
  beforeEach(() => {
    calls = [];
    measured = [];
    useAssemblyStore.setState({ active: true, pieces: [piece(1), piece(2)] });
    useAssemblyFitStore.getState().reset();
  });

  it('verifies on the target set and keeps each piece its explode offset', async () => {
    useAssemblyFitStore.getState().setTarget(0.25);
    await useAssemblyFitStore.getState().verify();
    expect(calls).toEqual([{ target: 0.25, tolerance: 0.1, leaveOut: [] }]);
    const state = useAssemblyFitStore.getState();
    expect(state.offsets['2:1']).toEqual([0, 0, 30]);
    expect(state.exact).toEqual(['1:1']);
    expect(state.joints).toEqual([]);
  });

  it('checks the hidden pieces unless told to leave out the pieces not shown', async () => {
    useAssemblyStore.setState({
      pieces: [piece(1), { ...piece(2), hidden: true }, piece(3)],
      isolated: null,
    });
    await useAssemblyFitStore.getState().verify();
    expect(calls[0]?.leaveOut).toEqual([]);
    useAssemblyFitStore.getState().setIncludeHidden(false);
    expect(useAssemblyFitStore.getState().joints).toBeNull();
    await useAssemblyFitStore.getState().verify();
    expect(calls[1]?.leaveOut).toEqual([{ sheet: 2, part: 1, component: null }]);
    // Isolated, every piece but the isolated one is left out.
    useAssemblyStore.setState({ isolated: '3:1' });
    await useAssemblyFitStore.getState().verify();
    expect(calls[2]?.leaveOut).toEqual([
      { sheet: 1, part: 1, component: null },
      { sheet: 2, part: 1, component: null },
    ]);
    useAssemblyStore.setState({ isolated: null });
    useAssemblyFitStore.getState().setIncludeHidden(true);
  });

  it('works the explode out the first time it moves, and again once a piece moves', async () => {
    useAssemblyFitStore.getState().setExplode(0.5);
    expect(useAssemblyFitStore.getState().explode).toBe(0.5);
    await Promise.resolve();
    await Promise.resolve();
    expect(calls.length).toBe(1);
    useAssemblyStore.setState({ pieces: [piece(1), piece(2, 7)] });
    expect(calls.length).toBe(2);
  });

  it('measures between two picked points, then forgets it once a piece moves', async () => {
    const store = useAssemblyFitStore.getState();
    store.startMeasure();
    await useAssemblyFitStore.getState().measurePick({ sheet: 1, part: 1, point: [1, 2, 3] });
    expect(useAssemblyFitStore.getState().measure.step).toBe('second');
    await useAssemblyFitStore.getState().measurePick({ sheet: 2, part: 1, point: [4, 5, 6] });
    expect(measured).toEqual([
      {
        a: { sheet: 1, part: 1, component: null, point: [1, 2, 3] },
        b: { sheet: 2, part: 1, component: null, point: [4, 5, 6] },
      },
    ]);
    expect(useAssemblyFitStore.getState().measure.result?.faceDistance).toBe(0.2);
    useAssemblyStore.setState({ pieces: [piece(1), piece(2, 9)] });
    expect(useAssemblyFitStore.getState().measure.result).toBeNull();
  });

  it('keeps section changes and clears them when the view closes', () => {
    useAssemblyFitStore.getState().setSection({ on: true, axis: 'z', offset: 4 });
    expect(useAssemblyFitStore.getState().section).toEqual({
      on: true,
      axis: 'z',
      offset: 4,
      flip: false,
    });
    useAssemblyStore.setState({ active: false });
    expect(useAssemblyFitStore.getState().section.on).toBe(false);
  });

  it('puts the pieces back together when the view closes', () => {
    useAssemblyFitStore.setState({ explode: 1, showFit: true, picked: 0 });
    useAssemblyStore.setState({ active: false });
    const state = useAssemblyFitStore.getState();
    expect(state.explode).toBe(0);
    expect(state.picked).toBeNull();
  });
});
