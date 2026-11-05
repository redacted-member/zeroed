import type { JSX } from 'react';

import { chooseMesh } from '../../lib/importModel';
import { furnitureStrings } from '../../lib/strings/furniture';
import { useFurnitureStore } from '../../state/furnitureStore';
import { PopupMenu } from '../PopupMenu/PopupMenu';
import { faceMenuEntries, type FaceAction } from './model';

const text = furnitureStrings.furniture.face;

/**
 * The right click menu of a furniture panel's face in the Assembly View:
 * Add Model to Face..., Edit Face in 2D View and Edit Face in 3D View.
 */
export function FaceMenu(): JSX.Element | null {
  const menu = useFurnitureStore((s) => s.faceMenu);
  if (menu === null) {
    return null;
  }
  const store = () => useFurnitureStore.getState();
  const run = async (action: FaceAction) => {
    store().setFaceMenu(null);
    if (action === 'addModel') {
      const path = await chooseMesh();
      if (path !== null) {
        await store().addModelToFace(menu.target, { file: path });
      }
    } else {
      await store().editFace(menu.target, action === 'edit2d' ? '2d' : '3d');
    }
  };
  return (
    <PopupMenu
      entries={faceMenuEntries()}
      label={text.menu(text.label(menu.target.name, menu.target.hit.face))}
      onClose={() => store().setFaceMenu(null)}
      onSelect={(action) => void run(action)}
      x={menu.x}
      y={menu.y}
    />
  );
}
