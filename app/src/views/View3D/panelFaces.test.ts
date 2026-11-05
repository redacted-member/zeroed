import { describe, expect, it } from 'vitest';

import type { DesignRow } from '../../bridge/furniture';
import { faceOfPieceHit, madeOf, panelOfPiece } from './panelFaces';

// A door 400 by 700 by 18 standing at (100, 50, 400), not turned.
const DOOR = {
  position: [100, 50, 400] as [number, number, number],
  rotation: [0, 0, 0] as [number, number, number],
  size: [400, 700, 18] as [number, number, number],
};

describe('faceOfPieceHit', () => {
  it('finds the top face and where on the panel the point is', () => {
    const hit = faceOfPieceHit(DOOR, [150, 100, 409]);
    expect(hit.kind).toBe('face');
    if (hit.kind === 'face') {
      expect(hit.face).toBe('top');
      // 50 right of the middle of 400 wide is 250; 50 up of the middle of 700 is 400.
      expect(hit.at[0]).toBeCloseTo(250, 9);
      expect(hit.at[1]).toBeCloseTo(400, 9);
    }
  });

  it('finds the bottom face', () => {
    const hit = faceOfPieceHit(DOOR, [100, 50, 391]);
    expect(hit).toMatchObject({ kind: 'face', face: 'bottom' });
  });

  it('refuses an edge face, at the tolerance edge', () => {
    // Top is 409; 408.5 is exactly the tolerance, 408.49 is past it.
    expect(faceOfPieceHit(DOOR, [300, 50, 408.5]).kind).toBe('face');
    expect(faceOfPieceHit(DOOR, [300, 50, 408.49]).kind).toBe('edge');
    expect(faceOfPieceHit(DOOR, [300, 50, 400]).kind).toBe('edge');
  });

  it('follows a piece stood up on its edge', () => {
    // Turned 90 degrees about X: the part's Z (thickness) now runs along -Y.
    const standing = { ...DOOR, rotation: [90, 0, 0] as [number, number, number] };
    const front = faceOfPieceHit(standing, [100, 50 - 9, 400]);
    const back = faceOfPieceHit(standing, [100, 50 + 9, 400]);
    expect([front.kind, back.kind]).toEqual(['face', 'face']);
    expect(
      front.kind === 'face' && back.kind === 'face' ? [front.face, back.face].sort() : [],
    ).toEqual(['bottom', 'top']);
  });
});

function design(id: number, made: DesignRow['made']): DesignRow {
  return {
    id,
    name: `Design ${id}`,
    template: 'bookcase',
    values: {},
    style: 'template',
    classJoints: {},
    connectionJoints: {},
    made,
  };
}

describe('panelOfPiece and madeOf', () => {
  const designs = [
    design(1, { left: { vector: 10, sheet: 2, piece: 0, name: 'Left side' } }),
    design(2, {
      door: { vector: 20, sheet: 3, piece: 1 },
      back: { vector: 21, sheet: 4, piece: 0 },
    }),
  ];

  it('finds the design and panel a piece was made for', () => {
    expect(panelOfPiece(designs, '3:1')).toMatchObject({
      panel: 'door',
      name: 'door',
      design: { id: 2 },
    });
    expect(panelOfPiece(designs, '2:0')?.name).toBe('Left side');
  });

  it('answers null for a piece no design made', () => {
    expect(panelOfPiece(designs, '9:0')).toBeNull();
    expect(panelOfPiece([design(3, undefined)], '2:0')).toBeNull();
  });

  it('gives the sheet and vector of a panel', () => {
    const second = designs[1];
    if (second === undefined) {
      throw new Error('no design');
    }
    expect(madeOf(second, 'back')).toEqual({ sheet: 4, vector: 21 });
    expect(madeOf(second, 'shelf')).toBeNull();
  });
});
