// Pure logic of the Sheets tab: the names it proposes, which sheets are
// picked in the list, what Add New copies, what Delete asks, the size shown
// on each row and the right click menu.

import type { Sheet } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import { strings } from '../../lib/strings';
import { whyStrings } from '../../lib/strings/why';
import { formatLength, unitSuffix } from '../../lib/units';
import { withIcons, type PopupEntry } from '../../views/PopupMenu/model';

/** The icons of the sheet menu, by item id. */
const SHEET_ICONS: Readonly<Record<string, string>> = {
  edit: 'properties',
  rename: 'rename',
  duplicate: 'editCopy',
  delete: 'editDelete',
  active: 'sheetActive',
  moveHere: 'sheetMoveHere',
  newSheet: 'sheetAdd',
};

/** "Sheet N" with the lowest N from the sheet count up that no sheet uses yet. */
export function nextSheetName(sheets: readonly Sheet[], taken: readonly string[] = []): string {
  const used = new Set([...sheets.map((s) => s.name), ...taken].map((n) => n.trim().toLowerCase()));
  let n = sheets.length + taken.length + 1;
  while (used.has(`sheet ${n}`)) {
    n += 1;
  }
  return `Sheet ${n}`;
}

export interface PickModifiers {
  /** Cmd or Ctrl: add or remove one sheet. */
  toggle: boolean;
  /** Shift: every sheet from the anchor to this one. */
  range: boolean;
}

/**
 * The picked sheets after a click on `id`. `anchor` is the sheet the last
 * plain or toggle click landed on, for Shift ranges.
 */
export function pickSheets(
  sheets: readonly Sheet[],
  picked: readonly number[],
  anchor: number | null,
  id: number,
  { toggle, range }: PickModifiers,
): number[] {
  const order = sheets.map((s) => s.id);
  if (range && anchor !== null && order.includes(anchor)) {
    const [from, to] = [order.indexOf(anchor), order.indexOf(id)].sort((a, b) => a - b);
    return order.slice(from, (to ?? from) + 1);
  }
  if (toggle) {
    return picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id];
  }
  return [id];
}

/** Picked ids that are still sheets of the job, in list order. */
export function livePicks(sheets: readonly Sheet[], picked: readonly number[]): number[] {
  return sheets.filter((s) => picked.includes(s.id)).map((s) => s.id);
}

/** The sheets Add New copies the size of: the picked ones, else the active one. */
export function addSources(sheets: readonly Sheet[], picked: readonly number[]): Sheet[] {
  const chosen = sheets.filter((s) => picked.includes(s.id));
  if (chosen.length > 0) {
    return chosen;
  }
  const active = sheets.find((s) => s.active) ?? sheets[0];
  return active === undefined ? [] : [active];
}

/** True when deleting `picked` still leaves the job a sheet. */
export function canDelete(sheets: readonly Sheet[], picked: readonly number[]): boolean {
  const doomed = livePicks(sheets, picked).length;
  return doomed > 0 && doomed < sheets.length;
}

/** Why the picked sheets cannot be deleted, or undefined when they can. */
export function deleteWhy(sheets: readonly Sheet[], picked: readonly number[]): string | undefined {
  const doomed = livePicks(sheets, picked).length;
  if (doomed === 0) {
    return whyStrings.why.pickSheet;
  }
  return doomed < sheets.length ? undefined : strings.sheets.keepOne;
}

/** Vectors and components on a sheet. */
export function contentCount(sheet: Sheet): number {
  return (sheet.count ?? 0) + (sheet.components ?? 0);
}

/** The question to ask before deleting sheets, or null when they are empty. */
export function deletePrompt(doomed: readonly Sheet[]): string | null {
  const count = doomed.reduce((sum, s) => sum + contentCount(s), 0);
  if (count === 0) {
    return null;
  }
  const [only] = doomed;
  return doomed.length === 1 && only !== undefined
    ? strings.sheets.deleteConfirm(only.name, count)
    : strings.sheets.deleteManyConfirm(doomed.length, count);
}

/** "W x H x T unit" for a sheet row, or an empty string before the kernel says. */
export function sheetSize(sheet: Sheet, units: Units): string {
  const m = sheet.material;
  if (m === undefined) {
    return '';
  }
  return strings.sheets.size(
    formatLength(m.width, units),
    formatLength(m.height, units),
    formatLength(m.thickness, units),
    unitSuffix(units),
  );
}

export type SheetStep = 'next' | 'previous' | 'first' | 'last';

/** The sheet a key moves to from `id`, stopping at the ends; null when there is none. */
export function stepSheet(sheets: readonly Sheet[], id: number, step: SheetStep): number | null {
  if (sheets.length === 0) {
    return null;
  }
  const at = Math.max(
    0,
    sheets.findIndex((s) => s.id === id),
  );
  const last = sheets.length - 1;
  const index =
    step === 'first'
      ? 0
      : step === 'last'
        ? last
        : step === 'next'
          ? Math.min(at + 1, last)
          : Math.max(at - 1, 0);
  return sheets[index]?.id ?? null;
}

/** Maps a key to a step through the vertical list, or null for other keys. */
export function stepForKey(key: string): SheetStep | null {
  switch (key) {
    case 'ArrowDown':
      return 'next';
    case 'ArrowUp':
      return 'previous';
    case 'Home':
      return 'first';
    case 'End':
      return 'last';
    default:
      return null;
  }
}

export type SheetAction =
  'edit' | 'rename' | 'makeActive' | 'duplicate' | 'moveHere' | 'delete' | 'newSheet';

export interface SheetMenuContext {
  sheet: Sheet;
  sheetCount: number;
  /** Vectors and components selected on the active sheet. */
  selectedCount?: number;
}

export function buildSheetMenu({
  sheet,
  sheetCount,
  selectedCount = 0,
}: SheetMenuContext): PopupEntry<SheetAction>[] {
  const m = strings.sheets;
  return withIcons<SheetAction>(
    [
      { id: 'edit', label: m.edit, action: 'edit' },
      { id: 'rename', label: m.rename, action: 'rename' },
      { id: 'active', label: m.makeActive, disabled: sheet.active, action: 'makeActive' },
      { id: 'duplicate', label: m.duplicate, action: 'duplicate' },
      {
        id: 'moveHere',
        label: m.moveHere,
        disabled: sheet.active || selectedCount === 0,
        action: 'moveHere',
      },
      { separator: true },
      { id: 'delete', label: m.delete, disabled: sheetCount <= 1, action: 'delete' },
      { separator: true },
      { id: 'newSheet', label: m.newSheet, action: 'newSheet' },
    ],
    SHEET_ICONS,
  );
}
