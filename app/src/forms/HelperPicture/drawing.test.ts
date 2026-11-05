import { describe, expect, it } from 'vitest';

import { CircularArrayForm } from '../CircularArrayForm/CircularArrayForm';
import { CopyAlongForm } from '../CopyAlongForm/CopyAlongForm';
import { DistortForm } from '../DistortForm/DistortForm';
import { DrawArcForm } from '../DrawArcForm/DrawArcForm';
import { DrawPolygonForm } from '../DrawPolygonForm/DrawPolygonForm';
import { DrawRectangleForm } from '../DrawRectangleForm/DrawRectangleForm';
import { DrawStarForm } from '../DrawStarForm/DrawStarForm';
import { FilletForm } from '../FilletForm/FilletForm';
import { FILLET_KINDS } from '../FilletForm/model';
import { FitCurvesForm } from '../FitCurvesForm/FitCurvesForm';
import { NestForm } from '../NestForm/NestForm';
import { OffsetForm } from '../OffsetForm/OffsetForm';
import { RotateForm } from '../RotateForm/RotateForm';
import { SnapSettingsForm } from '../SnapSettingsForm/SnapSettingsForm';
import { TextOnCurveForm } from '../TextOnCurveForm/TextOnCurveForm';
import { TraceBitmapForm } from '../TraceBitmapForm/TraceBitmapForm';
import { VectorTextureForm } from '../VectorTextureForm/VectorTextureForm';
import { VectorUnwrapperForm } from '../VectorUnwrapperForm/VectorUnwrapperForm';
import type { Picture } from './kit';
import {
  arcModePicture,
  cornerPicture,
  distortPicture,
  filletPicture,
  fitPicture,
  nestCornerPicture,
  offsetPicture,
  polygonSizePicture,
  textOnCurvePicture,
  tracePicture,
  vectorTexturePicture,
} from './pictures/drawing';
import { distinct, pictureMarkup, renderedHelpers } from './renderHelpers';

function expectDistinct(pictures: Picture[]): void {
  const { ids, alts, count } = distinct(pictures);
  expect(ids).toBe(count);
  expect(alts).toBe(count);
}

describe('drawing and editing form helpers', () => {
  it('each form shows its picture beside the option it explains', () => {
    expect(renderedHelpers(DrawRectangleForm)).toEqual(['corner-none']);
    expect(renderedHelpers(DrawArcForm)).toEqual(['arc-threePoints']);
    expect(renderedHelpers(DrawPolygonForm)).toEqual(['polygon-radius']);
    expect(renderedHelpers(DrawStarForm)).toEqual(['star-radii']);
    expect(renderedHelpers(OffsetForm)).toEqual(['offset-outward-round']);
    expect(renderedHelpers(FilletForm)).toEqual(['fillet-normal']);
    expect(renderedHelpers(FitCurvesForm)).toEqual(['fit-arcs']);
    expect(renderedHelpers(NestForm)).toEqual(['nest-bottomLeft']);
    expect(renderedHelpers(DistortForm)).toEqual(['distort-box']);
    expect(renderedHelpers(VectorTextureForm)).toEqual(['vector-texture-straight']);
    expect(renderedHelpers(TraceBitmapForm)).toEqual(['trace-curves']);
    expect(renderedHelpers(TextOnCurveForm)).toEqual(['text-on-curve-above']);
    expect(renderedHelpers(RotateForm)).toEqual(['rotate-pivot']);
    expect(renderedHelpers(CircularArrayForm)).toEqual(['circular-array']);
    expect(renderedHelpers(CopyAlongForm)).toEqual(['copy-along']);
    expect(renderedHelpers(VectorUnwrapperForm)).toEqual(['vector-unwrap']);
  });

  it('every snap kind has its own icon beside its check box', () => {
    expect(renderedHelpers(SnapSettingsForm)).toEqual([
      'snap-nodes',
      'snap-midpoints',
      'snap-centres',
      'snap-intersections',
      'snap-jobPoints',
      'snap-guides',
      'snap-grid',
      'snap-smart',
    ]);
  });

  it('every reactive picture has one picture per option', () => {
    expectDistinct([cornerPicture('none'), cornerPicture('external'), cornerPicture('internal')]);
    expectDistinct([arcModePicture('threePoints'), arcModePicture('centreStartEnd')]);
    expectDistinct([polygonSizePicture('radius'), polygonSizePicture('diameter')]);
    expectDistinct([
      offsetPicture('outward', 'round'),
      offsetPicture('outward', 'sharp'),
      offsetPicture('inward', 'round'),
    ]);
    expectDistinct(FILLET_KINDS.map((kind) => filletPicture(kind)));
    expectDistinct([fitPicture('arcs'), fitPicture('beziers'), fitPicture('lines')]);
    expectDistinct([
      nestCornerPicture('bottomLeft'),
      nestCornerPicture('bottomRight'),
      nestCornerPicture('topLeft'),
      nestCornerPicture('topRight'),
    ]);
    expectDistinct([distortPicture('box'), distortPicture('curve'), distortPicture('twoCurves')]);
    expectDistinct([vectorTexturePicture(false), vectorTexturePicture(true)]);
    expectDistinct([tracePicture('lines'), tracePicture('curves')]);
    expectDistinct([
      textOnCurvePicture('above'),
      textOnCurvePicture('on'),
      textOnCurvePicture('below'),
    ]);
  });

  it('an inward offset keeps its corners whatever the corner option', () => {
    expect(offsetPicture('inward', 'sharp').id).toBe(offsetPicture('inward', 'round').id);
  });

  it('rounded corners bulge out and internal corners curve in', () => {
    // An arc from (46, 8) to (54, 16) bulges outward when drawn clockwise
    // (sweep flag 1) and curves into the rectangle when drawn the other way.
    expect(pictureMarkup(cornerPicture('external'))).toContain('A8 8 0 0 1 54 16');
    expect(pictureMarkup(cornerPicture('internal'))).toContain('A8 8 0 0 0 54 16');
  });
});
