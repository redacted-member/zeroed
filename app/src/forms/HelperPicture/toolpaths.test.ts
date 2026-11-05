import { createElement, type ComponentType } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { ArrayCopyToolpathForm } from '../ArrayCopyToolpathForm/ArrayCopyToolpathForm';
import { ChamferToolpathForm } from '../ChamferToolpathForm/ChamferToolpathForm';
import { DrillBankToolpathForm } from '../DrillBankToolpathForm/DrillBankToolpathForm';
import { DrillToolpathForm } from '../DrillToolpathForm/DrillToolpathForm';
import { FlutingToolpathForm } from '../FlutingToolpathForm/FlutingToolpathForm';
import { InlayProfileForm } from '../InlayProfileForm/InlayProfileForm';
import { InlayPocketForm } from '../InlayPocketForm/InlayPocketForm';
import { InlayToolpathForm } from '../InlayToolpathForm/InlayToolpathForm';
import { MouldingToolpathForm } from '../MouldingToolpathForm/MouldingToolpathForm';
import { PhotoCarveToolpathForm } from '../PhotoCarveToolpathForm/PhotoCarveToolpathForm';
import { PocketToolpathForm } from '../PocketToolpathForm/PocketToolpathForm';
import { PrismToolpathForm } from '../PrismToolpathForm/PrismToolpathForm';
import { ProfileOptionPages } from '../ProfileOptionPages/ProfileOptionPages';
import { initialProfileState } from '../ProfileToolpathForm/model';
import { ProfileToolpathForm } from '../ProfileToolpathForm/ProfileToolpathForm';
import { QuickEngraveToolpathForm } from '../QuickEngraveToolpathForm/QuickEngraveToolpathForm';
import { SketchToolpathForm } from '../SketchToolpathForm/SketchToolpathForm';
import { TextureToolpathForm } from '../TextureToolpathForm/TextureToolpathForm';
import { ThreadMillingToolpathForm } from '../ThreadMillingToolpathForm/ThreadMillingToolpathForm';
import { Finish3dToolpathForm, Rough3dToolpathForm } from '../Toolpath3dForm/Toolpath3dForm';
import { ToolpathTilingForm } from '../ToolpathTilingForm/ToolpathTilingForm';
import { VGrooveToolpathForm } from '../VGrooveToolpathForm/VGrooveToolpathForm';
import { HelperPicture } from './HelperPicture';
import type { Picture } from './kit';
import {
  boundary3dPicture,
  clearancePicture,
  cornersPicture,
  engravePicture,
  flutePicture,
  leadPicture,
  machineVectorsPicture,
  peckPicture,
  photoPicture,
  prismPicture,
  rampPicture,
  strategy3dPicture,
  tabsPicture,
  threadPicture,
  vgrooveDepthPicture,
} from './pictures/toolpaths';

/** The data-helper ids a rendered form shows, in order. */
function helpers(Form: ComponentType): string[] {
  const html = renderToStaticMarkup(createElement(Form));
  return [...html.matchAll(/data-helper="([^"]+)"/g)].map((match) => match[1] ?? '');
}

/** Each option gets its own picture with its own description. */
function expectDistinct(pictures: Picture[]): void {
  expect(new Set(pictures.map((picture) => picture.id)).size).toBe(pictures.length);
  expect(new Set(pictures.map((picture) => picture.alt)).size).toBe(pictures.length);
  for (const picture of pictures) {
    expect(picture.alt.length).toBeGreaterThan(10);
  }
}

describe('HelperPicture', () => {
  it('is an image named by its description', () => {
    const html = renderToStaticMarkup(
      createElement(HelperPicture, { picture: machineVectorsPicture('inside', 'climb') }),
    );
    expect(html).toContain('role="img"');
    expect(html).toContain('data-helper="machine-vectors-inside-climb"');
    expect(html).toMatch(/aria-labelledby="([^"]+)".*<title id="\1">Machine vectors inside/);
  });
});

