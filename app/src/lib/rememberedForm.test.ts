import { describe, expect, it } from 'vitest';

import {
  convertLaserCutFillUnits,
  initialLaserCutFillState,
  type LaserCutFillState,
} from '../forms/LaserCutFillForm/model';
import {
  convertLaserPictureUnits,
  initialLaserPictureState,
} from '../forms/LaserPictureForm/model';
import { startFrom } from './rememberedForm';

describe('remembered form fields', () => {
  it('starts a laser Cut and Fill form as the last one calculated, in today units', () => {
    const first = startFrom(
      null,
      'mm',
      initialLaserCutFillState,
      convertLaserCutFillUnits,
      (s) => s,
    );
    expect(first).toEqual(initialLaserCutFillState('mm'));
    const last: LaserCutFillState = {
      ...first,
      name: 'Laser Cut 1',
      strategy: 'hatch',
      hatch: 'cross',
      power: '60',
      speed: '1500',
      lineInterval: '0.2',
      angle: '45',
    };
    const fresh = (s: LaserCutFillState) => ({ ...s, name: '' });
    const next = startFrom(
      { state: last, units: 'mm' },
      'mm',
      initialLaserCutFillState,
      convertLaserCutFillUnits,
      fresh,
    );
    expect(next).toEqual({ ...last, name: '' });
    const inches = startFrom(
      { state: last, units: 'mm' },
      'inch',
      initialLaserCutFillState,
      convertLaserCutFillUnits,
      fresh,
    );
    expect(inches.power).toBe('60');
    expect(inches.speed).toBe('59.0551');
    expect(inches.strategy).toBe('hatch');
  });

  it('starts a Laser Picture form as the last one, without an older file', () => {
    const last = {
      ...initialLaserPictureState('mm'),
      name: 'Laser Picture 1',
      lineSpacing: '0.3',
      speed: '3000',
      overscan: '5',
    };
    const next = startFrom(
      { state: last, units: 'mm' },
      'mm',
      initialLaserPictureState,
      convertLaserPictureUnits,
      (s) => ({ ...s, name: '', legacy: null }),
    );
    expect(next).toEqual({ ...last, name: '' });
  });
});
