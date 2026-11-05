// The common options of the modeling tools: the component's name and how
// it combines, and how Apply, Reset and Start New Component track the
// component the form is making.

import type { AppliedComponent, ApplyTarget, Combine, ComponentSummary } from '../../bridge/model';
import { strings } from '../../lib/formStrings';

/** Combine modes the creation tools offer (Low and Multiply are set in the
 * Component Tree afterwards). */
export const MODELING_COMBINES: ReadonlyArray<{ value: Combine; label: string }> = [
  { value: 'add', label: strings.model.combine.add },
  { value: 'subtract', label: strings.model.combine.subtract },
  { value: 'mergeHigh', label: strings.model.combine.mergeHigh },
];

/** What the form's Apply works on. */
export interface ApplyTrack {
  /** The component the last Apply made, or the one being edited. */
  target: number | null;
  /** The component the form opened to edit, which Reset goes back to. */
  editing: number | null;
  /** Highest point of the last Apply's own heights, mm. */
  shapeHeight: number | null;
  /** Highest point of the last Apply, mm, with the base height. */
  totalHeight: number | null;
  /** The edited component's own heights when the form opened. */
  start: { shapeHeight: number | null; totalHeight: number | null };
}

const NO_HEIGHTS = { shapeHeight: null, totalHeight: null };

/** The heights an edited component shows before any Apply: its shape and
 * total height (shape plus base), unknown when a fade or tilt changes its
 * top. */
export function componentHeights(component: ComponentSummary | undefined): ApplyTrack['start'] {
  if (
    component === undefined ||
    component.maxHeight === null ||
    (component.fade ?? null) !== null ||
    (component.tilt ?? null) !== null
  ) {
    return NO_HEIGHTS;
  }
  const base = component.baseHeight ?? 0;
  const scale = component.heightScale > 0 ? component.heightScale : 1;
  return { shapeHeight: (component.maxHeight - base) / scale, totalHeight: component.maxHeight };
}

export function startTrack(editing: number | null, component?: ComponentSummary): ApplyTrack {
  const start = editing === null ? NO_HEIGHTS : componentHeights(component);
  return { target: editing, editing, ...start, start };
}

/** The `target` and `replace` of the next Apply. */
export function applyTarget(track: ApplyTrack, replace: number | undefined): ApplyTarget {
  return {
    ...(track.target === null ? {} : { target: track.target }),
    ...(replace === undefined ? {} : { replace }),
  };
}

export function afterApply(track: ApplyTrack, reply: AppliedComponent): ApplyTrack {
  return {
    ...track,
    target: reply.id,
    shapeHeight: reply.shapeHeight,
    totalHeight: reply.totalHeight,
  };
}

/** Reset took the form's steps back: an edited component is as it was,
 * a new one is gone. */
export function afterReset(track: ApplyTrack): ApplyTrack {
  return { ...track, target: track.editing, ...track.start };
}

/** Start New Component: the next Apply makes another component. */
export function afterStartNew(): ApplyTrack {
  return startTrack(null);
}