describe('toolpath form helpers', () => {
  it('each form shows its pictures in their place', () => {
    expect(helpers(ProfileToolpathForm)).toEqual([
      'cutting-depths',
      'machine-vectors-outside-climb',
      'last-pass',
      'tabs-2d',
      'ramp-smooth',
    ]);
    expect(helpers(PocketToolpathForm)).toEqual(['cutting-depths', 'clearance-offset']);
    expect(helpers(DrillToolpathForm)).toEqual(['cutting-depths', 'drill-straight']);
    expect(helpers(DrillBankToolpathForm)).toEqual(['cutting-depths', 'drill-straight']);
    expect(helpers(VGrooveToolpathForm)).toEqual(['vgroove-depth']);
    expect(helpers(SketchToolpathForm)).toEqual(['vgroove-depth']);
    expect(helpers(PrismToolpathForm)).toEqual(['prism-outside']);
    expect(helpers(QuickEngraveToolpathForm)).toEqual(['engrave-outline']);
    expect(helpers(ChamferToolpathForm)).toEqual(['chamfer']);
    expect(helpers(ThreadMillingToolpathForm)).toEqual(['thread-internal']);
    expect(helpers(FlutingToolpathForm)).toEqual(['flute-complete']);
    expect(helpers(MouldingToolpathForm)).toEqual(['moulding-position']);
    expect(helpers(TextureToolpathForm)).toEqual(['texture-pattern']);
    expect(helpers(PhotoCarveToolpathForm)).toEqual(['photo-raster']);
    expect(helpers(Rough3dToolpathForm)).toEqual(['boundary-model', 'strategy-zLevelRaster']);
    expect(helpers(Finish3dToolpathForm)).toEqual(['boundary-model', 'strategy-offset']);
    expect(helpers(InlayToolpathForm)).toEqual(['inlay-depths']);
    expect(helpers(InlayPocketForm)).toEqual(['cutting-depths', 'clearance-offset']);
    expect(helpers(ArrayCopyToolpathForm)).toEqual(['toolpath-grid']);
    expect(helpers(ToolpathTilingForm)).toEqual(['toolpath-tiling']);
  });

  it('the inlay insert and hole show the side they cut', () => {
    const insert = () => createElement(InlayProfileForm, { kind: 'maleStepped' });
    const hole = () => createElement(InlayProfileForm, { kind: 'femaleHole' });
    expect(helpers(insert)).toContain('machine-vectors-outside-climb');
    expect(helpers(hole)).toContain('machine-vectors-inside-climb');
    expect(helpers(hole)).not.toContain('machine-vectors-outside-climb');
  });

  it('the option pages switch picture with the option picked', () => {
    const base = { ...initialProfileState('mm'), tabTriangular: true, rampKind: 'zigZag' as const };
    const html = renderToStaticMarkup(
      createElement(ProfileOptionPages, {
        state: base,
        onChange: () => undefined,
        errors: {},
        units: 'mm',
        onEditTabs: () => undefined,
      }),
    );
    expect(html).toContain('data-helper="tabs-3d"');
    expect(html).toContain('data-helper="ramp-zigZag"');
    expect(html).not.toContain('data-helper="ramp-smooth"');
  });

  it('every reactive picture has one picture per option', () => {
    expectDistinct([
      machineVectorsPicture('outside', 'climb'),
      machineVectorsPicture('outside', 'conventional'),
      machineVectorsPicture('inside', 'climb'),
      machineVectorsPicture('inside', 'conventional'),
      machineVectorsPicture('on', 'climb'),
      machineVectorsPicture('on', 'conventional'),
    ]);
    expectDistinct([rampPicture('smooth'), rampPicture('zigZag'), rampPicture('spiral')]);
    expectDistinct([leadPicture('none'), leadPicture('straight'), leadPicture('arc')]);
    expectDistinct([cornersPicture(false), cornersPicture(true)]);
    expectDistinct([tabsPicture(false), tabsPicture(true)]);
    expectDistinct([clearancePicture('offset'), clearancePicture('raster')]);
    expectDistinct([peckPicture(false), peckPicture(true)]);
    expectDistinct([vgrooveDepthPicture(false), vgrooveDepthPicture(true)]);
    expectDistinct([prismPicture('outside'), prismPicture('inside')]);
    expectDistinct([engravePicture('outline'), engravePicture('fill')]);
    expectDistinct([threadPicture('internal'), threadPicture('external')]);
    expectDistinct([flutePicture('complete'), flutePicture('start'), flutePicture('startEnd')]);
    expectDistinct([photoPicture('raster'), photoPicture('hatch'), photoPicture('vectors')]);
    expectDistinct([
      strategy3dPicture('zLevelRaster'),
      strategy3dPicture('zLevelOffset'),
      strategy3dPicture('raster3d'),
      strategy3dPicture('raster'),
      strategy3dPicture('rasterXY'),
      strategy3dPicture('offset'),
    ]);
    expectDistinct([
      boundary3dPicture('model'),
      boundary3dPicture('material'),
      boundary3dPicture('selection'),
      boundary3dPicture('level'),
    ]);
  });

  it('the travel arrow points the way the tool runs', () => {
    // Climb runs clockwise outside: the arrow on the top edge points right
    // (+x); inside it runs counterclockwise, so it points left.
    const tip = (picture: Picture) => {
      const html = renderToStaticMarkup(createElement(HelperPicture, { picture }));
      const heads = [
        ...html.matchAll(/<polygon class="helper-picture__path-head" points="([^"]+)"/g),
      ];
      const points = (heads[0]?.[1] ?? '').split(' ').map((pair) => Number(pair.split(',')[0]));
      return Math.sign((points[0] ?? 0) - (points[1] ?? 0));
    };
    expect(tip(machineVectorsPicture('outside', 'climb'))).toBe(1);
    expect(tip(machineVectorsPicture('outside', 'conventional'))).toBe(-1);
    expect(tip(machineVectorsPicture('inside', 'climb'))).toBe(-1);
    expect(tip(machineVectorsPicture('inside', 'conventional'))).toBe(1);
  });
});
