import { describe, expect, it } from 'vitest';

import type { ToolDatabaseList } from '../../bridge/cam';
import {
  EDITABLE_KINDS,
  blankTool,
  breadcrumb,
  chipLoad,
  groupEditorName,
  insertVariable,
  notesLinks,
  nudge,
  pickableDatabase,
  placement,
  savePlan,
  setGeometry,
  setHolder,
  setStepoverDistance,
  setStepoverPercent,
  switchKind,
  switchUnits,
  toolFormState,
  toolFromState,
  usesVariable,
  validVariableName,
  validateTool,
} from './model';

describe('tool database editor model', () => {
  it('round trips a tool through the form', () => {
    const tool = blankTool(2);
    const back = toolFromState(toolFormState(tool), tool);
    expect(back).toEqual({ ...tool, stepover: 2.4 });
  });

  it('edits the shank and holder the preview checks, in the tool unit', () => {
    const tool = blankTool(2);
    let state = toolFormState(tool);
    expect(state.holder.fluteLength).toBe('0');
    expect(state.centerCutting).toBe(true);
    state = setHolder(state, 'fluteLength', '22');
    state = setHolder(state, 'holderDiameter', '35');
    state = setHolder(state, 'lengthBelowHolder', '30');
    state = { ...state, centerCutting: false };
    const built = toolFromState(state, tool);
    expect(built?.fluteLength).toBe(22);
    expect(built?.holderDiameter).toBe(35);
    expect(built?.lengthBelowHolder).toBe(30);
    expect(built?.centerCutting).toBe(false);
    // Left at zero, a field the tool did not have stays out.
    expect(built !== null && 'holderLength' in built).toBe(false);
    // Inches convert, and back again.
    const inches = switchUnits(state, 'inch');
    expect(Number(inches.holder.fluteLength)).toBeCloseTo(22 / 25.4, 3);
    expect(toolFromState(inches, tool)?.fluteLength).toBeCloseTo(22, 2);
    expect(validateTool(setHolder(state, 'shankDiameter', '-1')).shankDiameter).toBeDefined();
  });

  it('asks for the geometry of the chosen type and keeps shared fields', () => {
    const state = switchKind(toolFormState(blankTool(0)), 'vBit');
    expect(state.geometry).toEqual({ diameter: '6', includedAngle: '90' });
    const tool = toolFromState(
      { ...state, geometry: { diameter: '12', includedAngle: '60' } },
      blankTool(0),
    );
    expect(tool?.toolType).toEqual({ kind: 'vBit', diameter: 12, includedAngle: 60 });
    expect(tool?.stepover).toBeCloseTo(4.8);
  });

  it('converts lengths but not angles when the unit changes', () => {
    const state = switchKind(toolFormState(blankTool(0)), 'vBit');
    const inch = switchUnits(
      { ...state, geometry: { diameter: '25.4', includedAngle: '60' } },
      'inch',
    );
    expect(inch.geometry).toEqual({ diameter: '1', includedAngle: '60' });
    expect(toolFromState(inch, blankTool(0))?.toolType['diameter']).toBeCloseTo(25.4);
  });

  it('flags a missing name and impossible values', () => {
    const errors = validateTool({
      ...toolFormState(blankTool(0)),
      name: ' ',
      stepoverPercent: '150',
      geometry: { diameter: '0' },
    });
    expect(errors.name).toBeDefined();
    expect(errors.stepoverPercent).toBeDefined();
    expect(errors.diameter).toBeDefined();
  });
});

function database(): ToolDatabaseList {
  const tool = (id: number, group: number) => ({ ...blankTool(group), id, name: `T${id}` });
  return {
    dbRevision: 1,
    groups: [
      { id: 1, name: 'Metric', parent: null, path: ['Metric'] },
      { id: 2, name: 'Mills', parent: 1, path: ['Metric', 'Mills'] },
      { id: 3, name: 'Inch', parent: null, path: ['Inch'] },
    ],
    tools: [tool(10, 0), tool(11, 2), tool(12, 2), tool(13, 2)],
    materials: [],
    machines: [],
  };
}

