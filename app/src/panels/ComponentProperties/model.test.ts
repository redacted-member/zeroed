import { describe, expect, it } from 'vitest';

import type { ComponentGroup } from '../../bridge/componentTree';
import type { ComponentSummary, Level } from '../../bridge/model';
import {
  componentPatch,
  componentState,
  convertPropertiesUnits,
  groupPatch,
  groupState,
  levelPatch,
  levelState,
  validateProperties,
} from './model';

const part = (patch: Partial<ComponentSummary> = {}): ComponentSummary => ({
  id: 1,
  name: 'Rose',
  visible: true,
  combine: 'add',
  heightScale: 1,
  level: 1,
  maxHeight: 4,
  baseHeight: 0,
  color: null,
  group: 0,
  ...patch,
});

describe('component properties', () => {
  it('starts from the component, shape height as a percentage', () => {
    const state = componentState(
      [part({ heightScale: 0.5, baseHeight: 2, color: '#112233' })],
      'mm',
    );
    expect(state).toMatchObject({
      name: 'Rose',
      shapeHeight: '50',
      baseHeight: '2',
      appearance: 'colour',
      colour: '#112233',
    });
    expect(componentState([part(), part({ id: 2 })], 'mm').name).toBe('');
  });

  it('sends only what changed', () => {
    const initial = componentState([part()], 'mm');
    expect(componentPatch(initial, initial, 1, 'mm')).toEqual({});
    const edited = {
      ...initial,
      name: ' Lily ',
      combine: 'multiply' as const,
      shapeHeight: '150',
      baseHeight: '3',
      appearance: 'colour' as const,
      colour: '#AABBCC',
    };
    expect(componentPatch(edited, initial, 1, 'mm')).toEqual({
      name: 'Lily',
      combine: 'multiply',
      heightScale: 1.5,
      baseHeight: 3,
      color: '#aabbcc',
    });
    const material = {
      ...componentState([part({ color: '#010203' })], 'mm'),
      appearance: 'parent' as const,
    };
    expect(
      componentPatch(material, componentState([part({ color: '#010203' })], 'mm'), 1, 'mm'),
    ).toEqual({
      color: null,
    });
  });

  it('a material is sent as its colour with its name, and read back', () => {
    const initial = componentState([part()], 'mm');
    expect(initial.appearance).toBe('parent');
    const walnut = { ...initial, appearance: 'material' as const, material: 'walnut' as const };
    const patch = componentPatch(walnut, initial, 1, 'mm');
    expect(patch?.material).toBe('walnut');
    expect(patch?.color).toMatch(/^#[0-9a-f]{6}$/);
    const back = componentState([part({ color: patch?.color ?? null, material: 'walnut' })], 'mm');
    expect(back).toMatchObject({ appearance: 'material', material: 'walnut' });
    expect(
      componentPatch({ ...back, appearance: 'colour', colour: '#123456' }, back, 1, 'mm'),
    ).toEqual({
      color: '#123456',
      material: null,
    });
  });

  it('never renames several components at once', () => {
    const initial = componentState([part(), part({ id: 2 })], 'mm');
    const patch = componentPatch({ ...initial, name: 'X', visible: false }, initial, 2, 'mm');
    expect(patch).toEqual({ visible: false });
  });

  it('checks the numbers and the name', () => {
    const initial = componentState([part()], 'mm');
    const errors = validateProperties(
      { ...initial, name: ' ', shapeHeight: '-5', baseHeight: 'x' },
      'components',
      1,
      'mm',
    );
    expect(Object.keys(errors).sort()).toEqual(['baseHeight', 'name', 'shapeHeight']);
    expect(componentPatch({ ...initial, shapeHeight: 'abc' }, initial, 1, 'mm')).toBeNull();
    expect(validateProperties({ ...initial, name: '' }, 'components', 3, 'mm')).toEqual({});
  });

  it('keeps base heights in the display unit', () => {
    const state = componentState([part({ baseHeight: 25.4 })], 'inch');
    expect(state.baseHeight).toBe('1');
    expect(convertPropertiesUnits(state, 'inch', 'mm').baseHeight).toBe('25.4');
    const patch = componentPatch({ ...state, baseHeight: '2' }, state, 1, 'inch');
    expect(patch?.baseHeight).toBeCloseTo(50.8);
  });
});

describe('group and level properties', () => {
  const group: ComponentGroup = {
    id: 7,
    name: 'Leaves',
    visible: true,
    combine: 'add',
    expanded: true,
    color: null,
  };
  const level: Level = { id: 2, name: 'Level 2', visible: true, combine: 'add' };

  it('a group takes its members colours until given its own', () => {
    const initial = groupState(group, 'mm');
    expect(initial.appearance).toBe('children');
    expect(groupPatch({ ...initial, appearance: 'colour', colour: '#00ff00' }, initial)).toEqual({
      color: '#00ff00',
    });
    expect(groupPatch({ ...initial, combine: 'subtract', name: 'Veins' }, initial)).toEqual({
      combine: 'subtract',
      name: 'Veins',
    });
    expect(groupPatch({ ...initial, name: '' }, initial)).toBeNull();
  });

  it('a level takes its components colours until given its own', () => {
    const initial = levelState(level, 'mm');
    expect(initial.appearance).toBe('children');
    expect(
      levelPatch({ ...initial, appearance: 'colour', colour: '#0000ff' }, initial, 'mm'),
    ).toEqual({
      color: '#0000ff',
    });
    const oak = levelState({ ...level, color: '#b98c57', material: 'oak' }, 'mm');
    expect(oak.appearance).toBe('material');
    expect(levelPatch({ ...oak, appearance: 'children' }, oak, 'mm')).toEqual({
      color: null,
      material: null,
    });
  });

  it('a level edits its name, combine mode and visibility', () => {
    const initial = levelState(level, 'mm');
    expect(levelPatch({ ...initial, combine: 'mergeLow', visible: false }, initial, 'mm')).toEqual({
      combine: 'mergeLow',
      visible: false,
    });
  });

  it('a level has a base height of its own, in the job unit', () => {
    const initial = levelState({ ...level, baseHeight: 2 }, 'inch');
    expect(initial.baseHeight).toBe('0.0787');
    expect(levelPatch({ ...initial, baseHeight: '0.5' }, initial, 'inch')).toEqual({
      baseHeight: 12.7,
    });
    expect(levelPatch({ ...initial, baseHeight: 'x' }, initial, 'inch')).toBeNull();
    expect(
      validateProperties({ ...initial, baseHeight: 'x' }, 'level', 0, 'inch').baseHeight,
    ).toBeDefined();
  });

  it('sends a fade or tilt once its anchors are set, and null to turn it off', () => {
    const tilt = { pivot: { x: 0, y: 0 }, toward: { x: 0, y: 10 }, angleDeg: 8 };
    const initial = componentState([part({ tilt })], 'mm');
    expect(initial.dynamic).toMatchObject({ tiltOn: true, tiltAngle: '8', fadeOn: false });
    expect(componentPatch(initial, initial, 1, 'mm')).toEqual({});
    const waiting = { ...initial, dynamic: { ...initial.dynamic, fadeOn: true } };
    expect(componentPatch(waiting, initial, 1, 'mm')).toEqual({});
    const faded = {
      ...waiting,
      dynamic: {
        ...waiting.dynamic,
        fadeFrom: { x: 1, y: 1 },
        fadeTo: { x: 9, y: 1 },
        fadePercent: '30',
        tiltOn: false,
      },
    };
    expect(componentPatch(faded, initial, 1, 'mm')).toEqual({
      fade: { from: { x: 1, y: 1 }, to: { x: 9, y: 1 }, amount: 0.3 },
      tilt: null,
    });
    const bad = { ...faded, dynamic: { ...faded.dynamic, fadePercent: '101' } };
    expect(validateProperties(bad, 'components', 1, 'mm').fadePercent).toBeDefined();
  });
});
