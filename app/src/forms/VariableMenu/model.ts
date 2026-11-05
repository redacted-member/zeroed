// The right click menu of a numeric field: the document variables to
// insert, and a new one to create and insert.

import type { PopupEntry } from '../../views/PopupMenu/model';
import { strings } from '../../lib/formStrings';

export type VariableAction = { insert: string } | { create: true };

/** The menu entries for the current variables. */
export function variableMenu(names: readonly string[]): PopupEntry<VariableAction>[] {
  const text = strings.documentVariables;
  const sorted = [...names].sort((a, b) => a.localeCompare(b));
  const items: PopupEntry<VariableAction>[] =
    sorted.length === 0
      ? [{ id: 'none', label: text.menuNone, disabled: true, info: true, action: { insert: '' } }]
      : sorted.map((name) => ({
          id: `v-${name}`,
          label: text.menuInsert(name),
          icon: 'variables',
          action: { insert: name },
        }));
  return [
    ...items,
    { separator: true },
    { id: 'new', label: text.menuNew, icon: 'variableNew', action: { create: true } },
  ];
}

/** The field text with `{name}` put in place of the selection. */
export function insertVariable(
  value: string,
  name: string,
  start: number | null,
  end: number | null,
): { text: string; caret: number } {
  const token = `{${name}}`;
  const from = Math.max(0, Math.min(start ?? value.length, value.length));
  const to = Math.max(from, Math.min(end ?? from, value.length));
  return { text: value.slice(0, from) + token + value.slice(to), caret: from + token.length };
}
