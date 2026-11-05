import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB } from '../../bridge/types';
import {
  combinedThickness,
  tooThick,
  chooseGap,
  modelingPlaneZ,
  otherGap,
  surfaceZFor,
  withRotaryRows,
  machineFrom,
  materialStateFrom,
  sideZZero,
  thicknessValue,
  withMaterialRows,
  validateMaterial,
} from './model';

describe('material setup model', () => {
  it('keeps the model inside the material and links the two gaps', () => {
    const state = { ...materialStateFrom(DEFAULT_JOB.machine, 'mm'), modelGap: '3' };
    expect(machineFrom(state, 'mm', 18)?.modelGap).toBe(3);
    expect(validateMaterial({ ...state, modelGap: '20' }, 'mm', 18).modelGap).toBeDefined();
    expect(validateMaterial({ ...state, modelGap: '-1' }, 'mm', 18).modelGap).toBeDefined();
    expect(otherGap(3, 18, 5)).toBe(10);
    expect(otherGap(10, 18, 5)).toBe(3);
    expect(otherGap(30, 18, 5)).toBe(0);
  });

  it('switches the gap radio without moving the model', () => {
    const state = { ...materialStateFrom(DEFAULT_JOB.machine, 'mm'), modelGap: '3' };
    expect(state.gapBelow).toBe(false);
    const below = chooseGap(state, true, 'mm', 18, 5);
    expect(below).toMatchObject({ gapBelow: true, modelGap: '10' });
    expect(machineFrom(below, 'mm', 18)).toMatchObject({ modelGap: 10, gapBelow: true });
    expect(chooseGap(below, true, 'mm', 18, 5)).toBe(below);
    expect(chooseGap(below, false, 'mm', 18, 5).modelGap).toBe('3');
  });

  it('reads the modeling plane Z from the Z zero', () => {
    expect(modelingPlaneZ(surfaceZFor('top', 18), 8)).toBe(-8);
    expect(modelingPlaneZ(surfaceZFor('bottom', 18), 8)).toBe(10);
    expect(modelingPlaneZ(surfaceZFor('centre', 25), 5)).toBe(20);
    expect(modelingPlaneZ(surfaceZFor('surface', 25), 5)).toBe(-5);
  });

  it('sets a rotary job from its diameter and cylinder Z zero', () => {
    const rotary = { ...DEFAULT_JOB, kind: 'rotary' as const };
    const set = withRotaryRows(rotary, 60, 'centre');
    expect(set.rotary).toMatchObject({ diameter: 60, zZero: 'centre' });
    expect(set.thickness).toBe(30);
  });

  it('round trips the machine settings', () => {
    const state = materialStateFrom(DEFAULT_JOB.machine, 'inch');
    const back = machineFrom(state, 'inch');
    expect(back?.safeZ).toBeCloseTo(DEFAULT_JOB.machine.safeZ, 2);
    expect(back?.homeZ).toBeCloseTo(DEFAULT_JOB.machine.homeZ, 2);
  });

  it('keeps the plunge height at or below the clearance height', () => {
    const state = { ...materialStateFrom(DEFAULT_JOB.machine, 'mm'), plungeGap: '6' };
    expect(validateMaterial(state, 'mm').plungeGap).toBeDefined();
    expect(machineFrom(state, 'mm')).toBeNull();
  });

  it('measures the whole two sided model on a double sided job', () => {
    const two = { ...DEFAULT_JOB, kind: 'doubleSided' as const };
    expect(combinedThickness(5, 4, two)).toBe(9);
    expect(combinedThickness(5, 4, DEFAULT_JOB)).toBe(5);
    expect(otherGap(2, 20, combinedThickness(5, 4, two))).toBe(9);
  });

  it('flags a model thicker than the material', () => {
    expect(tooThick(21, 20)).toBe(true);
    expect(tooThick(20, 20)).toBe(false);
  });

  it('sets the thickness and the Z zero of the side being set up', () => {
    const flat = { ...DEFAULT_JOB, kind: 'singleSided' as const, zZero: 'top' as const };
    const set = withMaterialRows(flat, 'top', 25, 'bottom');
    expect(set).toMatchObject({ thickness: 25, zZero: 'bottom' });
    const two = {
      ...DEFAULT_JOB,
      kind: 'doubleSided' as const,
      zZero: 'top' as const,
      doubleSided: { flip: 'horizontal' as const, bottomZZero: 'top' as const },
    };
    const bottom = withMaterialRows(two, 'bottom', 20, 'bottom');
    expect(bottom.zZero).toBe('top');
    expect(bottom.doubleSided?.bottomZZero).toBe('bottom');
    expect(sideZZero(bottom, 'bottom')).toBe('bottom');
    expect(sideZZero(bottom, 'top')).toBe('top');
    expect(thicknessValue('0', 'mm')).toBeNull();
    expect(thicknessValue('1', 'inch')).toBeCloseTo(25.4);
  });
});
