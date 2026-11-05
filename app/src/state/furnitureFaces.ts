// Which furniture panel face a piece and a point on it are, from the
// Assembly View's pieces and the designs' made panels.

import { pieceKey, type PieceRef } from '../bridge/assembly';
import { faceOfPieceHit, panelOfPiece } from '../views/View3D/panelFaces';
import { useAssemblyStore } from './assemblyStore';
import { useFurnitureStore, type FaceTarget } from './furnitureStore';

/** A furniture face, a narrow edge of a furniture panel (`edge` names it), or nothing of a design. */
export type FacePick = { kind: 'face'; target: FaceTarget } | { kind: 'edge'; name: string } | null;

/** What a piece and a point on it (assembly space) are, for a design's panels. */
export function faceTargetOf(ref: PieceRef, point: readonly [number, number, number]): FacePick {
  const key = pieceKey(ref);
  const piece = useAssemblyStore.getState().pieces.find((p) => pieceKey(p) === key);
  const owner = panelOfPiece(useFurnitureStore.getState().designs, key);
  if (piece === undefined || owner === null) {
    return null;
  }
  const hit = faceOfPieceHit(piece, point);
  if (hit.kind === 'edge') {
    return { kind: 'edge', name: owner.name };
  }
  return {
    kind: 'face',
    target: { design: owner.design.id, panel: owner.panel, name: owner.name, hit },
  };
}
