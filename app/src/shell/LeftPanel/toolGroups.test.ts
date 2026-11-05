import { describe, expect, it } from 'vitest';

import { TOOL_GROUPS, visibleGroups, type DesignOption, type ToolGroup } from './toolGroups';

/** A group's tool ids, one array per row of its grid. */
function rows(group: ToolGroup | undefined): string[][] {
  const out: string[][] = [];
  for (const tool of group?.tools ?? []) {
    const last = out[out.length - 1];
    if (last === undefined || tool.rowStart === true) {
      out.push([tool.id]);
    } else {
      last.push(tool.id);
    }
  }
  return out;
}

function group(id: string): ToolGroup | undefined {
  return TOOL_GROUPS.design.find((g) => g.id === id);
}

describe('tool groups', () => {
  it('has the groups in order', () => {
    expect(TOOL_GROUPS.design.map((g) => g.title)).toEqual([
      'File Operations',
      'Create Vectors',
      'Transform Objects',
      'Edit Objects',
      'Align Objects',
      'Offset and Layout',
      'Joinery',
      'Modeling Tools',
    ]);
  });

  it('shows the optional rows and groups only while their option is on', () => {
    const titles = (on: DesignOption[]) =>
      visibleGroups(TOOL_GROUPS.design, new Set(on)).map((g) => g.id);
    const ids = (on: DesignOption[], id: string) =>
      rows(visibleGroups(TOOL_GROUPS.design, new Set(on)).find((g) => g.id === id));
    expect(titles([])).not.toContain('alignObjects');
    expect(titles(['alignTools'])).toEqual([
      'fileOperations',
      'createVectors',
      'transformObjects',
      'editObjects',
      'alignObjects',
      'offsetAndLayout',
      'joinery',
      'modelingTools',
    ]);
    expect(ids([], 'fileOperations')).toEqual([
      ['jobSetup', 'fileOpen', 'importVectors', 'importModel', 'bitmapRelief', 'exportModel'],
    ]);
    expect(ids(['fileOperations'], 'fileOperations')[0]).toEqual([
      'fileNew',
      'fileSave',
      'editUndo',
      'editRedo',
    ]);
    // Vector Unwrapper is for rotary jobs only.
    expect(ids([], 'transformObjects')).toEqual([
      ['move', 'setSize', 'rotate', 'mirror', 'distort', 'align'],
    ]);
    expect(ids(['rotaryJob'], 'transformObjects')[1]).toEqual(['vectorUnwrapper']);
  });

  it('lays out File Operations and Create Vectors in their rows', () => {
    expect(rows(group('fileOperations'))).toEqual([
      ['fileNew', 'fileSave', 'editUndo', 'editRedo'],
      ['jobSetup', 'fileOpen', 'importVectors', 'importModel', 'bitmapRelief', 'exportModel'],
    ]);
    expect(rows(group('createVectors'))).toEqual([
      ['drawCircle', 'drawEllipse', 'drawRectangle', 'drawPolygon', 'drawStar'],
      ['drawPolyline', 'drawArc', 'drawFreehand', 'vectorTexture', 'dimensions'],
      ['createText', 'textToCurves', 'textOnCurve', 'traceBitmap'],
    ]);
  });

  it('lays out Transform Objects, Edit Objects and Offset and Layout in their rows', () => {
    expect(rows(group('transformObjects'))).toEqual([
      ['move', 'setSize', 'rotate', 'mirror', 'distort', 'align'],
      ['vectorUnwrapper'],
    ]);
    expect(rows(group('editObjects'))).toEqual([
      ['vectorSelection', 'nodeEditing', 'interactiveSelection', 'group', 'ungroup', 'measure'],
      ['weld', 'subtract', 'intersect', 'trimObjects', 'trim', 'validator'],
      ['fillet', 'extend', 'fitCurves', 'boundary', 'editPicture', 'cropBitmap'],
      ['join', 'close', 'closeSmooth', 'closeMoveEnds'],
    ]);
    expect(rows(group('alignObjects'))).toEqual([
      ['alignMaterial', 'alignMaterialX', 'alignMaterialY'],
      ['alignLeft', 'alignRight', 'alignTop', 'alignBottom'],
      ['alignCenter', 'alignHCenter', 'alignVCenter'],
      ['alignOutsideLeft', 'alignOutsideRight', 'alignOutsideTop', 'alignOutsideBottom'],
    ]);
    expect(rows(group('offsetAndLayout'))).toEqual([
      ['offset', 'arrayCopy', 'circularArray', 'copyAlong', 'plates', 'nest'],
    ]);
  });

  it('lays out Modeling Tools as a 5 by 5 grid', () => {
    expect(rows(group('modelingTools'))).toEqual([
      [
        'createFlatShape',
        'createShape',
        'createAngledShape',
        'createSmoothShape',
        'createCustomShape',
      ],
      ['twoRailSweep', 'extrudeWeave', 'turnSpin', 'textureArea', 'sculpt'],
      ['clearInside', 'clearOutside', 'splitComponent', 'replaceBelow', 'sliceModel'],
      ['componentBoundary', 'crossSection', 'bakeComponents', 'fromVisible'],
      ['smoothComponents', 'embossComponent', 'scaleModelHeight', 'addDraft', 'offsetModel'],
    ]);
  });

  it('lays out the Toolpaths panel under one heading, rows of at most six', () => {
    expect(TOOL_GROUPS.toolpaths.map((g) => g.title)).toEqual(['Toolpath Operations']);
    expect(rows(TOOL_GROUPS.toolpaths[0])).toEqual([
      ['profile', 'pocket', 'drill', 'drillBank'],
      ['threadMilling', 'quickEngrave', 'straightInlay'],
      ['laserCutFill', 'laserPicture', 'laserSketch'],
      ['vgroove', 'photoCarve', 'sketchCarve', 'prismCarve', 'inlay'],
      ['fluting', 'texture', 'chamfer', 'moulding'],
      ['rough3d', 'finish3d'],
      ['toolDatabase', 'editToolpath', 'duplicateToolpath', 'deleteToolpath', 'recalculateAll'],
      [
        'loadTemplate',
        'saveTemplate',
        'saveVisibleTemplate',
        'arrayCopyToolpath',
        'mergeToolpaths',
      ],
      ['preview', 'toolpathSummary', 'toolpathTiling', 'keepOutZones', 'jobSheet', 'saveToolpaths'],
      ['assemblyView'],
    ]);
  });

  it('gives every tool one slot across the tool sets', () => {
    const ids = Object.values(TOOL_GROUPS)
      .flat()
      .flatMap((g) => g.tools.map((tool) => tool.id));
    expect(new Set(ids).size).toBe(ids.length);
  });
});
