// A tool's naming variables as a sorted list, the way the kernel keeps them.

import type { ToolVariable } from '../../bridge/cam';

/** The list with `name` set to `value`, replacing one of the same name, sorted by name. */
export function withVariable(list: ToolVariable[], name: string, value: string): ToolVariable[] {
  const trimmed = name.trim();
  return [...list.filter((v) => v.name !== trimmed), { name: trimmed, value }].sort((a, b) =>
    a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
  );
}

/** The list without the variable `name`. */
export function withoutVariable(list: ToolVariable[], name: string): ToolVariable[] {
  return list.filter((v) => v.name !== name);
}
