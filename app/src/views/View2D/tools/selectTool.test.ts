import { describe, expect, it, vi } from 'vitest';

import type { PointerInfo, ToolHost } from './types';

let answerPick: (hit: boolean) => void = () => undefined;
const hitTest = vi.fn(() => Promise.resolve({ ids: [] as number[] }));
const pickComponentAt = vi.fn(
  () =>
    new Promise<boolean>((resolve) => {
      answerPick = resolve;
    }),
);
const transformSelection = vi.fn(() => Promise.resolve());

vi.mock('../../../bridge/api', () => ({ api: { entities: { hitTest } } }));
vi.mock('../../../state/componentSelection', () => ({
  pickComponentAt,
  clearPickedComponents: vi.fn(),
}));
vi.mock('./carriedComponents', () => ({ carriedComponents: () => [] }));
vi.mock('../../../state/documentStore', () => ({
  useDocumentStore: {
    getState: () => ({
      selection: { ids: [], bbox: null },
      clearSelection: () => Promise.resolve(),
      select: () => Promise.resolve(),
      transformSelection,
      transformSelectionCopy: transformSelection,
    }),
  },
}));

vi.stubGlobal('window', { setTimeout: () => 0, clearTimeout: () => undefined });

const { SelectTool } = await import('./selectTool');

function host(): ToolHost {
  const state = { selectionFrame: null, handleMode: 'none', rotationCentre: null };
  return {
    renderer: {
      state,
      update: vi.fn(),
      getViewport: () => ({ width: 100, height: 100 }),
    },
    requestDraw: vi.fn(),
    probe: vi.fn(),
    clearSnap: vi.fn(),
    setCursor: vi.fn(),
    mmPerPx: () => 1,
    switchTool: vi.fn(),
  } as unknown as ToolHost;
}

function at(x: number, y: number): PointerInfo {
  const point = { x, y };
  return {
    screen: point,
    job: point,
    snapped: point,
    button: 0,
    shift: false,
    ctrl: false,
    alt: false,
  };
}

describe('the select tool', () => {
  it('leaves no drag behind a click that ends while its pick is on the way', async () => {
    const tool = new SelectTool();
    tool.activate(host());
    // A quick click on a component preview: the button is up before the
    // kernel answers the pick.
    const down = tool.pointerDown(at(10, 10));
    await Promise.resolve();
    await tool.pointerUp(at(10, 10));
    answerPick(true);
    await down;
    // Moving the mouse afterwards, button up, and the next click must not
    // move the picked component.
    tool.pointerMove(at(80, 40));
    await tool.pointerUp(at(80, 40));
    expect(transformSelection).not.toHaveBeenCalled();
  });

  it('still drags a component held down', async () => {
    const tool = new SelectTool();
    tool.activate(host());
    const down = tool.pointerDown(at(10, 10));
    await Promise.resolve();
    answerPick(true);
    await down;
    tool.pointerMove(at(40, 10));
    await tool.pointerUp(at(40, 10));
    expect(transformSelection).toHaveBeenCalledWith([1, 0, 0, 1, 30, 0]);
  });
});
