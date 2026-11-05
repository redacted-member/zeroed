// Editing keys inside text fields. In the desktop app the native menu owns
// Cmd+A, Cmd+Z, Shift+Cmd+Z, Cmd+Y and Delete as accelerators of Select
// All Vectors, Undo, Redo and Delete, so the keys reach the page as menu
// commands even while a text box has focus. These helpers turn such a
// command back into the text box's own edit.

import type { AppCommand } from './shortcuts';

/** What a focused element is, as far as text editing goes. */
export interface FieldInfo {
  tag: string;
  /** The input's type; empty for other elements. */
  type: string;
  readOnly: boolean;
  disabled: boolean;
  contentEditable: boolean;
}

/** Native edits a text box does itself. */
export type FieldEdit = 'selectAll' | 'undo' | 'redo' | 'forwardDelete';

const TEXT_INPUTS = new Set(['', 'text', 'search', 'number', 'email', 'url', 'tel', 'password']);

const EDITS: Partial<Record<AppCommand, FieldEdit>> = {
  'edit.selectAll': 'selectAll',
  'edit.undo': 'undo',
  'edit.redo': 'redo',
  'edit.delete': 'forwardDelete',
};

/** Whether `command` also edits a focused text box (its native accelerator
 * must stay live, even where the app command is greyed). */
export function editsFields(command: AppCommand): boolean {
  return EDITS[command] !== undefined;
}

/** Whether `field` takes typed text. */
export function isTextField(field: FieldInfo | null): boolean {
  if (field === null || field.disabled) {
    return false;
  }
  if (field.contentEditable) {
    return true;
  }
  const tag = field.tag.toLowerCase();
  return tag === 'textarea' || (tag === 'input' && TEXT_INPUTS.has(field.type.toLowerCase()));
}

/** The edit `command` means inside `field`, or null when it is the app's command. */
export function fieldEditFor(command: AppCommand, field: FieldInfo | null): FieldEdit | null {
  const edit = EDITS[command];
  if (edit === undefined || !isTextField(field)) {
    return null;
  }
  // A read-only box can still have its text selected.
  if (field?.readOnly === true && edit !== 'selectAll') {
    return null;
  }
  return edit;
}

function describe(element: Element | null): FieldInfo | null {
  if (!(element instanceof HTMLElement)) {
    return null;
  }
  const control =
    element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement ? element : null;
  return {
    tag: element.tagName,
    type: element instanceof HTMLInputElement ? element.type : '',
    readOnly: control?.readOnly ?? false,
    disabled: control?.disabled ?? false,
    contentEditable: element.isContentEditable,
  };
}

/**
 * Does `command` as the focused text box's own edit when a text box has
 * focus. Answers whether it did, so the caller leaves the app command alone.
 */
export function runFieldEdit(command: AppCommand): boolean {
  if (typeof document === 'undefined') {
    return false;
  }
  const focused = document.activeElement;
  const edit = fieldEditFor(command, describe(focused));
  if (edit === null) {
    return false;
  }
  if (
    edit === 'selectAll' &&
    (focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement)
  ) {
    focused.select();
    return true;
  }
  // execCommand is deprecated but remains the only way to reach the text
  // box's own undo stack and editing actions.
  document.execCommand(edit);
  return true;
}
