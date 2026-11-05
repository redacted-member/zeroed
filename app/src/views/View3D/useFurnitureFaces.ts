// Furniture panel faces in the Assembly View: a right click on a panel's
// broad face opens the face menu (Add Model to Face..., Edit Face in 2D or
// 3D View), a clip dropped on one lands on that face, and the status line
// names the panel and face under the pointer. Narrow edges are refused in
// plain words.

import { useCallback, useEffect, type RefObject } from 'react';

import { pieceKey } from '../../bridge/assembly';
import { furnitureStrings } from '../../lib/strings/furniture';
import { registerFaceDrop } from '../../panels/ClipartPanel/drop';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useFaceEditStore } from '../../state/faceEditStore';
import { faceTargetOf, type FacePick } from '../../state/furnitureFaces';
import { useFurnitureStore } from '../../state/furnitureStore';
import type { View3DRenderer } from './renderer';

const text = furnitureStrings.furniture.face;

export interface FurnitureFaces {
  /** A right click; true when it was on a furniture panel. */
  contextAt: (clientX: number, clientY: number, ndcX: number, ndcY: number) => boolean;
  /** The pointer over the view (null: it left): names the panel and face for the status line. */
  hoverAt: (ndcX: number | null, ndcY?: number) => void;
}

export function useFurnitureFaces(
  rendererRef: RefObject<View3DRenderer | null>,
  canvasRef: RefObject<HTMLCanvasElement | null>,
): FurnitureFaces {
  const assembly = useAssemblyStore((s) => s.active);
  const designs = useFurnitureStore((s) => s.designs);
  const active = assembly && designs.length > 0;

  const pickAt = useCallback(
    (ndcX: number, ndcY: number): FacePick => {
      const renderer = rendererRef.current;
      if (renderer === null) {
        return null;
      }
      const hit = renderer.pickPiece(ndcX, ndcY);
      if (hit === null) {
        return null;
      }
      const piece = useAssemblyStore.getState().pieces.find((p) => pieceKey(p) === hit.key);
      return piece === undefined ? null : faceTargetOf(piece, hit.point);
    },
    [rendererRef],
  );

  const contextAt = useCallback(
    (clientX: number, clientY: number, ndcX: number, ndcY: number): boolean => {
      if (!active) {
        return false;
      }
      const pick = pickAt(ndcX, ndcY);
      if (pick === null) {
        return false;
      }
      if (pick.kind === 'edge') {
        useAssemblyStore.getState().setNotice(text.edgeRefused);
        return true;
      }
      useFurnitureStore.getState().setFaceMenu({ x: clientX, y: clientY, target: pick.target });
      return true;
    },
    [active, pickAt],
  );

  const hoverAt = useCallback(
    (ndcX: number | null, ndcY?: number) => {
      const status = useFaceEditStore.getState();
      if (!active || ndcX === null || ndcY === undefined) {
        status.setPointerPanel(null);
        return;
      }
      const pick = pickAt(ndcX, ndcY);
      status.setPointerPanel(
        pick === null
          ? null
          : pick.kind === 'edge'
            ? pick.name
            : text.label(pick.target.name, pick.target.hit.face),
      );
    },
    [active, pickAt],
  );

  // A clip let go over a panel's face goes on that face.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!active || canvas === null) {
      return undefined;
    }
    return registerFaceDrop(canvas, (clientX, clientY, clip) => {
      const rect = canvas.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) {
        return false;
      }
      const pick = pickAt(
        ((clientX - rect.left) / rect.width) * 2 - 1,
        -(((clientY - rect.top) / rect.height) * 2 - 1),
      );
      if (pick === null) {
        return false;
      }
      if (pick.kind === 'edge') {
        useAssemblyStore.getState().setNotice(text.edgeRefused);
        return true;
      }
      void useFurnitureStore.getState().addModelToFace(pick.target, { clipart: clip });
      return true;
    });
  }, [active, canvasRef, pickAt]);

  useEffect(() => {
    if (!active) {
      useFaceEditStore.getState().setPointerPanel(null);
    }
  }, [active]);

  return { contextAt, hoverAt };
}
