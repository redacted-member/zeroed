// How the preview shown was made, so running the animation to an issue can
// make the same preview again: the run into fresh material, each Preview
// Selected Toolpath run cut into it after, and the waste removed between.
// To reach an issue the steps before the last run that cut its toolpath are
// made at once, that run is animated to the issue, and the steps after it
// wait until the animation finishes.

import type { PreviewQuality } from '../../bridge/cam';

/** A preview run: the toolpaths asked for and whether it cut into the
 * preview shown. */
export interface RunStep {
  kind: 'run';
  ids?: number[];
  all?: boolean;
  keep: boolean;
}

/** A piece of waste removed at job point (x, y). */
export interface WasteStep {
  kind: 'waste';
  x: number;
  y: number;
}

export type PreviewStep = RunStep | WasteStep;

/** The steps that made the preview shown, first first, at one quality. */
export interface PreviewHistory {
  quality: PreviewQuality;
  steps: PreviewStep[];
}

/**
 * The history after a run. A run kept on the preview shown (at the same
 * quality, which is when the kernel cuts into it) adds to the history;
 * any other starts it again.
 */
export function recordRun(
  history: PreviewHistory | null,
  quality: PreviewQuality,
  run: Omit<RunStep, 'kind'>,
  shown: boolean,
): PreviewHistory {
  const step: RunStep = { kind: 'run', ...run };
  if (run.keep && shown && history !== null && history.quality === quality) {
    return { quality, steps: [...history.steps, step] };
  }
  return { quality, steps: [{ ...step, keep: false }] };
}

/** The history after a piece of waste was removed. */
export function recordWaste(
  history: PreviewHistory | null,
  x: number,
  y: number,
): PreviewHistory | null {
  return history === null
    ? null
    : { ...history, steps: [...history.steps, { kind: 'waste', x, y }] };
}

/** The history after Undo Last took back its last step. */
export function undoStep(history: PreviewHistory | null): PreviewHistory | null {
  if (history === null || history.steps.length <= 1) {
    return null;
  }
  return { ...history, steps: history.steps.slice(0, -1) };
}

/** Whether a run cut toolpath `id` (a run of the visible toolpaths is
 * taken to have). */
function cut(step: PreviewStep, id: number): step is RunStep {
  return step.kind === 'run' && (step.ids === undefined || step.ids.includes(id));
}

/** How to make the preview again up to where toolpath `id` cuts. */
export interface ReplayPlan {
  /** Made at once first: the first into fresh material, the rest kept. */
  before: PreviewStep[];
  /** Animated to the issue, kept on `before` when there is any. */
  run: RunStep;
  /** Made at once when the animation finishes. */
  after: PreviewStep[];
}

/**
 * The plan for running the animation to an issue of toolpath `id`: the
 * last run that cut it is the one animated, or the last run of all when
 * none did. Null without a history.
 */
export function replayPlan(history: PreviewHistory | null, id: number): ReplayPlan | null {
  if (history === null) {
    return null;
  }
  const { steps } = history;
  let at = -1;
  for (let k = steps.length - 1; k >= 0 && at < 0; k -= 1) {
    const step = steps[k];
    if (step !== undefined && cut(step, id)) {
      at = k;
    }
  }
  if (at < 0) {
    for (let k = steps.length - 1; k >= 0 && at < 0; k -= 1) {
      if (steps[k]?.kind === 'run') {
        at = k;
      }
    }
  }
  const run = steps[at];
  if (run === undefined || run.kind !== 'run') {
    return null;
  }
  return { before: steps.slice(0, at), run, after: steps.slice(at + 1) };
}
