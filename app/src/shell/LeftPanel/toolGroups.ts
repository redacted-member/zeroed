// Tool grids of the Design tab (left dock) and of the Toolpaths panel (right
// dock), in the groups, rows and order. A tool with a form id
// opens that form, one with a command runs it at once. Some groups and tools
// show only with a preference or on a rotary job (`showIf`, `visibleGroups`).

import { strings } from '../../lib/strings';
import type { AppCommand } from '../../lib/shortcuts';
import type { FormId } from '../../state/uiStore';

/** The three editing modes of the 2D view. */
export type EditMode = 'select' | 'node' | 'transform';

/**
 * What an optional group or tool waits for: the extra File Operations row
 * (Preferences, Show additional file operations), the Align Objects group
 * (Alignment Tools, Show common tools on drawing tab) or a rotary job.
 */
export type DesignOption = 'fileOperations' | 'alignTools' | 'rotaryJob';

export interface ToolSlot {
  id: string;
  label: string;
  form?: FormId;
  /** Runs at once instead of opening a form. */
  command?: AppCommand;
  /** Runs instead when the tool is clicked with Shift held. */
  shiftCommand?: AppCommand;
  /** The editing mode this tool picks; its button shows pressed while on. */
  mode?: EditMode;
  /** Short text shown when the tool has no icon in shell/ToolIcons. */
  short: string;
  /** The tool starts a new row of its group's grid. */
  rowStart?: boolean;
  /** Shown only while this option is on. */
  showIf?: DesignOption;
}

export interface ToolGroup {
  id: string;
  title: string;
  tools: readonly ToolSlot[];
  /** Shown only while this option is on. */
  showIf?: DesignOption;
}

/** The groups and tools to show while the options in `on` are on. */
export function visibleGroups(
  groups: readonly ToolGroup[],
  on: ReadonlySet<DesignOption>,
): ToolGroup[] {
  const shown = (item: { showIf?: DesignOption }) =>
    item.showIf === undefined || on.has(item.showIf);
  return groups
    .filter(shown)
    .map((group) => ({ ...group, tools: group.tools.filter(shown) }))
    .filter((group) => group.tools.length > 0);
}

const t = strings.leftPanel.tools;

/** The tool sets: the Design tab and the Toolpaths panel. */
export type ToolSet = 'design' | 'toolpaths';

