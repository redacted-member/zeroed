import { describe, expect, it } from 'vitest';

import { createElement } from 'react';

import { DEFAULT_JOB } from '../../bridge/types';
import { MaterialSummaryPictures } from '../../shell/MaterialSummaryPictures/MaterialSummaryPictures';
import { JobSetupForm } from '../JobSetupForm/JobSetupForm';
import { MaterialSetupForm } from '../MaterialSetupForm/MaterialSetupForm';
import {
  flipPicture,
  jobTypePicture,
  rotaryAxisPicture,
  rotaryZZeroPicture,
  xyDatumPicture,
  zZeroPicture,
} from './pictures/job';
import { distinct, pictureMarkup, renderedHelpers } from './renderHelpers';

describe('Job Setup and setup dialog helpers', () => {
  it('each section shows its picture at its left', () => {
    expect(renderedHelpers(JobSetupForm)).toEqual([
      'job-type-singleSided',
      'job-size',
      'z-zero-top',
      'xy-datum-bottomLeft',
      'modeling-resolution',
    ]);
    expect(renderedHelpers(MaterialSetupForm)).toEqual([
      'z-zero-top',
      'model-position',
      'rapid-gaps',
      'home-position',
    ]);
    const summary = (side: 'left' | 'right') => () =>
      createElement(MaterialSummaryPictures, { job: DEFAULT_JOB, side });
    expect(renderedHelpers(summary('left'))).toEqual(['z-zero-top']);
    expect(renderedHelpers(summary('right'))).toEqual(['xy-datum-bottomLeft']);
    const rotary = { ...DEFAULT_JOB, kind: 'rotary' as const };
    const rotarySummary = (side: 'left' | 'right') => () =>
      createElement(MaterialSummaryPictures, { job: rotary, side });
    expect(renderedHelpers(rotarySummary('left'))).toEqual(['rotary-z-zero-surface']);
    expect(renderedHelpers(rotarySummary('right'))).toEqual([]);
  });

  it('every reactive picture has one picture per option', () => {
    const groups = [
      [jobTypePicture('singleSided'), jobTypePicture('doubleSided'), jobTypePicture('rotary')],
      [zZeroPicture('top'), zZeroPicture('bottom')],
      [rotaryZZeroPicture('surface'), rotaryZZeroPicture('centre')],
      [rotaryAxisPicture('x'), rotaryAxisPicture('y')],
      [flipPicture('horizontal'), flipPicture('vertical')],
      [
        xyDatumPicture('bottomLeft'),
        xyDatumPicture('bottomRight'),
        xyDatumPicture('topLeft'),
        xyDatumPicture('topRight'),
        xyDatumPicture('centre'),
      ],
    ];
    for (const group of groups) {
      const { ids, alts, count } = distinct(group);
      expect(ids).toBe(count);
      expect(alts).toBe(count);
    }
  });

  it('the datum dot sits at the picked corner of the material', () => {
    // The material spans x 8 to 56 and y 8 to 40 (y down): bottom left is
    // (8, 40), top right (56, 8), the center (32, 24).
    const dot = (markup: string) => {
      const match = /class="helper-picture__dot" cx="([\d.]+)" cy="([\d.]+)"/.exec(markup);
      return [Number(match?.[1]), Number(match?.[2])];
    };
    expect(dot(pictureMarkup(xyDatumPicture('bottomLeft')))).toEqual([8, 40]);
    expect(dot(pictureMarkup(xyDatumPicture('topRight')))).toEqual([56, 8]);
    expect(dot(pictureMarkup(xyDatumPicture('centre')))).toEqual([32, 24]);
  });

  it('the Z zero plane is on the top face or the bottom face', () => {
    // The block front face runs from y 20 (top) to y 38 (bottom).
    const planeY = (markup: string) =>
      Number(/class="helper-picture__plane" points="[\d.]+,([\d.]+)/.exec(markup)?.[1]);
    expect(planeY(pictureMarkup(zZeroPicture('top')))).toBe(20);
    expect(planeY(pictureMarkup(zZeroPicture('bottom')))).toBe(38);
  });
});
