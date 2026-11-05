// Furniture panels in the Assembly View: which design and panel a piece
// is, and which face of it a pick or a drop lands on. A piece's own frame
// is its part on the sheet (X and Y across the sheet, Z up through the
// thickness), so the face up on the sheet is the top and only the two
// broad faces take a model; the narrow edges cannot be carved from a flat
// sheet.

import type { PieceRow } from '../../bridge/assembly';
import type { DesignRow, PanelFace } from '../../bridge/furniture';
import { toPieceLocal } from './pieceGizmo';

/** A pick this close to a broad face (mm) counts as on it. */
export const FACE_TOLERANCE = 0.5;

export type FaceHit =
  | {
      kind: 'face';
      face: PanelFace;
      /** Where on the panel: mm from its part's lower left on the sheet. */
      at: [number, number];
    }
  | { kind: 'edge' };

/** Which face of a piece an assembly space point on it is on. */
export function faceOfPieceHit(
  piece: Pick<PieceRow, 'position' | 'rotation' | 'size'>,
  point: readonly [number, number, number],
  tolerance: number = FACE_TOLERANCE,
): FaceHit {
  // The local frame starts at the part's corner, Z up from its underside.
  const [x, y, z] = toPieceLocal(piece, [point[0], point[1], point[2]]);
  const t = piece.size[2];
  const at: [number, number] = [x, y];
  if (z >= t - tolerance) {
    return { kind: 'face', face: 'top', at };
  }
  if (z <= tolerance) {
    return { kind: 'face', face: 'bottom', at };
  }
  return { kind: 'edge' };
}

export interface PiecePanel {
  design: DesignRow;
  panel: string;
  name: string;
}

/** The design and panel a piece (key `sheet:part`) was made for, or null for any other piece. */
export function panelOfPiece(designs: readonly DesignRow[], key: string): PiecePanel | null {
  for (const design of designs) {
    for (const [panel, made] of Object.entries(design.made ?? {})) {
      if (`${made.sheet}:${made.piece}` === key) {
        return { design, panel, name: made.name ?? panel };
      }
    }
  }
  return null;
}

/** The sheet and vector of a panel, for Edit Face in 2D or 3D View. */
export function madeOf(design: DesignRow, panel: string): { sheet: number; vector: number } | null {
  const made = design.made?.[panel];
  return made === undefined ? null : { sheet: made.sheet, vector: made.vector };
}