describe('tool database editor, reference fields', () => {
  it('offers every tool type in order', () => {
    expect(EDITABLE_KINDS.slice(0, 9)).toEqual([
      'vBit',
      'engravingTool',
      'taperedBallNose',
      'ballNose',
      'endMill',
      'radiusedEndMill',
      'formTool',
      'diamondDrag',
      'drill',
    ]);
  });

  it('keeps stepover percent and distance in step, also when the diameter changes', () => {
    let state = toolFormState(blankTool(0));
    state = setStepoverPercent(state, '50');
    expect(state.stepover).toBe('3');
    state = setStepoverDistance(state, '1.5');
    expect(state.stepoverPercent).toBe('25');
    state = setGeometry(state, 'diameter', '12');
    expect(state.stepover).toBe('3');
  });

  it('needs a profile before a form cutter can be saved', () => {
    const state = switchKind(toolFormState(blankTool(0)), 'formTool');
    expect(validateTool(state).profile).toBeDefined();
    const withProfile = { ...state, profile: { subpaths: [] } };
    expect(validateTool(withProfile).profile).toBeUndefined();
    expect(toolFromState(withProfile, blankTool(0))?.toolType).toEqual({
      kind: 'formTool',
      profile: { subpaths: [] },
    });
  });

  it('edits a laser by kerf, power, speed and Maximum Burn Rate', () => {
    const tool = blankTool(0, 'laserTool');
    expect(tool.toolType).toEqual({
      kind: 'laserTool',
      kerf: 0.2,
      power: 80,
      speed: 1200,
      maxBurnRate: 3000,
    });
    const state = switchUnits(toolFormState(tool), 'inch');
    expect(state.geometry.power).toBe('80');
    // Speeds stay in mm per minute whatever the length unit.
    expect(state.geometry.maxBurnRate).toBe('3000');
    const faster = { ...state, geometry: { ...state.geometry, maxBurnRate: '4500' } };
    expect(toolFromState(faster, tool)?.toolType['speed']).toBe(1200);
    expect(toolFromState(faster, tool)?.toolType['maxBurnRate']).toBe(4500);
    const none = { ...state, geometry: { ...state.geometry, maxBurnRate: '0' } };
    expect(validateTool(none).maxBurnRate).toBeDefined();
    // A laser tool saved before the field existed opens with the default.
    const old = {
      ...tool,
      toolType: { kind: 'laserTool' as const, kerf: 0.1, power: 80, speed: 1200 },
    };
    expect(toolFormState(old).geometry.maxBurnRate).toBe('3000');
  });

  it('renames a new tool with the format of its new type', () => {
    const formats = { endMill: '{Tool Type} {Diameter}', vBit: '{Tool Type} {Included Angle}' };
    const state = toolFormState(blankTool(0, 'endMill', formats.endMill));
    expect(switchKind(state, 'vBit', formats).name).toBe(formats.vBit);
    const typed = { ...state, name: 'Mine' };
    expect(switchKind(typed, 'vBit', formats).name).toBe('Mine');
  });

  it('writes cutting data for the material pair and keeps the tool values', () => {
    const stored = blankTool(0);
    const state = { ...toolFormState(stored), feedRate: '999', number: '7' };
    const own = savePlan(state, stored, { machine: 'Shop', material: '' });
    expect(own?.cuttingData).toBeNull();
    expect(own?.tool.feedRate).toBe(999);
    const oak = savePlan(state, stored, { machine: 'Shop', material: 'Oak' });
    expect(oak?.tool.feedRate).toBe(2400);
    expect(oak?.tool.number).toBe(1);
    expect(oak?.cuttingData).toMatchObject({
      machine: 'Shop',
      material: 'Oak',
      feedRate: 999,
      number: 7,
    });
  });

  it('reads cutting fields from the tool resolved for the context', () => {
    const stored = blankTool(0);
    const resolved = { ...stored, feedRate: 1234, spindleSpeed: 12000 };
    const state = toolFormState(stored, resolved);
    expect(state.feedRate).toBe('1234');
    expect(state.spindleSpeed).toBe('12000');
  });

  it('computes the chip load in the tool units', () => {
    const state = { ...toolFormState(blankTool(0)), spindleSpeed: '12000', feedRate: '2400' };
    expect(chipLoad(state)).toBeCloseTo(0.1);
    expect(chipLoad({ ...state, flutes: '0' })).toBeNull();
    expect(chipLoad({ ...state, units: 'inch' })).toBeCloseTo(0.1 / 25.4);
  });

  it('finds links in notes and inserts variables into names', () => {
    expect(notesLinks('See https://example.com/a, and http://x.org.')).toEqual([
      'https://example.com/a',
      'http://x.org',
    ]);
    expect(insertVariable('End Mill', 'Diameter')).toBe('End Mill {Diameter}');
    expect(insertVariable('', 'Diameter')).toBe('{Diameter}');
    expect(usesVariable('A {Maker|U}', 'Maker')).toBe(true);
    expect(usesVariable('A {Makers}', 'Maker')).toBe(false);
    expect(validVariableName('Maker')).toBe(true);
    expect(validVariableName('a{b')).toBe(false);
  });
});

