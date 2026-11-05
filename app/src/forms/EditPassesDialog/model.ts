// Pure state for the Edit Passes dialog: the list of pass depths (measured
// from the Start Depth, the last always the Cut Depth), the selected pass,
// the Last Pass Thickness and the list utilities (tool pass depth, kept
// exact or not, or a number of passes).

import type { Units } from '../../bridge/types';
import { lengthText, lengthValue, numberValue } from '../../lib/formFields';
import { plannedDepths, type PassState } from '../../lib/passFields';

export type PassMethod = 'tool' | 'count';

export interface PassEditorState {
  /** Millimetres below the Start Depth, shallowest first; the last is the whole cut. */
  depths: number[];
  selected: number | null;
  depthText: string;
  /** The list was edited by hand rather than made by the utilities. */
  custom: boolean;
  useLastPass: boolean;
  lastText: string;
  method: PassMethod;
  exact: boolean;
  countText: string;
}

/** Depth of the whole cut and the tool's pass depth, in millimetres. */
export interface PassSpan {
  span: number;
  toolPassDepth: number;
}

function associated(state: PassEditorState, span: PassSpan, units: Units): number[] | null {
  const count = state.method === 'count' ? numberValue(state.countText) : null;
  if (state.method === 'count' && (count === null || count < 1 || !Number.isInteger(count))) {
    return null;
  }
  const last = state.useLastPass ? lengthValue(state.lastText, units) : null;
  if (state.useLastPass && (last === null || last <= 0)) {
    return null;
  }
  return plannedDepths(0, span.span, span.toolPassDepth, {
    passCount: count,
    lastPassThickness: last,
    exactPassDepth: state.exact,
    passList: null,
  });
}

export function openPassEditor(passes: PassState, span: PassSpan, units: Units): PassEditorState {
  const base: PassEditorState = {
    depths: [span.span],
    selected: null,
    depthText: '',
    custom: passes.list !== null,
    useLastPass: passes.useLastPass,
    lastText: passes.lastPassThickness,
    method: passes.byCount ? 'count' : 'tool',
    exact: passes.exact,
    countText: passes.passCount,
  };
  const depths =
    passes.list !== null
      ? plannedDepths(0, span.span, span.toolPassDepth, {
          passCount: null,
          lastPassThickness: null,
          exactPassDepth: false,
          passList: passes.list,
        })
      : associated(base, span, units);
  return { ...base, depths: depths ?? [span.span] };
}

export function selectPass(
  state: PassEditorState,
  index: number | null,
  units: Units,
): PassEditorState {
  const depth = index === null ? undefined : state.depths[index];
  return {
    ...state,
    selected: depth === undefined ? null : index,
    depthText: depth === undefined ? '' : lengthText(depth, units),
  };
}

function withDepths(
  state: PassEditorState,
  depths: number[],
  select: number | null,
  units: Units,
): PassEditorState {
  const sorted = [...depths].sort((a, b) => a - b);
  const index = select === null ? -1 : sorted.indexOf(select);
  return selectPass({ ...state, depths: sorted, custom: true }, index < 0 ? null : index, units);
}

/** Whether a depth can be a pass above the final one. */
function inside(depth: number, span: PassSpan, depths: readonly number[]): boolean {
  return (
    depth > 1e-9 && depth < span.span - 1e-9 && !depths.some((d) => Math.abs(d - depth) < 1e-6)
  );
}

/** Apply: moves the selected pass to the Depth box's value. The final pass stays on the cut depth. */
export function applyDepth(state: PassEditorState, span: PassSpan, units: Units): PassEditorState {
  const depth = lengthValue(state.depthText, units);
  const index = state.selected;
  if (depth === null || index === null || index >= state.depths.length - 1) {
    return state;
  }
  const others = state.depths.filter((_, i) => i !== index);
  if (!inside(depth, span, others)) {
    return state;
  }
  return withDepths(state, [...others, depth], depth, units);
}

/** Delete: removes the selected pass; the final pass cannot go. */
export function deletePass(state: PassEditorState, units: Units): PassEditorState {
  const index = state.selected;
  if (index === null || index >= state.depths.length - 1) {
    return state;
  }
  return withDepths(
    state,
    state.depths.filter((_, i) => i !== index),
    null,
    units,
  );
}

/** Clear All Passes: only the final pass on the cut depth is left. */
export function clearPasses(state: PassEditorState, span: PassSpan, units: Units): PassEditorState {
  return withDepths(state, [span.span], null, units);
}

/** A double click on the diagram: a new pass at that depth, selected. */
export function addPassAt(
  state: PassEditorState,
  depth: number,
  span: PassSpan,
  units: Units,
): PassEditorState {
  if (!inside(depth, span, state.depths)) {
    return state;
  }
  return withDepths(state, [...state.depths, depth], depth, units);
}

/**
 * The Last Pass Thickness settings changed: a hand made list moves the pass
 * above the final one so the last pass is that thick; otherwise the list is
 * made again by the utilities.
 */
export function setLastPass(
  state: PassEditorState,
  change: { useLastPass?: boolean; lastText?: string },
  span: PassSpan,
  units: Units,
): PassEditorState {
  const next = { ...state, ...change };
  if (!next.custom) {
    const depths = associated(next, span, units);
    return depths === null ? next : selectPass({ ...next, depths }, null, units);
  }
  const thickness = lengthValue(next.lastText, units);
  if (!next.useLastPass || thickness === null || thickness <= 0 || thickness >= span.span) {
    return next;
  }
  const above = span.span - thickness;
  const rest = next.depths.slice(0, -1).filter((d) => d < above - 1e-6);
  return withDepths(next, [...rest, above, span.span], null, units);
}

/** Set Associated Passes: the list made by the chosen utility, replacing any hand made passes. */
export function setAssociated(
  state: PassEditorState,
  span: PassSpan,
  units: Units,
): PassEditorState {
  const depths = associated(state, span, units);
  if (depths === null) {
    return state;
  }
  return selectPass({ ...state, depths, custom: false }, null, units);
}

/** The pass settings the form keeps after OK. */
export function editorResult(state: PassEditorState, previous: PassState): PassState {
  return {
    ...previous,
    byCount: state.method === 'count',
    passCount: state.countText,
    useLastPass: state.useLastPass,
    lastPassThickness: state.lastText,
    exact: state.exact,
    list: state.custom ? state.depths.slice(0, -1) : null,
  };
}

/** Where a depth sits on the diagram, from 0 at the start depth to 1 at the cut depth. */
export function diagramFraction(depth: number, span: PassSpan): number {
  return span.span > 0 ? Math.min(1, Math.max(0, depth / span.span)) : 0;
}

/** The pass nearest a point on the diagram, when it is within `reach` (a fraction of the cut). */
export function nearestPass(
  state: PassEditorState,
  fraction: number,
  span: PassSpan,
  reach = 0.04,
): number | null {
  let best: number | null = null;
  let gap = reach;
  state.depths.forEach((depth, i) => {
    const d = Math.abs(diagramFraction(depth, span) - fraction);
    if (d <= gap) {
      gap = d;
      best = i;
    }
  });
  return best;
}
