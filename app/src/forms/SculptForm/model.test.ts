import { describe, expect, it } from 'vitest';

import {
  addPreset,
  adjustSculpt,
  BRUSHES,
  brushFileName,
  brushParams,
  buildsOrCuts,
  customBrushParams,
  hasPreset,
  initialSculptState,
  presetSettings,
  removePreset,
  toolForKey,
  usesCustom,
  validateSculpt,
  withBrushFile,
  withPreset,
} from './model';

describe('sculpt form model', () => {
  it('starts with a valid deposit brush', () => {
    const state = initialSculptState('mm');
    expect(validateSculpt(state, 'mm')).toEqual({});
    expect(brushParams(state, 'mm')).toEqual({
      brush: 'deposit',
      diameter: 10,
      strength: 0.5,
      falloff: 0.5,
      grow: false,
      mode: 'normal',
    });
  });

  it('reads inches and percentages', () => {
    const state = { ...initialSculptState('inch'), diameter: '1', strength: '100', softness: '0' };
    expect(brushParams(state, 'inch')).toMatchObject({ diameter: 25.4, strength: 1, falloff: 0 });
  });

  it('has no height limit: Deposit and Remove send none', () => {
    const state = initialSculptState('mm');
    expect(Object.keys(state)).not.toContain('limit');
    expect(brushParams(state, 'mm')).not.toHaveProperty('limit');
    expect(brushParams({ ...state, brush: 'remove' }, 'mm')).not.toHaveProperty('limit');
    expect(buildsOrCuts('remove')).toBe(true);
    expect(buildsOrCuts('smudge')).toBe(false);
  });

  it('works past the edge for every tool once Preserve Transparency is off', () => {
    const free = { ...initialSculptState('mm'), preserve: false, mode: 'raise' as const };
    expect(brushParams(free, 'mm')).toMatchObject({ grow: true, mode: 'raise' });
    expect(brushParams({ ...free, brush: 'smooth' }, 'mm')?.grow).toBe(true);
    expect(brushParams(initialSculptState('mm'), 'mm')?.grow).toBe(false);
  });

  it('lists the five tools in the order of their keys', () => {
    expect(BRUSHES).toEqual(['smooth', 'smudge', 'deposit', 'remove', 'restore']);
    expect(toolForKey('1')).toBe('smooth');
    expect(toolForKey('5')).toBe('restore');
    expect(toolForKey('6')).toBeNull();
    expect(toolForKey('x')).toBeNull();
  });

  it('keeps presets per tool, replacing one of the same name', () => {
    const state = { ...initialSculptState('mm'), diameter: '4', mode: 'lower' as const };
    const settings = presetSettings(state, 'mm');
    expect(settings).toMatchObject({ diameter: 4, mode: 'lower', preserve: true });
    let list = addPreset([], { name: 'Fine', tool: 'deposit', settings });
    list = addPreset(list, { name: 'Fine', tool: 'smooth', settings });
    expect(list).toHaveLength(2);
    list = addPreset(list, { name: 'Fine', tool: 'deposit', settings: { diameter: 8 } });
    expect(list).toHaveLength(2);
    expect(hasPreset(list, 'Fine', 'smooth')).toBe(true);
    expect(removePreset(list, 'Fine', 'smooth')).toHaveLength(1);
    const back = withPreset(
      initialSculptState('mm'),
      { name: 'F', tool: 'smudge', settings },
      'mm',
    );
    expect(back).toMatchObject({ brush: 'smudge', diameter: '4', mode: 'lower' });
  });

  it('refuses sizes and strengths out of range', () => {
    const state = initialSculptState('mm');
    expect(validateSculpt({ ...state, diameter: '0' }, 'mm').diameter).toBeDefined();
    expect(validateSculpt({ ...state, strength: '0' }, 'mm').strength).toBeDefined();
    expect(validateSculpt({ ...state, softness: '120' }, 'mm').softness).toBeDefined();
    expect(brushParams({ ...state, strength: 'x' }, 'mm')).toBeNull();
  });

  it('offers the undo brush without growth', () => {
    const state = { ...initialSculptState('mm'), brush: 'restore' as const };
    expect(brushParams(state, 'mm')).toMatchObject({ brush: 'restore', grow: false });
  });

  it('takes wheel changes of the diameter and strength', () => {
    const start = initialSculptState('mm');
    const bigger = adjustSculpt(start, { kind: 'diameter', steps: 1, fine: false }, 'mm');
    expect(Number(bigger.diameter)).toBeCloseTo(11);
    const finer = adjustSculpt(start, { kind: 'diameter', steps: -1, fine: true }, 'mm');
    expect(Number(finer.diameter)).toBeCloseTo(9.8);
    expect(adjustSculpt(start, { kind: 'strength', steps: 2, fine: false }, 'mm').strength).toBe(
      '60',
    );
    expect(adjustSculpt(start, { kind: 'strength', steps: 30, fine: false }, 'mm').strength).toBe(
      '100',
    );
    const broken = { ...start, diameter: 'x' };
    expect(adjustSculpt(broken, { kind: 'diameter', steps: 1, fine: false }, 'mm')).toBe(broken);
  });

  it('sends a custom brush only when chosen for Deposit or Remove', () => {
    const start = initialSculptState('mm');
    expect(customBrushParams(start)).toBeNull();
    const chosen = { ...start, useCustom: true, customBrush: '7', angle: '45', dynamic: true };
    expect(usesCustom(chosen)).toBe(true);
    expect(customBrushParams(chosen)).toEqual({ brush: 7, angle: 45, dynamic: true, merge: true });
    expect(customBrushParams({ ...chosen, brush: 'smooth' })).toBeNull();
    expect(customBrushParams({ ...chosen, customBrush: '' })).toBeNull();
    expect(validateSculpt({ ...chosen, angle: 'x' }, 'mm').angle).toBeDefined();
    expect(validateSculpt({ ...start, angle: 'x' }, 'mm').angle).toBeUndefined();
  });

  it('uses a loaded brush file', () => {
    const start = { ...initialSculptState('mm'), angle: '10' };
    const loaded = withBrushFile(start, '/brushes/Leaf.carveclip');
    expect(loaded.useCustom).toBe(true);
    expect(customBrushParams(loaded)).toEqual({
      file: '/brushes/Leaf.carveclip',
      angle: 10,
      dynamic: false,
      merge: true,
    });
    expect(customBrushParams({ ...loaded, brushFile: '' })).toBeNull();
    expect(brushFileName('/brushes/Leaf.carveclip')).toBe('Leaf');
    expect(brushFileName('C:\\pics\\bark.png')).toBe('bark');
  });

  it('turns the custom brush with R and the wheel', () => {
    const start = { ...initialSculptState('mm'), angle: '170' };
    expect(adjustSculpt(start, { kind: 'angle', steps: 1, fine: false }, 'mm').angle).toBe('-175');
    expect(adjustSculpt(start, { kind: 'angle', steps: -2, fine: true }, 'mm').angle).toBe('168');
    const broken = { ...start, angle: 'x' };
    expect(adjustSculpt(broken, { kind: 'angle', steps: 1, fine: false }, 'mm')).toBe(broken);
  });
});
