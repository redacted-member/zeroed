// Pure list operations of the ordered tool list (ToolListField).

import type { Tool } from '../../bridge/cam';

/** The list with `tool` added at the end, or `tool` replacing the same database tool. */
export function addTool(tools: readonly Tool[], tool: Tool): { tools: Tool[]; index: number } {
  const at = tools.findIndex((t) => t.id === tool.id);
  if (at >= 0) {
    return { tools: tools.map((t, i) => (i === at ? tool : t)), index: at };
  }
  return { tools: [...tools, tool], index: tools.length };
}

/** The list without the tool at `index`, and the row to select after it. */
export function removeTool(
  tools: readonly Tool[],
  index: number,
): { tools: Tool[]; index: number } {
  const next = tools.filter((_, i) => i !== index);
  return { tools: next, index: Math.min(index, next.length - 1) };
}

/** The list with the tool at `index` moved one place up (-1) or down (1). */
export function moveTool(
  tools: readonly Tool[],
  index: number,
  by: -1 | 1,
): { tools: Tool[]; index: number } {
  const to = index + by;
  if (index < 0 || index >= tools.length || to < 0 || to >= tools.length) {
    return { tools: [...tools], index };
  }
  const next = [...tools];
  const [moved] = next.splice(index, 1);
  if (moved !== undefined) {
    next.splice(to, 0, moved);
  }
  return { tools: next, index: to };
}

/** The list with the tool at `index` replaced by its edited copy. */
export function replaceTool(tools: readonly Tool[], index: number, tool: Tool): Tool[] {
  return tools.map((t, i) => (i === index ? tool : t));
}
