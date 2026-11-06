import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB, type JobSetup } from '../../bridge/types';
import { applyScale } from '../../lib/designScale';
import { normaliseRotary } from '../../lib/rotary';
import {
  flipsDesign,
  designScaleOf,
  jobFromState,
  scaleSummary,
  stateFromJob,
  switchUnits,
  unwrappedLayout,
  validate,
  withMaterial,
  datumMarker,
  materialMoved,
} from './model';

describe('job setup form model', () => {
  it('fills the fields from a job in millimetres', () => {
    const state = stateFromJob(DEFAULT_JOB);
    expect(state.width).toBe('300');
    expect(state.thickness).toBe('18');
    expect(state.units).toBe('mm');
    expect(state.kind).toBe('singleSided');
  });

  it('shows inch values when the job is in inches', () => {
    const job: JobSetup = { ...DEFAULT_JOB, units: 'inch', width: 609.6, height: 304.8 };
    const state = stateFromJob(job);
    expect(state.width).toBe('24');
    expect(state.height).toBe('12');
  });

  it('converts what the fields hold when the unit changes', () => {
    const state = switchUnits(stateFromJob(DEFAULT_JOB), 'inch');
    expect(state.units).toBe('inch');
    expect(state.width).toBe('11.811');
    expect(switchUnits(state, 'mm').width).toBe('300');
  });

  it('keeps sizes nobody edited exact through the display rounding', () => {
    const job: JobSetup = { ...DEFAULT_JOB, units: 'inch', width: 600, height: 300 };
    const state = stateFromJob(job);
    expect(state.width).toBe('23.622');
    expect(jobFromState(state)?.width).toBe(600);
    const back = switchUnits(switchUnits(state, 'mm'), 'inch');
    expect(jobFromState(back)?.height).toBe(300);
    expect(jobFromState({ ...state, width: '23.622' })?.width).toBe(600);
    expect(jobFromState({ ...state, width: '24' })?.width).toBeCloseTo(609.6, 9);
  });

  it('leaves half typed text alone while switching units', () => {
    const state = { ...stateFromJob(DEFAULT_JOB), width: '' };
    expect(switchUnits(state, 'inch').width).toBe('');
  });

  it('reports fields that are not numbers', () => {
    const state = { ...stateFromJob(DEFAULT_JOB), width: 'wide', useOffset: true, offsetY: '' };
    const errors = validate(state);
    expect(errors.width).toBeDefined();
    expect(errors.offsetY).toBeDefined();
    expect(errors.height).toBeUndefined();
  });

  it('requires a positive size and thickness', () => {
    const errors = validate({ ...stateFromJob(DEFAULT_JOB), width: '0', thickness: '-2' });
    expect(errors.width).toBeDefined();
    expect(errors.thickness).toBeDefined();
  });

  it('allows a negative datum offset', () => {
    expect(validate({ ...stateFromJob(DEFAULT_JOB), offsetX: '-25' })).toEqual({});
  });

  it('only checks the blank diameter for a rotary job', () => {
    const flat = { ...stateFromJob(DEFAULT_JOB), rotaryDiameter: '0' };
    expect(validate(flat)).toEqual({});
    expect(validate({ ...flat, kind: 'rotary' as const }).rotaryDiameter).toBeDefined();
  });

  it('builds a job in millimetres from inch fields', () => {
    const state = switchUnits(stateFromJob(DEFAULT_JOB), 'inch');
    const job = jobFromState({ ...state, width: '24', height: '12', thickness: '0.75' });
    expect(job).not.toBeNull();
    expect(job?.width).toBeCloseTo(609.6, 9);
    expect(job?.thickness).toBeCloseTo(19.05, 9);
    expect(job?.units).toBe('inch');
  });

  it('refuses to build a job while a field is wrong', () => {
    expect(jobFromState({ ...stateFromJob(DEFAULT_JOB), height: 'tall' })).toBeNull();
  });

  it('keeps the machine heights it does not edit', () => {
    const job = {
      ...DEFAULT_JOB,
      machine: { safeZ: 12, plungeGap: 3, homeX: 5, homeY: 6, homeZ: 40, modelGap: 2 },
    };
    expect(jobFromState(stateFromJob(job))?.machine).toEqual(job.machine);
  });

  it('carries the flip direction and bottom Z zero of a double sided job', () => {
    const job: JobSetup = {
      ...DEFAULT_JOB,
      kind: 'doubleSided',
      doubleSided: { flip: 'vertical', bottomZZero: 'bottom' },
    };
    const state = stateFromJob(job);
    expect(state.flip).toBe('vertical');
    expect(state.bottomZZero).toBe('bottom');
    const back = jobFromState({ ...state, flip: 'horizontal' });
    expect(back?.doubleSided).toEqual({ flip: 'horizontal', bottomZZero: 'bottom' });
    expect(back?.zZero).toBe('top');
  });

  it('defaults the double sided settings when the kernel sends none', () => {
    const older: JobSetup = { ...DEFAULT_JOB };
    delete older.doubleSided;
    const state = stateFromJob(older);
    expect(state.flip).toBe('horizontal');
    expect(state.bottomZZero).toBe('top');
  });
  it('builds a rotary job laid out on the unwrapped blank', () => {
    const state = {
      ...stateFromJob(DEFAULT_JOB),
      kind: 'rotary' as const,
      rotaryLength: '240',
      rotaryDiameter: '50',
      rotaryAxis: 'x' as const,
      rotaryZZero: 'centre' as const,
    };
    const job = jobFromState(state);
    expect(job?.width).toBe(240);
    expect(job?.height).toBeCloseTo(Math.PI * 50, 9);
    expect(job?.thickness).toBe(25);
    expect(job?.zZero).toBe('bottom');
    expect(job?.rotary).toEqual({
      diameter: 50,
      axis: 'x',
      length: 240,
      blank: 'cylinder',
      zZero: 'centre',
    });
    const along = jobFromState({ ...state, rotaryAxis: 'y', rotaryZZero: 'surface' });
    expect(along?.height).toBe(240);
    expect(along?.width).toBeCloseTo(Math.PI * 50, 9);
    expect(along?.zZero).toBe('top');
  });

  it('checks the blank rather than the flat size for a rotary job', () => {
    const state = {
      ...stateFromJob(DEFAULT_JOB),
      kind: 'rotary' as const,
      width: 'none',
      rotaryLength: '0',
    };
    const errors = validate(state);
    expect(errors.width).toBeUndefined();
    expect(errors.rotaryLength).toBeDefined();
  });

  it('shows the unwrapped size in the form unit', () => {
    const state = {
      ...stateFromJob(DEFAULT_JOB),
      rotaryAxis: 'x' as const,
      rotaryLength: '10',
      rotaryDiameter: '2',
    };
    expect(unwrappedLayout(state)).toEqual({ width: '10', height: '6.283' });
    expect(unwrappedLayout({ ...state, rotaryAxis: 'y' })).toEqual({
      width: '6.283',
      height: '10',
    });
    expect(unwrappedLayout({ ...state, rotaryDiameter: '' })).toBeNull();
  });

  it('reads older rotary settings with defaults', () => {
    const state = stateFromJob(DEFAULT_JOB);
    expect(state.rotaryLength).toBe('300');
    expect(state.rotaryBlank).toBe('cylinder');
    expect(state.rotaryZZero).toBe('centre');
  });

  it('starts with Scale Design off', () => {
    expect(stateFromJob(DEFAULT_JOB).scaleDesign).toBe(false);
  });

  it('works out the design scale only when asked and the size changes', () => {
    const state = { ...stateFromJob(DEFAULT_JOB), width: '600', height: '100' };
    expect(designScaleOf(state, DEFAULT_JOB)).toBeNull();
    const scale = designScaleOf({ ...state, scaleDesign: true }, DEFAULT_JOB);
    expect(scale?.scaleX).toBe(2);
    expect(scale?.scaleY).toBe(0.5);
    if (scale !== null) {
      expect(scaleSummary(scale)).toBe('Design scale: X 200%, Y 50%');
    }
    const same = { ...stateFromJob(DEFAULT_JOB), thickness: '6', scaleDesign: true };
    expect(designScaleOf(same, DEFAULT_JOB)).toBeNull();
    const bad = { ...state, width: 'abc', scaleDesign: true };
    expect(designScaleOf(bad, DEFAULT_JOB)).toBeNull();
    const rotary = { ...state, kind: 'rotary' as const, scaleDesign: true };
    expect(designScaleOf(rotary, DEFAULT_JOB)).toBeNull();
  });

  it('scales a rotary design around the blank with its circumference', () => {
    const current = normaliseRotary({
      ...DEFAULT_JOB,
      kind: 'rotary',
      rotary: { diameter: 60, axis: 'x', length: 200 },
    });
    const wider = { ...stateFromJob(current), rotaryDiameter: '62', scaleDesign: true };
    const scale = designScaleOf(wider, current);
    expect(scale?.scaleX).toBeCloseTo(1, 12);
    expect(scale?.scaleY).toBeCloseTo(62 / 60, 12);
    if (scale !== null) {
      // The far end of a line all the way round lands on the new circumference.
      const from = applyScale(scale, { x: 20, y: 0 });
      const to = applyScale(scale, { x: 20, y: Math.PI * 60 });
      expect(to.y - from.y).toBeCloseTo(Math.PI * 62, 9);
    }
    expect(designScaleOf({ ...wider, scaleDesign: false }, current)).toBeNull();
    const flat = { ...stateFromJob(current), kind: 'singleSided' as const, scaleDesign: true };
    expect(designScaleOf(flat, current)).toBeNull();
  });
});

