import { describe, expect, it } from 'vitest';

import { AddDraftForm } from '../AddDraftForm/AddDraftForm';
import {
  CreateAngledShapeForm,
  CreateCustomShapeForm,
  CreateFlatShapeForm,
  CreateShapeForm,
  CreateSmoothShapeForm,
} from '../CreateShapeForm/CreateShapeForm';
import { ExtrudeWeaveForm } from '../ExtrudeWeaveForm/ExtrudeWeaveForm';
import { OffsetModelForm } from '../OffsetModelForm/OffsetModelForm';
import { PositionModelForm } from '../PositionModelForm/PositionModelForm';
import { ReplaceBelowForm } from '../ReplaceBelowForm/ReplaceBelowForm';
import { ScaleModelHeightForm } from '../ScaleModelHeightForm/ScaleModelHeightForm';
import { SculptForm } from '../SculptForm/SculptForm';
import { SliceModelForm } from '../SliceModelForm/SliceModelForm';
import { TurnSpinForm } from '../TurnSpinForm/TurnSpinForm';
import { TwoRailSweepForm } from '../TwoRailSweepForm/TwoRailSweepForm';
import {
  combinePicture,
  sculptPicture,
  shapeProfilePicture,
  turnSpinPicture,
  weavePicture,
} from './pictures/modeling';
import { distinct, renderedHelpers } from './renderHelpers';

const COMBINES = ['combine-add', 'combine-subtract', 'combine-mergeHigh'];

describe('modeling form helpers', () => {
  it('the shape tools show their cross section and the combine glyphs', () => {
    expect(renderedHelpers(CreateShapeForm)).toEqual(['shape-round', 'shape-heights', ...COMBINES]);
    expect(renderedHelpers(CreateAngledShapeForm)).toEqual([
      'shape-angled',
      'shape-heights',
      ...COMBINES,
    ]);
    expect(renderedHelpers(CreateSmoothShapeForm)).toEqual([
      'shape-smooth',
      'shape-heights',
      ...COMBINES,
    ]);
    expect(renderedHelpers(CreateCustomShapeForm)).toEqual([
      'shape-custom',
      'shape-heights',
      ...COMBINES,
    ]);
    // Flat has no profile settings: its section shows beside the heights.
    expect(renderedHelpers(CreateFlatShapeForm)).toEqual(['shape-flat', ...COMBINES]);
  });

  it('the other modeling tools show their picture', () => {
    expect(renderedHelpers(TurnSpinForm)).toEqual(['turn-spin-turn', ...COMBINES]);
    expect(renderedHelpers(ExtrudeWeaveForm)).toEqual(['weave-off', ...COMBINES]);
    expect(renderedHelpers(TwoRailSweepForm)).toEqual(['two-rail-sweep', ...COMBINES]);
    expect(renderedHelpers(SculptForm)).toEqual(['sculpt-normal', 'sculpt-raise', 'sculpt-lower']);
    expect(renderedHelpers(AddDraftForm)).toEqual(['draft']);
    expect(renderedHelpers(ReplaceBelowForm)).toEqual(['replace-below']);
    expect(renderedHelpers(OffsetModelForm)).toEqual(['offset-model']);
    expect(renderedHelpers(ScaleModelHeightForm)).toEqual(['scale-height']);
    expect(renderedHelpers(SliceModelForm)).toEqual(['slice-model']);
    expect(renderedHelpers(PositionModelForm)).toEqual(['zero-plane']);
  });

  it('every reactive picture has one picture per option', () => {
    const groups = [
      (['add', 'subtract', 'mergeHigh', 'mergeLow', 'multiply'] as const).map(combinePicture),
      (['round', 'flat', 'angled', 'smooth', 'custom'] as const).map(shapeProfilePicture),
      (['normal', 'raise', 'lower'] as const).map(sculptPicture),
      [turnSpinPicture('turn'), turnSpinPicture('spin')],
      [weavePicture(false), weavePicture(true)],
    ];
    for (const group of groups) {
      const { ids, alts, count } = distinct(group);
      expect(ids).toBe(count);
      expect(alts).toBe(count);
    }
  });
});
