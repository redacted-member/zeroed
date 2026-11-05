import { describe, expect, it } from 'vitest';

import { entriesWithoutIcon, menuItems } from '../../views/PopupMenu/model';
import { insertVariable, variableMenu } from './model';

describe('variable menu of numeric fields', () => {
  it('lists the variables by name and offers a new one', () => {
    const items = menuItems(variableMenu(['width', 'depth']));
    expect(items.map((i) => i.action)).toEqual([
      { insert: 'depth' },
      { insert: 'width' },
      { create: true },
    ]);
    expect(menuItems(variableMenu([]))[0]?.disabled).toBe(true);
    // Every command item carries an icon; the disabled note stays blank.
    expect(items.map((i) => i.icon)).toEqual(['variables', 'variables', 'variableNew']);
    expect(menuItems(variableMenu([]))[0]?.icon).toBeUndefined();
    expect(menuItems(variableMenu([]))[0]?.info).toBe(true);
    expect(entriesWithoutIcon(variableMenu([]))).toEqual([]);
    expect(entriesWithoutIcon(variableMenu(['w', 'd']))).toEqual([]);
  });

  it('puts the variable where the caret is', () => {
    expect(insertVariable('10 + ', 'depth', 5, 5)).toEqual({ text: '10 + {depth}', caret: 12 });
    expect(insertVariable('12', 'w', 0, 2)).toEqual({ text: '{w}', caret: 3 });
    expect(insertVariable('5', 'w', null, null)).toEqual({ text: '5{w}', caret: 4 });
  });
});