describe('job appearance', () => {
  it('reads and writes the material look with the job', () => {
    const state = stateFromJob({
      ...DEFAULT_JOB,
      appearance: { material: 'walnut', colour: '#123456' },
    });
    expect(state.material).toBe('walnut');
    const job = jobFromState({ ...state, material: 'solid', solidColour: '#abcdef' });
    expect(job?.appearance).toEqual({ material: 'solid', colour: '#abcdef' });
    const old = stateFromJob({ ...DEFAULT_JOB, appearance: undefined });
    expect(old.material).toBe('theme');
  });

  it('keeps a texture through OK, and Material Settings with another material drops it', () => {
    const state = stateFromJob({
      ...DEFAULT_JOB,
      appearance: { material: 'oak', colour: '#123456', texture: 4 },
    });
    expect(jobFromState(state)?.appearance?.texture).toBe(4);
    expect(withMaterial(state, 'oak', '#123456').texture).toBe(4);
    const solid = withMaterial(state, 'solid', '#abcdef');
    expect(solid.texture).toBe(0);
    expect(jobFromState(solid)?.appearance).toEqual({ material: 'solid', colour: '#abcdef' });
  });
});

describe('modelling resolution', () => {
  it('reads and writes the resolution, and older jobs are standard', () => {
    const state = stateFromJob({ ...DEFAULT_JOB, resolution: 'high' });
    expect(state.resolution).toBe('high');
    expect(jobFromState({ ...state, resolution: 'highest' })?.resolution).toBe('highest');
    const old = stateFromJob({ ...DEFAULT_JOB, resolution: undefined });
    expect(old.resolution).toBe('standard');
  });

  it('flips the design only for a rotary job whose axis changes with Flip Design on', () => {
    const rotary: JobSetup = {
      ...DEFAULT_JOB,
      kind: 'rotary',
      rotary: { ...DEFAULT_JOB.rotary, axis: 'x' },
    };
    const state = stateFromJob(rotary);
    expect(state.flipDesign).toBe(true);
    expect(flipsDesign(state, rotary)).toBe(false);
    expect(flipsDesign({ ...state, rotaryAxis: 'y' }, rotary)).toBe(true);
    expect(flipsDesign({ ...state, rotaryAxis: 'y', flipDesign: false }, rotary)).toBe(false);
    expect(flipsDesign({ ...state, rotaryAxis: 'y' }, DEFAULT_JOB)).toBe(false);
  });
});