export const TOOL_GROUPS: Record<ToolSet, readonly ToolGroup[]> = {
  design: [
    {
      id: 'fileOperations',
      title: strings.leftPanel.groups.fileOperations,
      tools: [
        {
          id: 'fileNew',
          label: t.fileNew,
          command: 'file.new',
          short: 'New',
          showIf: 'fileOperations',
        },
        {
          id: 'fileSave',
          label: t.fileSave,
          command: 'file.save',
          short: 'Save',
          showIf: 'fileOperations',
        },
        {
          id: 'editUndo',
          label: t.editUndo,
          command: 'edit.undo',
          short: 'Undo',
          showIf: 'fileOperations',
        },
        {
          id: 'editRedo',
          label: t.editRedo,
          command: 'edit.redo',
          short: 'Redo',
          showIf: 'fileOperations',
        },
        { id: 'jobSetup', label: t.jobSetup, form: 'jobSetup', short: 'Job', rowStart: true },
        { id: 'fileOpen', label: t.fileOpen, command: 'file.open', short: 'Open' },
        {
          id: 'importVectors',
          label: t.importVectors,
          command: 'file.importVectorsBitmaps',
          short: 'Imp',
        },
        { id: 'importModel', label: t.importModel, form: 'importModel', short: 'Imp3D' },
        {
          id: 'bitmapRelief',
          label: t.bitmapRelief,
          command: 'model.fromBitmap',
          short: 'Bmp',
        },
        { id: 'exportModel', label: t.exportModel, form: 'exportStl', short: 'STL' },
      ],
    },
    {
      id: 'createVectors',
      title: strings.leftPanel.groups.createVectors,
      tools: [
        { id: 'drawCircle', label: t.drawCircle, form: 'drawCircle', short: 'Circ' },
        { id: 'drawEllipse', label: t.drawEllipse, form: 'drawEllipse', short: 'Elli' },
        { id: 'drawRectangle', label: t.drawRectangle, form: 'drawRectangle', short: 'Rect' },
        { id: 'drawPolygon', label: t.drawPolygon, form: 'drawPolygon', short: 'Poly' },
        { id: 'drawStar', label: t.drawStar, form: 'drawStar', short: 'Star' },
        {
          id: 'drawPolyline',
          label: t.drawPolyline,
          form: 'drawPolyline',
          short: 'Line',
          rowStart: true,
        },
        { id: 'drawArc', label: t.drawArc, form: 'drawArc', short: 'Arc' },
        { id: 'drawFreehand', label: t.drawFreehand, form: 'drawFreehand', short: 'Free' },
        { id: 'vectorTexture', label: t.vectorTexture, form: 'vectorTexture', short: 'VTex' },
        { id: 'dimensions', label: t.dimensions, form: 'dimensions', short: 'Dim' },
        {
          id: 'createText',
          label: t.createText,
          form: 'createText',
          short: 'Text',
          rowStart: true,
        },
        {
          id: 'textToCurves',
          label: t.textToCurves,
          command: 'edit.textToCurves',
          short: 'TxtC',
        },
        { id: 'textOnCurve', label: t.textOnCurve, form: 'textOnCurve', short: 'ToC' },
        { id: 'traceBitmap', label: t.traceBitmap, form: 'traceBitmap', short: 'Trace' },
      ],
    },
    {
      id: 'transformObjects',
      title: strings.leftPanel.groups.transformObjects,
      tools: [
        { id: 'move', label: t.move, form: 'move', short: 'Move' },
        { id: 'setSize', label: t.setSize, form: 'setSize', short: 'Size' },
        { id: 'rotate', label: t.rotate, form: 'rotate', short: 'Rot' },
        { id: 'mirror', label: t.mirror, form: 'mirror', short: 'Mirr' },
        { id: 'distort', label: t.distort, form: 'distort', short: 'Dist' },
        { id: 'align', label: t.align, form: 'align', short: 'Algn' },
        {
          id: 'vectorUnwrapper',
          label: t.vectorUnwrapper,
          form: 'vectorUnwrapper',
          short: 'Unwr',
          rowStart: true,
          showIf: 'rotaryJob',
        },
      ],
    },
    {
      id: 'editObjects',
      title: strings.leftPanel.groups.editObjects,
      tools: [
        {
          id: 'vectorSelection',
          label: t.vectorSelection,
          command: 'tool.select',
          mode: 'select',
          short: 'Sel',
        },
        {
          id: 'nodeEditing',
          label: t.nodeEditing,
          command: 'tool.node',
          mode: 'node',
          short: 'Node',
        },
        {
          id: 'interactiveSelection',
          label: t.interactiveSelection,
          command: 'tool.transform',
          mode: 'transform',
          short: 'Xfrm',
        },
        { id: 'group', label: t.group, command: 'edit.group', short: 'Grp' },
        { id: 'ungroup', label: t.ungroup, command: 'edit.ungroup', short: 'Ungr' },
        { id: 'measure', label: t.measure, form: 'measure', short: 'Meas' },
        { id: 'weld', label: t.weld, command: 'edit.weld', short: 'Weld', rowStart: true },
        { id: 'subtract', label: t.subtract, command: 'edit.subtract', short: 'Sub' },
        { id: 'intersect', label: t.intersect, command: 'edit.intersect', short: 'Int' },
        { id: 'trimObjects', label: t.trimObjects, form: 'trimObjects', short: 'TrmO' },
        { id: 'trim', label: t.trim, form: 'trim', short: 'Trim' },
        { id: 'validator', label: t.validator, form: 'validator', short: 'Vald' },
        { id: 'fillet', label: t.fillet, form: 'fillet', short: 'Fill', rowStart: true },
        { id: 'extend', label: t.extend, form: 'extend', short: 'Ext' },
        { id: 'fitCurves', label: t.fitCurves, form: 'fitCurves', short: 'Fit' },
        { id: 'boundary', label: t.boundary, form: 'boundary', short: 'Bnd' },
        { id: 'editPicture', label: t.editPicture, form: 'editPicture', short: 'Pict' },
        { id: 'cropBitmap', label: t.cropBitmap, command: 'bitmap.crop', short: 'CrpB' },
        { id: 'join', label: t.join, form: 'join', short: 'Join', rowStart: true },
        { id: 'close', label: t.close, command: 'edit.closeVectors', short: 'Clos' },
        {
          id: 'closeSmooth',
          label: t.closeSmooth,
          command: 'edit.closeSmooth',
          short: 'ClsS',
        },
        {
          id: 'closeMoveEnds',
          label: t.closeMoveEnds,
          command: 'edit.closeMoveEnds',
          short: 'ClsM',
        },
      ],
    },
    {
      id: 'alignObjects',
      title: strings.leftPanel.groups.alignObjects,
      showIf: 'alignTools',
      tools: [
        {
          id: 'alignMaterial',
          label: t.alignMaterial,
          command: 'align.centreInMaterial',
          short: 'Mat',
        },
        {
          id: 'alignMaterialX',
          label: t.alignMaterialX,
          command: 'align.inMaterialX',
          short: 'MatX',
        },
        {
          id: 'alignMaterialY',
          label: t.alignMaterialY,
          command: 'align.inMaterialY',
          short: 'MatY',
        },
        {
          id: 'alignLeft',
          label: t.alignLeft,
          command: 'align.lastLeft',
          short: 'L',
          rowStart: true,
        },
        { id: 'alignRight', label: t.alignRight, command: 'align.lastRight', short: 'R' },
        { id: 'alignTop', label: t.alignTop, command: 'align.lastTop', short: 'T' },
        {
          id: 'alignBottom',
          label: t.alignBottom,
          command: 'align.lastBottom',
          short: 'B',
        },
        {
          id: 'alignCenter',
          label: t.alignCenter,
          command: 'align.lastCenter',
          short: 'C',
          rowStart: true,
        },
        {
          id: 'alignHCenter',
          label: t.alignHCenter,
          command: 'align.lastHCenter',
          short: 'CX',
        },
        {
          id: 'alignVCenter',
          label: t.alignVCenter,
          command: 'align.lastVCenter',
          short: 'CY',
        },
        {
          id: 'alignOutsideLeft',
          label: t.alignOutsideLeft,
          command: 'align.outsideLeft',
          short: 'OL',
          rowStart: true,
        },
        {
          id: 'alignOutsideRight',
          label: t.alignOutsideRight,
          command: 'align.outsideRight',
          short: 'OR',
        },
        {
          id: 'alignOutsideTop',
          label: t.alignOutsideTop,
          command: 'align.outsideTop',
          short: 'OT',
        },
        {
          id: 'alignOutsideBottom',
          label: t.alignOutsideBottom,
          command: 'align.outsideBottom',
          short: 'OB',
        },
      ],
    },
    {
      id: 'offsetAndLayout',
      title: strings.leftPanel.groups.offsetAndLayout,
      tools: [
        { id: 'offset', label: t.offset, form: 'offset', short: 'Offs' },
        { id: 'arrayCopy', label: t.arrayCopy, form: 'arrayCopy', short: 'Arr' },
        { id: 'circularArray', label: t.circularArray, form: 'circularArray', short: 'Circ' },
        { id: 'copyAlong', label: t.copyAlong, form: 'copyAlong', short: 'Along' },
        { id: 'plates', label: t.plates, form: 'plates', short: 'Plate' },
        { id: 'nest', label: t.nest, form: 'nest', short: 'Nest' },
      ],
    },
    {
      id: 'joinery',
      title: strings.leftPanel.groups.joinery,
      tools: [{ id: 'jointCreator', label: t.jointCreator, form: 'jointCreator', short: 'Joint' }],
    },
    {
      id: 'modelingTools',
      title: strings.leftPanel.groups.modelingTools,
      tools: [
        {
          id: 'createFlatShape',
          label: t.createFlatShape,
          form: 'createFlatShape',
          shiftCommand: 'model.addZeroPlane',
          short: 'Flat',
        },
        { id: 'createShape', label: t.createShape, form: 'createShape', short: 'Round' },
        {
          id: 'createAngledShape',
          label: t.createAngledShape,
          form: 'createAngledShape',
          short: 'Angle',
        },
        {
          id: 'createSmoothShape',
          label: t.createSmoothShape,
          form: 'createSmoothShape',
          short: 'Smth',
        },
        {
          id: 'createCustomShape',
          label: t.createCustomShape,
          form: 'createCustomShape',
          short: 'Cust',
        },
        {
          id: 'twoRailSweep',
          label: t.twoRailSweep,
          form: 'twoRailSweep',
          short: 'Sweep',
          rowStart: true,
        },
        { id: 'extrudeWeave', label: t.extrudeWeave, form: 'extrudeWeave', short: 'Extr' },
        { id: 'turnSpin', label: t.turnSpin, form: 'turnSpin', short: 'Turn' },
        { id: 'textureArea', label: t.textureArea, form: 'textureArea', short: 'Tex' },
        { id: 'sculpt', label: t.sculpt, form: 'sculpt', short: 'Sculpt' },
        {
          id: 'clearInside',
          label: t.clearInside,
          command: 'model.resetInside',
          short: 'ClrIn',
          rowStart: true,
        },
        {
          id: 'clearOutside',
          label: t.clearOutside,
          command: 'model.resetOutside',
          short: 'ClrOut',
        },
        { id: 'splitComponent', label: t.splitComponent, command: 'model.split', short: 'Split' },
        { id: 'replaceBelow', label: t.replaceBelow, form: 'replaceBelow', short: 'RBelow' },
        { id: 'sliceModel', label: t.sliceModel, form: 'sliceModel', short: 'Slice' },
        {
          id: 'componentBoundary',
          label: t.componentBoundary,
          command: 'model.boundary',
          short: 'Bound',
          rowStart: true,
        },
        { id: 'crossSection', label: t.crossSection, form: 'crossSection', short: 'XSec' },
        { id: 'bakeComponents', label: t.bakeComponents, command: 'model.bake', short: 'Bake' },
        { id: 'fromVisible', label: t.fromVisible, command: 'model.fromVisible', short: 'Vis' },
        {
          id: 'smoothComponents',
          label: t.smoothComponents,
          form: 'smoothComponents',
          short: 'Smooth',
          rowStart: true,
        },
        {
          id: 'embossComponent',
          label: t.embossComponent,
          form: 'embossComponent',
          short: 'Emboss',
        },
        {
          id: 'scaleModelHeight',
          label: t.scaleModelHeight,
          form: 'scaleModelHeight',
          short: 'ScaleZ',
        },
        { id: 'addDraft', label: t.addDraft, form: 'addDraft', short: 'Draft' },
        { id: 'offsetModel', label: t.offsetModel, form: 'offsetModel', short: 'Offs3D' },
      ],
    },
  ],
  toolpaths: [
    {
      id: 'toolpathOperations',
      title: strings.leftPanel.groups.toolpathOperations,
      tools: [
        { id: 'profile', label: t.profile, form: 'profileToolpath', short: 'Prof' },
        { id: 'pocket', label: t.pocket, form: 'pocketToolpath', short: 'Pock' },
        { id: 'drill', label: t.drill, form: 'drillToolpath', short: 'Drill' },
        { id: 'drillBank', label: t.drillBank, form: 'drillBankToolpath', short: 'Bank' },
        {
          id: 'threadMilling',
          label: t.threadMilling,
          form: 'threadMillingToolpath',
          short: 'Thread',
          rowStart: true,
        },
        {
          id: 'quickEngrave',
          label: t.quickEngrave,
          form: 'quickEngraveToolpath',
          short: 'Engr',
        },
        {
          id: 'straightInlay',
          label: t.straightInlay,
          form: 'straightInlayToolpath',
          short: 'Inlay',
        },
        {
          id: 'laserCutFill',
          label: t.laserCutFill,
          form: 'laserCutFillToolpath',
          short: 'LCut',
          rowStart: true,
        },
        {
          id: 'laserPicture',
          label: t.laserPicture,
          form: 'laserPictureToolpath',
          short: 'LPic',
        },
        {
          id: 'laserSketch',
          label: t.laserSketch,
          form: 'laserSketchToolpath',
          short: 'LSketch',
        },
        {
          id: 'vgroove',
          label: t.vgroove,
          form: 'vgrooveToolpath',
          short: 'VGrv',
          rowStart: true,
        },
        { id: 'photoCarve', label: t.photoCarve, form: 'photoCarveToolpath', short: 'Photo' },
        { id: 'sketchCarve', label: t.sketchCarve, form: 'sketchToolpath', short: 'Sketch' },
        { id: 'prismCarve', label: t.prismCarve, form: 'prismToolpath', short: 'Prism' },
        { id: 'inlay', label: t.inlay, form: 'inlayToolpath', short: 'VInlay' },
        {
          id: 'fluting',
          label: t.fluting,
          form: 'flutingToolpath',
          short: 'Flute',
          rowStart: true,
        },
        { id: 'texture', label: t.texture, form: 'textureToolpath', short: 'Textr' },
        { id: 'chamfer', label: t.chamfer, form: 'chamferToolpath', short: 'Chamf' },
        { id: 'moulding', label: t.moulding, form: 'mouldingToolpath', short: 'Mold' },
        {
          id: 'rough3d',
          label: t.rough3d,
          form: 'rough3dToolpath',
          short: 'Rough',
          rowStart: true,
        },
        { id: 'finish3d', label: t.finish3d, form: 'finish3dToolpath', short: 'Finish' },
        {
          id: 'toolDatabase',
          label: strings.toolDatabase.title,
          command: 'toolpaths.toolDatabase',
          short: 'Tools',
          rowStart: true,
        },
        { id: 'editToolpath', label: t.editToolpath, command: 'toolpaths.edit', short: 'Edit' },
        {
          id: 'duplicateToolpath',
          label: t.duplicateToolpath,
          command: 'toolpaths.duplicate',
          short: 'Dup',
        },
        {
          id: 'deleteToolpath',
          label: t.deleteToolpath,
          command: 'toolpaths.delete',
          short: 'Del',
        },
        {
          id: 'recalculateAll',
          label: t.recalculateAll,
          command: 'toolpaths.recalculateAll',
          short: 'Recalc',
        },
        {
          id: 'loadTemplate',
          label: t.loadTemplate,
          command: 'toolpaths.loadTemplate',
          short: 'LoadT',
          rowStart: true,
        },
        {
          id: 'saveTemplate',
          label: t.saveTemplate,
          command: 'toolpaths.saveTemplateSelected',
          short: 'SaveT',
        },
        {
          id: 'saveVisibleTemplate',
          label: t.saveVisibleTemplate,
          command: 'toolpaths.saveTemplateVisible',
          short: 'VisT',
        },
        {
          id: 'arrayCopyToolpath',
          label: t.arrayCopyToolpath,
          form: 'arrayCopyToolpath',
          short: 'Array',
        },
        {
          id: 'mergeToolpaths',
          label: t.mergeToolpaths,
          command: 'toolpaths.merge',
          short: 'Merge',
        },
        {
          id: 'preview',
          label: t.preview,
          form: 'previewToolpaths',
          short: 'Prev',
          rowStart: true,
        },
        {
          id: 'toolpathSummary',
          label: t.toolpathSummary,
          command: 'toolpaths.summary',
          short: 'Time',
        },
        { id: 'toolpathTiling', label: t.toolpathTiling, form: 'toolpathTiling', short: 'Tile' },
        { id: 'keepOutZones', label: t.keepOutZones, short: 'Keep' },
        { id: 'jobSheet', label: t.jobSheet, command: 'toolpaths.jobSheet', short: 'Sheet' },
        { id: 'saveToolpaths', label: t.saveToolpaths, form: 'saveToolpaths', short: 'Save' },
        {
          id: 'assemblyView',
          label: strings.formTitles.assemblyView.title,
          form: 'assemblyView',
          short: 'Asm',
          rowStart: true,
        },
      ],
    },
  ],
};
