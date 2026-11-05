import { describe, expect, it } from 'vitest';

import type { PieceRow } from '../bridge/assembly';
import type { Heightfield, ToolpathSummary } from '../bridge/cam';
import { pieceKey, pieceRef } from '../bridge/assembly';
import {
  componentPiecesKey,
  NO_MATE,
  reliefsToLoad,
  shownPieces,
  toolpathsKey,
  withFace,
} from './assemblyStore';

const piece = (sheet: number, width = 100, part = 1): PieceRow => ({
  sheet,
  part,
  parts: 1,
  sheetName: `Sheet ${sheet}`,
  vectorName: null,
  model: false,
  position: [0, 0, 0],
  rotation: [0, 0, 0],
  size: [width, 50, 10],
  hasModel: false,
  hasToolpaths: true,
});

const relief = {} as Heightfield;

describe('shownPieces', () => {
  it('shows a dragged piece where the drag has it and the rest where they are', () => {
    const shown = shownPieces([piece(1), piece(2)], {
      key: '2:1',
      position: [5, 6, 7],
      rotation: [90, 0, 0],
    });
    expect(shown[0]?.position).toEqual([0, 0, 0]);
    expect(shown[1]?.position).toEqual([5, 6, 7]);
    expect(shown[1]?.rotation).toEqual([90, 0, 0]);
    expect(shownPieces([piece(1)], null)[0]?.position).toEqual([0, 0, 0]);
  });
});

describe('reliefsToLoad', () => {
  it('loads new pieces and resized ones, and keeps the rest', () => {
    const before = [piece(1), piece(2)];
    const now = [piece(1), piece(2, 120), piece(3), piece(3, 100, 2)];
    const kept = { '1:1': relief, '2:1': relief };
    expect(reliefsToLoad(now, before, kept, false)).toEqual(['2:1', '3:1', '3:2']);
    expect(reliefsToLoad(now, before, kept, true)).toEqual(['1:1', '2:1', '3:1', '3:2']);
  });
});

describe('withFace', () => {
  const face = (part: number) => ({ sheet: 1, part, point: [1, 2, 3] as [number, number, number] });

  it('takes the face that stays, then a face on another piece, then mates', () => {
    const first = withFace({ ...NO_MATE, step: 'target', turn: 90 }, face(1));
    expect(first.ready).toBe(false);
    expect(first.mate.step).toBe('moving');
    expect(first.mate.target).toEqual(face(1));
    const same = withFace(first.mate, face(1));
    expect(same.ready).toBe(false);
    expect(same.mate.step).toBe('moving');
    const second = withFace(first.mate, face(2));
    expect(second.ready).toBe(true);
    expect(second.mate).toMatchObject({ step: null, moving: face(2), turn: null });
  });

  it('ignores faces while no mate waits for one', () => {
    expect(withFace(NO_MATE, face(1))).toEqual({ mate: NO_MATE, ready: false });
  });
});

describe('toolpathsKey', () => {
  const toolpath = (visible: boolean, moves = 10): ToolpathSummary => ({
    id: 1,
    name: 'Profile',
    kind: 'profile',
    visible,
    stale: false,
    toolName: 'End Mill',
    toolNumber: 1,
    estimatedSeconds: 60,
    moves,
    sheet: 2,
  });

  it('changes with what the simulation cuts, not with a new list of the same', () => {
    expect(toolpathsKey([toolpath(true)])).toBe(toolpathsKey([toolpath(true)]));
    expect(toolpathsKey([toolpath(true)])).not.toBe(toolpathsKey([toolpath(false)]));
    expect(toolpathsKey([toolpath(true)])).not.toBe(toolpathsKey([toolpath(true, 11)]));
  });
});

describe('component pieces', () => {
  it('keys a component piece by its component, apart from the part of its sheet', () => {
    expect(pieceKey({ sheet: 1, part: 1 })).toBe('1:1');
    expect(pieceKey({ sheet: 1, part: 1, component: null })).toBe('1:1');
    expect(pieceKey({ sheet: 1, part: 1, component: 7 })).toBe('c7');
    expect(pieceRef({ ...piece(1), component: 7 })).toEqual({ sheet: 1, part: 1, component: 7 });
    expect(pieceRef(piece(1))).toEqual({ sheet: 1, part: 1, component: null });
  });

  it('notices a component joining or leaving the pieces, so the sheet models load again', () => {
    const plain = [piece(1), { ...piece(1, 100, 0), model: true }];
    const withKnob = [...plain, { ...piece(1), component: 4 }];
    expect(componentPiecesKey(plain)).toBe('');
    expect(componentPiecesKey(withKnob)).toBe('4');
    expect(componentPiecesKey(withKnob)).not.toBe(componentPiecesKey(plain));
  });

  it('a mate between a component piece and a part needs two different pieces', () => {
    const knob = { sheet: 1, part: 1, component: 4, point: [1, 2, 3] as [number, number, number] };
    const part = { sheet: 1, part: 1, point: [1, 2, 3] as [number, number, number] };
    const first = withFace({ ...NO_MATE, step: 'target' }, part);
    expect(withFace(first.mate, knob).ready).toBe(true);
  });
});
