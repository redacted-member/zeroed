// The furniture face menu's items, pure so they are tested alone.

import { furnitureStrings } from '../../lib/strings/furniture';
import type { PopupEntry } from '../PopupMenu/model';

const text = furnitureStrings.furniture.face;

export type FaceAction = 'addModel' | 'edit2d' | 'edit3d';

/** Add Model to Face..., then the two edit commands, each with its icon. */
export function faceMenuEntries(): PopupEntry<FaceAction>[] {
  return [
    { id: 'addModel', label: text.addModel, icon: 'importModel', action: 'addModel' },
    { separator: true },
    { id: 'edit2d', label: text.edit2d, icon: 'faceEdit2d', action: 'edit2d' },
    { id: 'edit3d', label: text.edit3d, icon: 'faceEdit3d', action: 'edit3d' },
  ];
}
