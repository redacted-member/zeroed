// Pure state for the Merge Toolpaths form: every visible toolpath is
// merged (visibility is set in the toolpath list), in the quickest of the
// ticked orders, optionally part by part.

import type { MergeOptions, MergeOrder, ToolpathSummary } from '../../bridge/cam';

export interface MergeState {
  orders: MergeOrder[];
  byPart: boolean;
  name: string;
}

/** The orders in the form's order. */
export const MERGE_ORDERS: readonly MergeOrder[] = [
  'leftToRight',
  'bottomToTop',
  'grid',
  'shortestPath',
];

/** No order ticked and Merge By Part on opens. */
export function initialMergeState(): MergeState {
  return { orders: [], byPart: true, name: '' };
}

export function setOrder(state: MergeState, order: MergeOrder, on: boolean): MergeState {
  const rest = state.orders.filter((o) => o !== order);
  return {
    ...state,
    orders: MERGE_ORDERS.filter((o) => (o === order ? on : rest.includes(o))),
  };
}

/** The toolpaths the merge takes: the visible ones, in list order. */
export function mergedToolpaths(toolpaths: readonly ToolpathSummary[]): ToolpathSummary[] {
  return toolpaths.filter((t) => t.visible);
}

/** Whether the visible toolpaths name more than one tool. */
export function mixesTools(toolpaths: readonly ToolpathSummary[]): boolean {
  return new Set(mergedToolpaths(toolpaths).map((t) => t.toolName)).size > 1;
}

/** The cam.merge arguments, or null until two visible toolpaths share a tool. */
export function mergeRequest(
  state: MergeState,
  toolpaths: readonly ToolpathSummary[],
): { ids: number[]; name: string; options: MergeOptions } | null {
  const ids = mergedToolpaths(toolpaths).map((t) => t.id);
  if (ids.length < 2 || mixesTools(toolpaths)) {
    return null;
  }
  return {
    ids,
    name: state.name.trim(),
    options: { orders: state.orders, byPart: state.byPart },
  };
}