describe('job setup datum offset', () => {
  it('uses the offset only with Use Offset on', () => {
    const state = { ...stateFromJob(DEFAULT_JOB), offsetX: '5', offsetY: 'x' };
    expect(state.useOffset).toBe(false);
    expect(jobFromState(state)).toMatchObject({ offsetX: 0, offsetY: 0 });
    expect(validate({ ...state, useOffset: true }).offsetY).toBeDefined();
    const on = { ...state, useOffset: true, offsetY: '2' };
    expect(jobFromState(on)).toMatchObject({ offsetX: 5, offsetY: 2 });
    expect(stateFromJob({ ...DEFAULT_JOB, offsetX: 3 }).useOffset).toBe(true);
  });
});

describe('datum marker', () => {
  it('marks the chosen corner of the material as it is now', () => {
    const job = { ...DEFAULT_JOB, width: 300, height: 200, xyOrigin: 'bottomLeft' as const };
    const state = stateFromJob(job);
    expect(datumMarker(job, state)).toEqual({ x: 0, y: 0 });
    expect(datumMarker(job, { ...state, xyOrigin: 'topRight' })).toEqual({ x: 300, y: 200 });
    expect(datumMarker(job, { ...state, xyOrigin: 'centre' })).toEqual({ x: 150, y: 100 });
    expect(datumMarker(job, { ...state, width: 'x' })).toBeNull();
  });

  it('reframes the view only when the material moves in job space', () => {
    expect(materialMoved(DEFAULT_JOB, { ...DEFAULT_JOB })).toBe(false);
    expect(materialMoved(DEFAULT_JOB, { ...DEFAULT_JOB, thickness: 30 })).toBe(false);
    expect(materialMoved(DEFAULT_JOB, { ...DEFAULT_JOB, width: 400 })).toBe(true);
    expect(materialMoved(DEFAULT_JOB, { ...DEFAULT_JOB, xyOrigin: 'centre' })).toBe(true);
    expect(materialMoved(DEFAULT_JOB, { ...DEFAULT_JOB, offsetX: 10 })).toBe(true);
  });
});
