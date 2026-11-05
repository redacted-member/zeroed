import { describe, expect, it } from 'vitest';

import {
  addRow,
  deleteRow,
  draftFromPlan,
  editRow,
  isDefaultPlan,
  isEditing,
  moveRow,
  planFromDraft,
  quantityError,
  startEdit,
  stopEdit,
} from './model';

describe('custom sheet selection model', () => {
  it('starts from the active sheet with No Limit', () => {
    const draft = draftFromPlan(null, 7);
    expect(draft.rows).toEqual([{ sheet: 7, quantity: '' }]);
    expect(planFromDraft(draft)).toEqual([{ sheet: 7 }]);
    expect(isDefaultPlan(null, 7)).toBe(true);
    expect(isDefaultPlan([{ sheet: 7 }], 7)).toBe(true);
    expect(isDefaultPlan([{ sheet: 7, quantity: 2 }], 7)).toBe(false);
  });

  it('adds the next unused sheet with a quantity of 1, and deletes all but the last row', () => {
    let draft = draftFromPlan([{ sheet: 1 }], 1);
    draft = addRow(draft, [1, 2, 3]);
    expect(draft.rows[1]).toEqual({ sheet: 2, quantity: '1' });
    expect(draft.selected).toBe(1);
    draft = deleteRow(draft);
    expect(draft.rows).toHaveLength(1);
    expect(deleteRow(draft).rows).toHaveLength(1);
  });

  it('moves the highlighted row up and down within the list', () => {
    let draft = draftFromPlan([{ sheet: 1 }, { sheet: 2, quantity: 1 }], 1);
    draft = { ...draft, selected: 1 };
    draft = moveRow(draft, -1);
    expect(draft.rows.map((row) => row.sheet)).toEqual([2, 1]);
    expect(draft.selected).toBe(0);
    expect(moveRow(draft, -1)).toBe(draft);
  });

  it('reads blank as No Limit and refuses other quantities that are not whole', () => {
    expect(quantityError('')).toBeUndefined();
    expect(quantityError('3')).toBeUndefined();
    expect(quantityError('0')).toBeDefined();
    expect(quantityError('1.5')).toBeDefined();
    const draft = editRow(draftFromPlan(null, 1), 0, { quantity: '2' });
    expect(planFromDraft(draft)).toEqual([{ sheet: 1, quantity: 2 }]);
    expect(planFromDraft(editRow(draft, 0, { quantity: 'x' }))).toBeNull();
  });

  it('opens one field for editing on a double click and closes it again', () => {
    let draft = draftFromPlan([{ sheet: 1 }, { sheet: 2, quantity: 1 }], 1);
    expect(draft.editing).toBeNull();
    draft = startEdit(draft, 1, 'quantity');
    expect(draft.selected).toBe(1);
    expect(isEditing(draft, 1, 'quantity')).toBe(true);
    expect(isEditing(draft, 1, 'sheet')).toBe(false);
    expect(isEditing(draft, 0, 'quantity')).toBe(false);
    draft = editRow(draft, 1, { quantity: '4' });
    expect(isEditing(draft, 1, 'quantity')).toBe(true);
    draft = stopEdit(draft);
    expect(draft.editing).toBeNull();
    expect(planFromDraft(draft)).toEqual([{ sheet: 1 }, { sheet: 2, quantity: 4 }]);
    expect(startEdit(draft, 5, 'sheet')).toBe(draft);
    // Moving, adding or deleting a row closes the field.
    expect(moveRow(startEdit(draft, 1, 'sheet'), -1).editing).toBeNull();
  });
});
