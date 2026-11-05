// Which form Edit opens for a toolpath in the list.

import type { ToolpathSummary } from '../../bridge/cam';
import type { FormId } from '../../state/uiStore';

/** The form that edits a toolpath of this kind, if this build has one. */
export function formForKind(kind: string): FormId | null {
  switch (kind) {
    case 'profile':
      return 'profileToolpath';
    case 'pocket':
      return 'pocketToolpath';
    case 'drill':
      return 'drillToolpath';
    case 'vGroove':
      return 'vgrooveToolpath';
    case 'inlay':
      return 'inlayToolpath';
    case 'moulding':
      return 'mouldingToolpath';
    case 'quickEngrave':
      return 'quickEngraveToolpath';
    case 'chamfer':
      return 'chamferToolpath';
    case 'photoCarve':
      return 'photoCarveToolpath';
    case 'imported':
      return 'importedToolpath';
    case 'straightInlay':
      return 'straightInlayToolpath';
    case 'threadMilling':
      return 'threadMillingToolpath';
    case 'fluting':
      return 'flutingToolpath';
    case 'prismCarve':
      return 'prismToolpath';
    case 'texture':
      return 'textureToolpath';
    case 'rough3d':
      return 'rough3dToolpath';
    case 'finish3d':
      return 'finish3dToolpath';
    default:
      return null;
  }
}

/** The form of a strategy whose kind other strategies share. */
function formForStrategy(strategy: string | null | undefined): FormId | null {
  switch (strategy) {
    case 'laserCutFill':
      return 'laserCutFillToolpath';
    case 'laserPicture':
      return 'laserPictureToolpath';
    case 'laserSketch':
      return 'laserSketchToolpath';
    case 'sketchCarve':
      return 'sketchToolpath';
    case 'drillBank':
      return 'drillBankToolpath';
    default:
      return null;
  }
}

/** The form Edit opens for a toolpath: the gadget that made it, if one
 * did, else the form of its strategy. */
export function formForToolpath(
  toolpath: Pick<ToolpathSummary, 'kind' | 'gadget' | 'strategy'>,
): FormId | null {
  if (typeof toolpath.gadget === 'string') {
    return 'gadget';
  }
  return formForStrategy(toolpath.strategy) ?? formForKind(toolpath.kind);
}
