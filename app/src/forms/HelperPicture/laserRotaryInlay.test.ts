import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { Tool, ToolKind } from '../../bridge/cam';
import { ToolEditor } from '../../panels/ToolEditor/ToolEditor';
import { LaserCutFillForm } from '../LaserCutFillForm/LaserCutFillForm';
import { LaserPictureForm } from '../LaserPictureForm/LaserPictureForm';
import { LaserSketchForm } from '../LaserSketchForm/LaserSketchForm';
import { RotaryImportFields } from '../RotaryImportFields/RotaryImportFields';
import { StraightInlayToolpathForm } from '../StraightInlayToolpathForm/StraightInlayToolpathForm';
import { rotaryWrapPicture } from './pictures/modeling';
import {
  inlayKindPicture,
  laserCutPicture,
  laserPicturePicture,
  laserSketchPicture,
  toolPicture,
} from './pictures/toolpaths';
import { distinct, renderedHelpers } from './renderHelpers';

const KINDS: readonly ToolKind[] = [
  'endMill',
  'ballNose',
  'vBit',
  'engravingTool',
  'taperedBallNose',
  'radiusedEndMill',
  'formTool',
  'drill',
  'diamondDrag',
  'laserTool',
  'threadMill',
];

function tool(kind: ToolKind): Tool {
  return {
    id: 1,
    name: 'Tool 1',
    number: 1,
    diameter: 6,
    toolType: { kind, diameter: 6 },
    passDepth: 3,
    stepover: 2.4,
    stepoverPercent: 40,
    feedRate: 1000,
    plungeRate: 300,
    feedUnits: 'mm/min',
    spindleSpeed: 18000,
    notes: '',
    units: 'mm',
    clearance: 0,
    cuttingData: [],
  };
}

function editorHelpers(kind: ToolKind): string[] {
  const html = renderToStaticMarkup(
    createElement(ToolEditor, {
      tool: tool(kind),
      context: { machine: '', material: '' },
      formats: null,
      onSave: () => undefined,
      onFormatsChanged: () => undefined,
    }),
  );
  return [...html.matchAll(/data-helper="([^"]+)"/g)].map((match) => match[1] ?? '');
}

function expectDistinct(...pictures: Parameters<typeof distinct>[0]): void {
  const { ids, alts, count } = distinct(pictures);
  expect(ids).toBe(count);
  expect(alts).toBe(count);
}

describe('laser, rotary, inlay and tool helpers', () => {
  it('the laser forms show their strategy picture', () => {
    expect(renderedHelpers(LaserCutFillForm)).toEqual(['laser-outside']);
    expect(renderedHelpers(LaserPictureForm)).toEqual(['laser-picture-raster']);
    expect(renderedHelpers(LaserSketchForm)).toEqual(['laser-sketch-hatch']);
  });

  it('the inlay chooser shows a picture beside each kind', () => {
    expect(renderedHelpers(StraightInlayToolpathForm)).toEqual([
      'inlay-maleStraight',
      'inlay-maleStepped',
      'inlay-femalePocket',
      'inlay-femaleHole',
    ]);
  });

  it('the rotary import shows whether the model wraps round the axis', () => {
    const fields = () => createElement(RotaryImportFields, { units: 'mm' });
    expect(renderedHelpers(fields)).toEqual(['rotary-wrap-off']);
  });

  it('the tool editor draws the tool kind being edited', () => {
    expect(editorHelpers('vBit')).toEqual(['tool-vBit']);
    expect(editorHelpers('ballNose')).toEqual(['tool-ballNose']);
  });

  it('every reactive picture has one picture per option', () => {
    expectDistinct(...KINDS.map(toolPicture));
    expectDistinct(
      inlayKindPicture('maleStraight'),
      inlayKindPicture('maleStepped'),
      inlayKindPicture('femalePocket'),
      inlayKindPicture('femaleHole'),
    );
    expectDistinct(
      laserCutPicture('outside'),
      laserCutPicture('inside'),
      laserCutPicture('on'),
      laserCutPicture('hatch'),
    );
    expectDistinct(
      laserPicturePicture('raster'),
      laserPicturePicture('hatch'),
      laserPicturePicture('vectors'),
    );
    expectDistinct(laserSketchPicture(false), laserSketchPicture(true));
    expectDistinct(rotaryWrapPicture(false), rotaryWrapPicture(true));
  });
});
