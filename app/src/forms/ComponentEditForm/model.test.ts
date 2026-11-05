import { describe, expect, it } from 'vitest';

import {
  bboxCentre,
  convertEditUnits,
  editArgs,
  editHasSettings,
  EDIT_OPS,
  initialEditState,
  validateEdit,
  type Bbox,
} from './model';

const bbox: Bbox = [10, 20, 30, 60];
const mm = initialEditState('mm');

describe('component edit model', () => {
  it('builds every edit with the kernel defaults', () => {
    expect(EDIT_OPS.map((op) => editArgs(op, mm, 'mm', bbox))).toEqual([
      { op: 'smooth', params: { strength: 1, iterations: 1 } },
      { op: 'draft', params: { angleDeg: 10 } },
      { op: 'tilt', params: { angleDeg: 5, directionDeg: 0, pivot: { x: 20, y: 40 } } },
      { op: 'fade', params: { directionDeg: 0, startFraction: 0, endFraction: 1 } },
      { op: 'mirror', params: { horizontal: true } },
      { op: 'flip' },
      { op: 'scale', params: { scale: 1, baseOffset: 0 } },
      { op: 'emboss', params: { maxHeight: 2 } },
    ]);
  });

  it('applies flip at once and opens a form for the rest', () => {
    expect(EDIT_OPS.filter((op) => !editHasSettings(op))).toEqual(['flip']);
  });

  it('takes the tilt pivot from the fields when not centred', () => {
    const state = { ...mm, pivotAtCentre: false, pivotX: '1', pivotY: '2', tiltAngle: '-30' };
    expect(editArgs('tilt', state, 'inch', undefined)).toEqual({
      op: 'tilt',
      params: { angleDeg: -30, directionDeg: 0, pivot: { x: 25.4, y: 50.8 } },
    });
    expect(bboxCentre(bbox)).toEqual({ x: 20, y: 40 });
  });

  it('needs an extent to pivot at the centre', () => {
    expect(validateEdit('tilt', mm, 'mm', undefined).pivotX).toBeDefined();
    expect(editArgs('tilt', mm, 'mm', undefined)).toBeNull();
  });

  it('keeps angles strictly inside their ranges', () => {
    expect(validateEdit('draft', { ...mm, draftAngle: '0' }, 'mm', bbox).draftAngle).toBeDefined();
    expect(validateEdit('draft', { ...mm, draftAngle: '90' }, 'mm', bbox).draftAngle).toBeDefined();
    expect(validateEdit('tilt', { ...mm, tiltAngle: '90' }, 'mm', bbox).tiltAngle).toBeDefined();
    expect(validateEdit('tilt', { ...mm, tiltAngle: '-89' }, 'mm', bbox)).toEqual({});
  });

  it('checks smoothing strength and whole passes', () => {
    expect(validateEdit('smooth', { ...mm, strength: '0' }, 'mm', bbox).strength).toBeDefined();
    expect(
      validateEdit('smooth', { ...mm, iterations: '1.5' }, 'mm', bbox).iterations,
    ).toBeDefined();
    expect(
      validateEdit('smooth', { ...mm, iterations: '101' }, 'mm', bbox).iterations,
    ).toBeDefined();
  });

  it('sends fade limits as fractions with the end after the start', () => {
    const state = { ...mm, fadeDirection: '90', fadeStart: '25', fadeEnd: '75' };
    expect(editArgs('fade', state, 'mm', bbox)).toEqual({
      op: 'fade',
      params: { directionDeg: 90, startFraction: 0.25, endFraction: 0.75 },
    });
    expect(validateEdit('fade', { ...state, fadeEnd: '25' }, 'mm', bbox).fadeEnd).toBeDefined();
    expect(validateEdit('fade', { ...state, fadeEnd: '120' }, 'mm', bbox).fadeEnd).toBeDefined();
  });

  it('only checks the fields of the chosen edit', () => {
    const state = { ...mm, maxHeight: '0', scale: 'x' };
    expect(validateEdit('smooth', state, 'mm', bbox)).toEqual({});
    expect(validateEdit('emboss', state, 'mm', bbox).maxHeight).toBeDefined();
    expect(validateEdit('scale', state, 'mm', bbox).scale).toBeDefined();
  });

  it('sends lengths in millimetres and mirrors top to bottom when asked', () => {
    const inch = initialEditState('inch');
    expect(editArgs('emboss', { ...inch, maxHeight: '0.5' }, 'inch', bbox)).toEqual({
      op: 'emboss',
      params: { maxHeight: 12.7 },
    });
    expect(editArgs('scale', { ...inch, scale: '2', baseOffset: '1' }, 'inch', bbox)).toEqual({
      op: 'scale',
      params: { scale: 2, baseOffset: 25.4 },
    });
    expect(editArgs('mirror', { ...mm, mirrorHorizontal: false }, 'mm', bbox)).toEqual({
      op: 'mirror',
      params: { horizontal: false },
    });
    expect(convertEditUnits({ ...mm, strength: '25.4' }, 'mm', 'inch').strength).toBe('1');
  });
});
