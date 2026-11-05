// Where a form opens. Toolpath forms open in the Toolpaths panel on the
// right; every other form opens in the left panel, on the Design tab (or on
// the Components tab a component form was opened from).

import type { FormId } from './uiStore';

export type Dock = 'left' | 'right';

const RIGHT_DOCK_FORMS: ReadonlySet<FormId> = new Set<FormId>([
  'materialSetup',
  'saveToolpaths',
  'mergeToolpaths',
  'arrayCopyToolpath',
  'toolpathTiling',
  'photoCarveToolpath',
  'importedToolpath',
  'previewToolpaths',
  'profileToolpath',
  'pocketToolpath',
  'drillToolpath',
  'drillBankToolpath',
  'vgrooveToolpath',
  'inlayToolpath',
  'mouldingToolpath',
  'quickEngraveToolpath',
  'chamferToolpath',
  'threadMillingToolpath',
  'straightInlayToolpath',
  'flutingToolpath',
  'prismToolpath',
  'textureToolpath',
  'rough3dToolpath',
  'finish3dToolpath',
  'laserCutFillToolpath',
  'laserPictureToolpath',
  'sketchToolpath',
  'laserSketchToolpath',
  'assemblyView',
  'furnitureDesigner',
]);

const MODELING_FORMS: ReadonlySet<FormId> = new Set<FormId>([
  'createShape',
  'createFlatShape',
  'createAngledShape',
  'createSmoothShape',
  'createCustomShape',
  'importModel',
  'positionModel',
  'segmentModel',
  'twoRailSweep',
  'turnSpin',
  'extrudeWeave',
  'textureArea',
  'componentEdit',
  'componentProperties',
  'componentTransform',
  'replaceBelow',
  'crossSection',
  'sliceModel',
  'smoothComponents',
  'embossComponent',
  'scaleModelHeight',
  'offsetModel',
  'addDraft',
  'sculpt',
  'exportStl',
]);

/** The dock `form` opens in. */
export function formDock(form: FormId): Dock {
  return RIGHT_DOCK_FORMS.has(form) ? 'right' : 'left';
}

/** Whether `form` is one of the Modeling Tools forms (or a component form). */
export function isModelingForm(form: FormId): boolean {
  return MODELING_FORMS.has(form);
}