describe('tool tree drag and drop', () => {
  it('drops a tool into a group or before another tool', () => {
    const db = database();
    expect(placement(db, { kind: 'tool', id: 10 }, { kind: 'group', id: 3 })).toEqual({
      item: { kind: 'tool', id: 10 },
      group: 3,
      before: null,
    });
    expect(placement(db, { kind: 'tool', id: 13 }, { kind: 'tool', id: 11 })).toEqual({
      item: { kind: 'tool', id: 13 },
      group: 2,
      before: 11,
    });
    expect(placement(db, { kind: 'tool', id: 11 }, { kind: 'tool', id: 11 })).toBeNull();
    expect(placement(db, { kind: 'tool', id: 11 }, 'top')?.group).toBe(0);
  });

  it('never drops a group inside itself', () => {
    const db = database();
    expect(placement(db, { kind: 'group', id: 1 }, { kind: 'group', id: 2 })).toBeNull();
    expect(placement(db, { kind: 'group', id: 1 }, { kind: 'tool', id: 11 })).toBeNull();
    expect(placement(db, { kind: 'group', id: 3 }, { kind: 'group', id: 1 })).toEqual({
      item: { kind: 'group', id: 3 },
      group: 0,
      before: 1,
    });
  });

  it('nudges an item among its siblings with the keyboard', () => {
    const db = database();
    expect(nudge(db, { kind: 'tool', id: 12 }, 'up')).toMatchObject({ group: 2, before: 11 });
    expect(nudge(db, { kind: 'tool', id: 12 }, 'down')).toMatchObject({ group: 2, before: null });
    expect(nudge(db, { kind: 'tool', id: 11 }, 'down')).toMatchObject({ before: 13 });
    expect(nudge(db, { kind: 'tool', id: 11 }, 'up')).toBeNull();
    expect(nudge(db, { kind: 'tool', id: 13 }, 'down')).toBeNull();
  });

  it('shows the group path as a breadcrumb', () => {
    const db = database();
    expect(breadcrumb(db, { kind: 'tool', id: 11 })).toEqual(['Metric', 'Mills']);
    expect(breadcrumb(db, { kind: 'tool', id: 10 })).toEqual([]);
  });

  it('edits a thread mill with its pitch, tooth offset and teeth', () => {
    const tool = blankTool(1, 'threadMill');
    expect(tool.toolType).toMatchObject({
      kind: 'threadMill',
      diameter: 6,
      pitch: 1.5,
      toothOffset: 0,
      teeth: 1,
    });
    const state = setGeometry(setGeometry(toolFormState(tool), 'teeth', '4'), 'toothOffset', '2');
    expect(validateTool(state)).toEqual({});
    expect(toolFromState(state, tool)?.toolType).toMatchObject({ teeth: 4, toothOffset: 2 });
    expect(validateTool(setGeometry(state, 'teeth', '1.5')).teeth).toBeDefined();
    expect(validateTool(setGeometry(state, 'toothOffset', '-1')).toothOffset).toBeDefined();
  });
});

describe('picking a tool for a form', () => {
  it('lists only the kinds the form cuts with, and every group', () => {
    const db = database();
    const kind = db.tools[0]?.toolType.kind ?? 'endMill';
    expect(pickableDatabase(db, null)).toBe(db);
    expect(pickableDatabase(db, [kind]).tools).toHaveLength(4);
    const none = pickableDatabase(db, kind === 'vBit' ? ['drill'] : ['vBit']);
    expect(none.tools).toHaveLength(0);
    expect(none.groups).toHaveLength(3);
  });

  it('remakes the group editor once the database lists a new group', () => {
    const before = { groups: [], tools: [] } as unknown as ToolDatabaseList;
    const after = {
      groups: [{ id: 34, name: 'New Group', order: 1, parent: null, path: ['New Group'] }],
      tools: [],
    } as unknown as ToolDatabaseList;
    const early = groupEditorName(before, 34);
    const late = groupEditorName(after, 34);
    expect(early.name).toBe('');
    expect(late.name).toBe('New Group');
    expect(late.key).not.toBe(early.key);
  });
});
