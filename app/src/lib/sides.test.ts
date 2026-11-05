import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB, type JobSetup } from '../bridge/types';
import {
  doubleSidedOf,
  isDoubleSided,
  sideRequest,
  toolpathSideTag,
  DEFAULT_DOUBLE_SIDED,
} from './sides';
import { strings } from './strings';

const twoSided: JobSetup = { ...DEFAULT_JOB, kind: 'doubleSided' };

describe('sides', () => {
  it('knows a double sided job', () => {
    expect(isDoubleSided(DEFAULT_JOB)).toBe(false);
    expect(isDoubleSided(twoSided)).toBe(true);
  });

  it('fills in settings an older kernel leaves out', () => {
    const older: JobSetup = { ...twoSided };
    delete older.doubleSided;
    expect(doubleSidedOf(older)).toEqual(DEFAULT_DOUBLE_SIDED);
    const vertical = { ...twoSided, doubleSided: { flip: 'vertical', bottomZZero: 'bottom' } };
    expect(doubleSidedOf(vertical as JobSetup).flip).toBe('vertical');
  });

  it('tags toolpaths with their side only on double sided jobs', () => {
    expect(toolpathSideTag('bottom', DEFAULT_JOB)).toBeNull();
    expect(toolpathSideTag('bottom', twoSided)).toBe(strings.sides.bottom);
    expect(toolpathSideTag(undefined, twoSided)).toBe(strings.sides.top);
  });

  it('switches sides only when there is another side to go to', () => {
    expect(sideRequest(DEFAULT_JOB, 'top', 'bottom')).toBeNull();
    expect(sideRequest(twoSided, 'top', 'top')).toBeNull();
    expect(sideRequest(twoSided, 'top', 'bottom')).toBe('bottom');
    expect(sideRequest(twoSided, 'bottom', 'top')).toBe('top');
  });
});
