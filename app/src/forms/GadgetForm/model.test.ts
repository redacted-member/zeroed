import { describe, expect, it } from 'vitest';

import type { GadgetField } from '../../bridge/gadgets';
import {
  convertGadgetUnits,
  fieldDisabled,
  gadgetSections,
  gadgetValid,
  gadgetValues,
  initialGadgetState,
  validateGadget,
} from './model';

const FIELDS: GadgetField[] = [
  { id: 'teeth', label: 'Teeth', kind: 'number', default: 20, min: 8, max: 300 },
  { id: 'module', label: 'Module', kind: 'length', default: 2, min: 0.1 },
  { id: 'bore', label: 'Bore', kind: 'length', default: 25.4, min: 0 },
  {
    id: 'pressure',
    label: 'Pressure',
    kind: 'choice',
    default: '20',
    options: [
      { value: '14.5', label: '14.5' },
      { value: '20', label: '20' },
    ],
  },
  { id: 'hole', label: 'Hole', kind: 'check', default: true },
  { id: 'layer', label: 'Layer', kind: 'text', default: 'Gear' },
];

describe('gadget form model', () => {
  it('starts from the defaults, lengths in the display unit', () => {
    expect(initialGadgetState(FIELDS, 'mm')).toEqual({
      teeth: '20',
      module: '2',
      bore: '25.4',
      pressure: '20',
      hole: true,
      layer: 'Gear',
    });
    expect(initialGadgetState(FIELDS, 'inch').bore).toBe('1');
  });

  it('sends numbers, millimetre lengths, choices, checks and text', () => {
    const state = { ...initialGadgetState(FIELDS, 'inch'), module: '0.1', hole: false };
    expect(gadgetValues(FIELDS, state, 'inch')).toEqual({
      teeth: 20,
      module: 2.54,
      bore: 25.4,
      pressure: '20',
      hole: false,
      layer: 'Gear',
    });
  });

  it('refuses values outside the field limits', () => {
    const state = { ...initialGadgetState(FIELDS, 'mm'), teeth: '4', module: 'x' };
    const errors = validateGadget(FIELDS, state, 'mm');
    expect(errors.teeth).toBe('Enter a value of at least 8.');
    expect(errors.module).toBe('Enter a number.');
    expect(errors.bore).toBeUndefined();
    expect(gadgetValid(errors)).toBe(false);
    expect(gadgetValues(FIELDS, state, 'mm')).toBeNull();
  });

  it('shows length limits in the display unit', () => {
    const state = { ...initialGadgetState(FIELDS, 'inch'), module: '0.001' };
    expect(validateGadget(FIELDS, state, 'inch').module).toBe('Enter a value of at least 0.0039.');
  });

  it('converts only the length fields when the unit changes', () => {
    const state = initialGadgetState(FIELDS, 'mm');
    const inch = convertGadgetUnits(FIELDS, state, 'mm', 'inch');
    expect(inch.bore).toBe('1');
    expect(inch.teeth).toBe('20');
    expect(convertGadgetUnits(FIELDS, inch, 'inch', 'mm').bore).toBe('25.4');
    expect(convertGadgetUnits(FIELDS, state, 'mm', 'mm')).toBe(state);
  });
});

describe('gadget tool fields', () => {
  const toolField = { id: 'tool', label: 'Tool', kind: 'tool' as const, default: 0 };

  it('needs a tool chosen and sends its id', () => {
    const state = initialGadgetState([toolField], 'mm');
    expect(state['tool']).toBe('');
    expect(validateGadget([toolField], state, 'mm')['tool']).toBeDefined();
    expect(gadgetValues([toolField], state, 'mm')).toBeNull();
    expect(gadgetValues([toolField], { tool: '12' }, 'mm')).toEqual({ tool: 12 });
  });
});

describe('gadget sections and dependent fields', () => {
  // The keyhole gadget's dialog, trimmed.
  const KEYHOLE: GadgetField[] = [
    {
      id: 'direction',
      label: 'Slot orientation',
      kind: 'choice',
      default: 'bottomToTop',
      group: 'Slot Orientation',
      options: [
        { value: 'bottomToTop', label: 'Vertical, bottom to top' },
        { value: 'leftToRight', label: 'Horizontal, left to right' },
      ],
    },
    { id: 'depth', label: 'Slot depth', kind: 'length', default: 6.35, group: 'Slot Dimensions' },
    { id: 'length', label: 'Slot length', kind: 'length', default: 25.4, group: 'Slot Dimensions' },
    { id: 'preview', label: 'Preview', kind: 'check', default: true, group: 'Preview Drawing' },
    {
      id: 'hole',
      label: 'Entry hole diameter',
      kind: 'length',
      default: 12.7,
      min: 0.01,
      group: 'Preview Drawing',
      enabledBy: 'preview',
    },
    { id: 'name', label: 'Toolpath name', kind: 'text', default: 'Keyhole Toolpath' },
  ];

  it('puts neighbouring fields of one group in one section', () => {
    const sections = gadgetSections(KEYHOLE);
    expect(sections.map((section) => section.group)).toEqual([
      'Slot Orientation',
      'Slot Dimensions',
      'Preview Drawing',
      undefined,
    ]);
    expect(sections[1]?.fields.map((field) => field.id)).toEqual(['depth', 'length']);
  });

  it('shows the defaults in inches', () => {
    const state = initialGadgetState(KEYHOLE, 'inch');
    expect(state['depth']).toBe('0.25');
    expect(state['length']).toBe('1');
    expect(state['hole']).toBe('0.5');
  });

  it('skips and leaves out fields whose check is off', () => {
    const on = { ...initialGadgetState(KEYHOLE, 'mm'), hole: '' };
    const hole = KEYHOLE[4];
    expect(hole).toBeDefined();
    if (hole === undefined) {
      return;
    }
    expect(fieldDisabled(hole, on)).toBe(false);
    expect(validateGadget(KEYHOLE, on, 'mm')['hole']).toBeDefined();
    const off = { ...on, preview: false };
    expect(fieldDisabled(hole, off)).toBe(true);
    expect(validateGadget(KEYHOLE, off, 'mm')['hole']).toBeUndefined();
    const values = gadgetValues(KEYHOLE, off, 'mm');
    expect(values).not.toBeNull();
    expect(values).not.toHaveProperty('hole');
    expect(values?.['preview']).toBe(false);
    expect(values?.['direction']).toBe('bottomToTop');
  });
});

describe('gadget form starting values', () => {
  it('starts from given values, lengths converted to the display unit', () => {
    const state = initialGadgetState(FIELDS, 'inch', {
      teeth: 32,
      bore: 50.8,
      pressure: '14.5',
      hole: false,
      layer: 'Gears',
    });
    expect(state).toEqual({
      teeth: '32',
      module: initialGadgetState(FIELDS, 'inch')['module'],
      bore: '2',
      pressure: '14.5',
      hole: false,
      layer: 'Gears',
    });
  });

  it('keeps the default where a value does not fit the field', () => {
    const state = initialGadgetState(FIELDS, 'mm', {
      teeth: 'many',
      pressure: '30',
      hole: 1,
    });
    expect(state['teeth']).toBe('20');
    expect(state['pressure']).toBe('20');
    expect(state['hole']).toBe(true);
  });

  it('gives back the values it started from', () => {
    const values = { teeth: 12, module: 1.5, bore: 6, pressure: '20', hole: true, layer: 'A' };
    const state = initialGadgetState(FIELDS, 'mm', values);
    expect(gadgetValues(FIELDS, state, 'mm')).toEqual(values);
  });
});
