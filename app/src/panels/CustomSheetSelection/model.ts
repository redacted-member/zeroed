// Pure state for the Custom Sheet Selection dialog: the sheets a nest fills,
// in order, each with how many of it to use.

import type { NestSheetUse } from '../../bridge/document';

/** One row while it is edited: the quantity as typed, blank for No Limit. */
export interface SheetRow {
  sheet: number;
  quantity: string;
}

/** A field of a row: its sheet or its quantity. */
export type SheetField = 'sheet' | 'quantity';

export interface SheetPlanDraft {
  rows: SheetRow[];
  /** The highlighted row the arrows and Delete act on, or null. */
  selected: number | null;
  /** The field open for editing (a double click opens one), or null. */
  editing: { row: number; field: SheetField } | null;
}

/** The draft for a plan, or for the default (the active sheet, No Limit). */
export function draftFromPlan(
  plan: readonly NestSheetUse[] | null,
  active: number,
): SheetPlanDraft {
  const rows =
    plan === null || plan.length === 0
      ? [{ sheet: active, quantity: '' }]
      : plan.map((use) => ({
          sheet: use.sheet,
          quantity: use.quantity === undefined ? '' : String(use.quantity),
        }));
  return { rows, selected: 0, editing: null };
}

/** A double click on a field (or Enter or F2 on it) opens it for editing
 * and highlights its row. */
export function startEdit(draft: SheetPlanDraft, row: number, field: SheetField): SheetPlanDraft {
  if (draft.rows[row] === undefined) {
    return draft;
  }
  return { ...draft, selected: row, editing: { row, field } };
}

/** Leaving the field, Enter or Escape closes it; what was typed stays. */
export function stopEdit(draft: SheetPlanDraft): SheetPlanDraft {
  return draft.editing === null ? draft : { ...draft, editing: null };
}

/** Whether the field is open for editing. */
export function isEditing(draft: SheetPlanDraft, row: number, field: SheetField): boolean {
  return draft.editing?.row === row && draft.editing.field === field;
}

/** The error of a quantity: blank is No Limit, else a whole number from 1 to 100. */
export function quantityError(text: string): string | undefined {
  const trimmed = text.trim();
  if (trimmed === '') {
    return undefined;
  }
  const value = Number(trimmed);
  return Number.isInteger(value) && value >= 1 && value <= 100 ? undefined : 'invalid';
}

/** The plan to send, or null while a quantity does not read or no row is left. */
export function planFromDraft(draft: SheetPlanDraft): NestSheetUse[] | null {
  if (draft.rows.length === 0 || draft.rows.some((row) => quantityError(row.quantity))) {
    return null;
  }
  return draft.rows.map((row) =>
    row.quantity.trim() === ''
      ? { sheet: row.sheet }
      : { sheet: row.sheet, quantity: Number(row.quantity.trim()) },
  );
}

/** Whether a plan is the default one: only the active sheet, No Limit. */
export function isDefaultPlan(plan: readonly NestSheetUse[] | null, active: number): boolean {
  if (plan === null) {
    return true;
  }
  const [only] = plan;
  return plan.length === 1 && only?.sheet === active && only.quantity === undefined;
}

/** Adds a row for the first sheet not in the list yet (else the last one), with a quantity of 1. */
export function addRow(draft: SheetPlanDraft, sheets: readonly number[]): SheetPlanDraft {
  const used = new Set(draft.rows.map((row) => row.sheet));
  const sheet = sheets.find((id) => !used.has(id)) ?? sheets[sheets.length - 1];
  if (sheet === undefined) {
    return draft;
  }
  const rows = [...draft.rows, { sheet, quantity: '1' }];
  return { rows, selected: rows.length - 1, editing: null };
}

/** Deletes the highlighted row; the last row cannot go. */
export function deleteRow(draft: SheetPlanDraft): SheetPlanDraft {
  const { selected } = draft;
  if (selected === null || draft.rows.length <= 1) {
    return draft;
  }
  const rows = draft.rows.filter((_, index) => index !== selected);
  return { rows, selected: Math.min(selected, rows.length - 1), editing: null };
}

/** Moves the highlighted row up (-1) or down (+1). */
export function moveRow(draft: SheetPlanDraft, by: -1 | 1): SheetPlanDraft {
  const { selected } = draft;
  if (selected === null) {
    return draft;
  }
  const to = selected + by;
  const moving = draft.rows[selected];
  if (moving === undefined || to < 0 || to >= draft.rows.length) {
    return draft;
  }
  const rows = draft.rows.filter((_, index) => index !== selected);
  rows.splice(to, 0, moving);
  return { rows, selected: to, editing: null };
}

/** Changes one field of a row. */
export function editRow(
  draft: SheetPlanDraft,
  index: number,
  change: Partial<SheetRow>,
): SheetPlanDraft {
  const rows = draft.rows.map((row, at) => (at === index ? { ...row, ...change } : row));
  return { ...draft, rows, selected: index };
}
