// The toolpath preview: the cut material heightfield the 3D view draws, and
// what the last run found. A document change leaves the preview up but
// marks it out of date until it is run again or reset.
//
// With Animate on, the kernel cuts the stock a little at a time: each frame
// moves the tool on, fetches the changed cells and patches them into the
// shown heightfield, with the tool drawn where it is. The animation can be
// paused, stepped a move at a time, skipped to the end of the toolpath or
// run to the end.
//
// The animation reports what the tool met (rapids through material, the
// shank or holder in the stock, cutting deeper than the flutes, plunges a
// tool cannot make, clamps); once a preview is complete the kernel compares
// it with the design (gouges, leftover, undercuts). Picking an issue frames
// it in the 3D view and runs the animation to it, making the preview shown
// again the way it was made (a Preview Selected Toolpath run cut into an
// earlier preview, waste removed between): see replay.ts.

import { create } from 'zustand';

import { api } from '../bridge/api';
import type {
  Collision,
  Heightfield,
  PreviewQuality,
  SimCheck,
  SimRect,
  SimStartReply,
  SimStepArgs,
} from '../bridge/cam';
import {
  DEFAULT_TOLERANCE,
  issueBox,
  seekArgs,
  toleranceValue,
  type PreviewIssue,
} from '../forms/PreviewToolpathsForm/issues';
import {
  recordRun,
  recordWaste,
  replayPlan,
  undoStep,
  type PreviewHistory,
  type PreviewStep,
} from '../forms/PreviewToolpathsForm/replay';
import {
  applyPatch,
  FRAME_MS,
  nextStepCost,
  paceDistance,
  type FillMode,
} from '../forms/PreviewToolpathsForm/model';
import { isRotary } from '../lib/rotary';
import { shownJob } from '../views/View3D/viewOptions';
import { useDocumentStore } from './documentStore';
import { useModelStore } from './modelStore';
import { usePrefsStore } from './prefsStore';
import { useUiStore } from './uiStore';
import { useView3dStore } from './view3dStore';

/** The tool as the animation draws it. */
export interface PreviewTool {
  x: number;
  y: number;
  z: number;
  /** [radius, height above the tip] pairs, tip first. */
  outline: Array<[number, number]>;
  /** The shank as the checks see it: radius, bottom and top above the tip. */
  shank: [number, number, number] | null;
  /** The holder when it is checked: radius, bottom and top above the tip. */
  holder: [number, number, number] | null;
}

type ToolParts = Pick<PreviewTool, 'outline' | 'shank' | 'holder'>;

/**
 * What the last animation step changed: `field` is `base` with only the
 * cells in the rectangles rewritten, so a view showing `base` can update
 * just those cells.
 */
export interface PatchedCells {
  base: Heightfield;
  field: Heightfield;
  rects: SimRect[];
}

interface PreviewState {
  heightfield: Heightfield | null;
  /** The cells the last animation step changed, or null. */
  patched: PatchedCells | null;
  quality: PreviewQuality;
  /** Calculating a preview that is not animated, or starting one. */
  running: boolean;
  outOfDate: boolean;
  /**
   * The model changed since this preview was made (a component created,
   * edited or removed), so the 3D view shows the model in its place until
   * the preview runs again; the preview itself is kept for the form.
   */
  modelChanged: boolean;
  collisions: Collision[];
  /** What the check of the finished preview found, or null. */
  check: SimCheck | null;
  /** The check is under way. */
  checking: boolean;
  /** Paint gouges, leftover and undercuts on the finished preview. */
  showDeviation: boolean;
  /** How far off the design the check lets the preview be, mm. */
  tolerance: number;
  /** Key of the issue last picked in the list. */
  selectedIssue: string | null;
  seconds: number;
  /** Show the material being removed as the tool moves. */
  animate: boolean;
  /** Draw the tool during the animation. */
  drawTool: boolean;
  /** Animation speed on the slider, SPEED_MIN to SPEED_MAX. */
  speed: number;
  /** An animation is under way (playing or paused). */
  animating: boolean;
  /** The animation is moving on by itself. */
  playing: boolean;
  /** Fraction of the animation cut, 0 to 1. */
  progress: number;
  tool: PreviewTool | null;
  /** How the machined areas are painted. */
  fill: FillMode;
  /** The colour of the machined areas with a global fill. */
  fillColour: string;
  /** Colours chosen per toolpath id; 'none' for no fill. */
  toolpathColours: Record<number, string>;
  /** The colour the uncut surface is painted with Surface Color. */
  surfaceColour: string;
  /** Undo Last has a preview or waste removal to take back. */
  canUndo: boolean;
  /** Shade the preview as a lithophane, lit from behind. */
  lithophane: boolean;
  /** How fast the light falls off through the material, 0 to 100. */
  lithoStrength: number;
  setLithophane: (lithophane: boolean) => void;
  setLithoStrength: (strength: number) => void;
  setQuality: (quality: PreviewQuality) => void;
  setAnimate: (animate: boolean) => void;
  setDrawTool: (drawTool: boolean) => void;
  setSpeed: (speed: number) => void;
  setFill: (fill: FillMode) => void;
  setFillColour: (colour: string) => void;
  setToolpathColour: (id: number, colour: string) => void;
  /** Set All: every one of `ids` takes `colour`. */
  setAllToolpathColours: (ids: readonly number[], colour: string) => void;
  setSurfaceColour: (colour: string) => void;
  /** Previews the given toolpaths, every one, or the visible ones. Shows the
   * 3D view. `keep` cuts into the preview shown (Preview Selected Toolpath). */
  run: (ids?: number[], all?: boolean, keep?: boolean) => Promise<void>;
  /** Undo Last: back to the preview before the last preview or waste removal. */
  undoLast: () => Promise<void>;
  /** Reads the preview again after the other side is shown: the kernel has
   * turned it over to be seen from that side. */
  sideShown: () => Promise<void>;
  /** Removes the piece of waste cut free under job point (x, y). */
  removeWaste: (x: number, y: number) => Promise<void>;
  pause: () => void;
  resume: () => void;
  /** Cuts the rest of the move under way. */
  stepMove: () => Promise<void>;
  /** Cuts the rest of the toolpath under way. */
  skipToolpath: () => Promise<void>;
  /** Cuts up to the next retract (Run to Retract). */
  runToRetract: () => Promise<void>;
  /** Cuts everything left at once. */
  finish: () => Promise<void>;
  reset: () => Promise<void>;
  setShowDeviation: (show: boolean) => void;
  /** Sets the tolerance and checks the preview again. */
  setTolerance: (mm: number) => void;
  /** Compares the finished preview with the design. */
  runCheck: () => Promise<void>;
  /** Frames an issue and runs the animation to it, paused there. */
  goToIssue: (issue: PreviewIssue) => Promise<void>;
}

/** Tool outlines by toolpath id for the animation under way. */
let outlines = new Map<number, ToolParts>();
/** How the preview shown was made, so an issue can be run to again. */
let history: PreviewHistory | null = null;
/** The steps of the preview left to make when the animation run to an
 * issue finishes, at the preview's quality. */
let pending: { quality: PreviewQuality; steps: PreviewStep[] } | null = null;

/** Makes `steps` of a preview at once, the first into fresh material
 * unless `kept`. */
async function makeSteps(
  quality: PreviewQuality,
  steps: readonly PreviewStep[],
  kept: boolean,
): Promise<void> {
  let keep = kept;
  for (const step of steps) {
    if (step.kind === 'run') {
      await api.sim.run(quality, step.ids, keep);
    } else {
      await api.sim.removeWaste(step.x, step.y);
    }
    keep = true;
  }
}

function toolParts(started: SimStartReply): Map<number, ToolParts> {
  return new Map(
    started.toolpaths.map((t) => [
      t.id,
      { outline: t.tool, shank: t.shank ?? null, holder: t.holder ?? null },
    ]),
  );
}

/** `found` without the ones already in `known` (the same move and kind). */
function newCollisions(known: readonly Collision[], found: readonly Collision[]): Collision[] {
  const seen = new Set(known.map((c) => `${c.toolpathId}:${c.moveIndex}:${c.kind}`));
  return found.filter((c) => !seen.has(`${c.toolpathId}:${c.moveIndex}:${c.kind}`));
}
/** Bumped to stop an animation loop that is no longer wanted. */
let generation = 0;

function reportError(error: unknown): void {
  useDocumentStore.setState({
    lastError: error instanceof Error ? error.message : String(error),
  });
}

function showIn3d(): void {
  const ui = useUiStore.getState();
  if (ui.viewMode === '2d') {
    ui.setViewMode('3d');
  }
}

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export const usePreviewStore = create<PreviewState>()((set, get) => {
  /** Moves the animation on and shows the result. Answers true when it finished. */
  const advance = async (args: SimStepArgs): Promise<boolean> => {
    const { step: reply, patch } = await api.sim.advance(args);
    const field = get().heightfield;
    const next = field === null ? null : applyPatch(field, patch);
    const parts = reply.tool === null ? undefined : outlines.get(reply.tool.toolpathId);
    const fresh = newCollisions(get().collisions, reply.collisions);
    set({
      heightfield: next,
      patched:
        field === null || next === null || next === field
          ? null
          : { base: field, field: next, rects: patch.rects },
      progress: reply.progress,
      seconds: reply.seconds,
      collisions: fresh.length > 0 ? [...get().collisions, ...fresh] : get().collisions,
      tool:
        reply.done || reply.tool === null || parts === undefined
          ? null
          : { x: reply.tool.x, y: reply.tool.y, z: reply.tool.z, ...parts },
    });
    if (reply.done) {
      set({ animating: false, playing: false, tool: null });
      void settle();
    }
    return reply.done;
  };

  /** Once an animation finishes: the rest of a preview made again, then
   * the check. */
  const settle = async (): Promise<void> => {
    const rest = pending;
    pending = null;
    if (rest !== null && rest.steps.length > 0) {
      const token = generation;
      try {
        await makeSteps(rest.quality, rest.steps, true);
        const heightfield = await api.sim.heightfield();
        if (token !== generation) {
          return;
        }
        set({ heightfield, modelChanged: false, patched: null, canUndo: true });
      } catch (error) {
        reportError(error);
        return;
      }
    }
    await runCheck();
  };

  /** Checks the finished preview against the design; a newer run wins. */
  let checks = 0;
  const runCheck = async (): Promise<void> => {
    if (get().heightfield === null || get().animating) {
      return;
    }
    checks += 1;
    const mine = checks;
    set({ checking: true });
    try {
      const check = await api.sim.check(get().tolerance);
      if (mine === checks && !get().animating) {
        set({ check });
      }
    } catch (error) {
      reportError(error);
    } finally {
      if (mine === checks) {
        set({ checking: false });
      }
    }
  };

  // A step every display frame, each moving the tool as far as the time
  // since the last one says, so the tool moves evenly at the chosen speed.
  // Where the kernel cannot cut that far within a frame (the finest preview
  // qualities) the steps shrink to what it can, so frames keep coming.
  const loop = async (token: number) => {
    let last = performance.now() - FRAME_MS;
    let cost: number | null = null;
    while (token === generation && get().playing) {
      const now = performance.now();
      const distance = paceDistance(get().speed, now - last, cost);
      last = now;
      try {
        const done = await advance({ distance });
        cost = nextStepCost(cost, performance.now() - now, distance);
        if (done) {
          return;
        }
      } catch (error) {
        set({ playing: false });
        reportError(error);
        return;
      }
      const left = FRAME_MS - (performance.now() - now);
      if (left > 1) {
        await wait(left);
      }
    }
  };

  /** A manual step while paused. */
  const manual = async (args: SimStepArgs) => {
    if (!get().animating || get().playing) {
      return;
    }
    try {
      await advance(args);
    } catch (error) {
      reportError(error);
    }
  };

  return {
    heightfield: null,
    patched: null,
    quality: 'standard',
    running: false,
    outOfDate: false,
    modelChanged: false,
    collisions: [],
    check: null,
    checking: false,
    showDeviation: true,
    tolerance: DEFAULT_TOLERANCE,
    selectedIssue: null,
    seconds: 0,
    animate: false,
    drawTool: true,
    speed: 5,
    animating: false,
    playing: false,
    progress: 0,
    tool: null,
    fill: 'material',
    fillColour: '#d9534f',
    toolpathColours: {},
    surfaceColour: '#3f6fb5',
    canUndo: false,
    lithophane: false,
    lithoStrength: 50,

    setLithophane: (lithophane) => set({ lithophane }),
    setLithoStrength: (strength) =>
      set({ lithoStrength: Math.min(Math.max(Math.round(strength), 0), 100) }),
    setQuality: (quality) => set({ quality }),
    setAnimate: (animate) => set({ animate }),
    setDrawTool: (drawTool) => set({ drawTool }),
    setSpeed: (speed) => set({ speed }),
    setFill: (fill) => set({ fill }),
    setFillColour: (fillColour) => set({ fillColour }),
    setToolpathColour: (id, colour) =>
      set({ toolpathColours: { ...get().toolpathColours, [id]: colour } }),
    setAllToolpathColours: (ids, colour) =>
      set({
        toolpathColours: {
          ...get().toolpathColours,
          ...Object.fromEntries(ids.map((id) => [id, colour])),
        },
      }),
    setSurfaceColour: (surfaceColour) => set({ surfaceColour }),

    run: async (ids, all, keep) => {
      generation += 1;
      const token = generation;
      set({
        running: true,
        playing: false,
        animating: false,
        tool: null,
        check: null,
        selectedIssue: null,
      });
      history = recordRun(
        history,
        get().quality,
        { ids, all, keep: keep === true },
        get().heightfield !== null,
      );
      pending = null;
      try {
        if (get().animate) {
          const started = await api.sim.start(get().quality, ids, all, keep);
          outlines = toolParts(started);
          const heightfield = await api.sim.heightfield();
          if (token !== generation) {
            return;
          }
          set({
            heightfield,
            modelChanged: false,
            collisions: [],
            seconds: 0,
            progress: 0,
            outOfDate: false,
            animating: true,
            playing: true,
            canUndo: true,
          });
          useDocumentStore.setState({ lastError: null });
          showIn3d();
          void loop(token);
        } else {
          const reply = await api.sim.run(get().quality, ids, keep);
          const heightfield = await api.sim.heightfield();
          set({
            heightfield,
            modelChanged: false,
            collisions: reply.collisions,
            seconds: reply.seconds,
            progress: 1,
            outOfDate: false,
            canUndo: true,
          });
          useDocumentStore.setState({ lastError: null });
          showIn3d();
          void runCheck();
        }
      } catch (error) {
        reportError(error);
      } finally {
        set({ running: false });
      }
    },

    pause: () => set({ playing: false }),

    resume: () => {
      if (!get().animating || get().playing) {
        return;
      }
      generation += 1;
      set({ playing: true });
      void loop(generation);
    },

    stepMove: () => manual({ oneMove: true }),
    skipToolpath: () => manual({ toToolpathEnd: true }),
    runToRetract: () => manual({ toRetract: true }),

    finish: async () => {
      if (!get().animating) {
        return;
      }
      generation += 1;
      set({ playing: false });
      try {
        await advance({ toEnd: true });
      } catch (error) {
        reportError(error);
      }
    },

    sideShown: async () => {
      if (get().heightfield === null || get().running || get().animating) {
        return;
      }
      try {
        const heightfield = await api.sim.heightfield();
        set({ heightfield, modelChanged: false, patched: null, tool: null, check: null });
        void runCheck();
      } catch (error) {
        reportError(error);
      }
    },

    undoLast: async () => {
      if (get().running || get().animating) {
        return;
      }
      try {
        const reply = await api.sim.undo();
        const heightfield = reply.empty ? null : await api.sim.heightfield();
        if (reply.restored) {
          history = heightfield === null ? null : undoStep(history);
        }
        set({
          heightfield,
          modelChanged: false,
          patched: null,
          canUndo: false,
          tool: null,
          check: null,
        });
        if (heightfield === null) {
          set({ collisions: [], seconds: 0, progress: 0, outOfDate: false });
        } else {
          void runCheck();
        }
      } catch (error) {
        reportError(error);
      }
    },

    removeWaste: async (x, y) => {
      if (get().heightfield === null || get().running || get().animating) {
        return;
      }
      try {
        const reply = await api.sim.removeWaste(x, y);
        if (reply.removed > 0) {
          history = recordWaste(history, x, y);
          const heightfield = await api.sim.heightfield();
          set({ heightfield, modelChanged: false, patched: null, canUndo: true });
          void runCheck();
        }
      } catch (error) {
        reportError(error);
      }
    },

    reset: async () => {
      generation += 1;
      try {
        await api.sim.reset();
      } catch {
        // Nothing to reset is fine.
      }
      history = null;
      pending = null;
      set({
        heightfield: null,
        collisions: [],
        check: null,
        selectedIssue: null,
        seconds: 0,
        outOfDate: false,
        modelChanged: false,
        animating: false,
        playing: false,
        progress: 0,
        tool: null,
        canUndo: false,
      });
    },

    setShowDeviation: (showDeviation) => set({ showDeviation }),

    setTolerance: (mm) => {
      const tolerance = toleranceValue(mm);
      if (tolerance === null || tolerance === get().tolerance) {
        return;
      }
      set({ tolerance });
      void runCheck();
    },

    runCheck,

    goToIssue: async (issue) => {
      set({ selectedIssue: issue.key });
      const parts = outlines.get(issue.toolpathId);
      const radius = parts === undefined ? 0 : Math.max(0, ...parts.outline.map(([r]) => r));
      const field = get().heightfield;
      const top = field?.top ?? issue.z;
      // A rotary job shown wrapped frames the issue where it lies on the blank.
      const job = shownJob(
        useDocumentStore.getState().job,
        usePrefsStore.getState().prefs.view3d.autoWrap,
      );
      // The wrapping maths loads with the 3D view, not at start.
      const wrapped =
        field !== null && isRotary(job)
          ? (await import('../views/View3D/wrappedMesh')).wrappedScenePoint(job, field.top, issue)
          : null;
      useView3dStore.getState().requestFrame(issueBox(issue, radius, top, wrapped), top);
      if (get().running) {
        return;
      }
      // The same preview made again up to the run that cut the issue's
      // toolpath, then that run animated to the issue.
      generation += 1;
      const token = generation;
      set({ playing: false, running: true });
      try {
        const quality = history?.quality ?? get().quality;
        const plan = replayPlan(history, issue.toolpathId) ?? {
          before: [],
          run: { kind: 'run', keep: false },
          after: [],
        };
        await makeSteps(quality, plan.before, false);
        const kept = plan.before.length > 0;
        const started = await api.sim.start(quality, plan.run.ids, plan.run.all, kept);
        outlines = toolParts(started);
        const heightfield = await api.sim.heightfield();
        if (token !== generation) {
          return;
        }
        pending = { quality, steps: plan.after };
        set({
          heightfield,
          modelChanged: false,
          patched: null,
          animating: true,
          playing: false,
          progress: 0,
          outOfDate: false,
          // Undo Last takes back the preview's last step once it is made
          // again; a preview of one run has none before it to go back to.
          canUndo: kept || plan.after.length > 0,
        });
        await advance(seekArgs(issue));
        showIn3d();
      } catch (error) {
        reportError(error);
      } finally {
        set({ running: false });
      }
    },
  };
});

/** The preview the 3D view draws: none while the model has moved on. */
export function shownPreview(state: Pick<PreviewState, 'heightfield' | 'modelChanged'>) {
  return state.modelChanged ? null : state.heightfield;
}

useModelStore.subscribe((state, previous) => {
  if (state.stamp !== previous.stamp && usePreviewStore.getState().heightfield !== null) {
    usePreviewStore.setState({ modelChanged: true });
  }
});

useDocumentStore.subscribe((state, previous) => {
  if (usePreviewStore.getState().heightfield === null) {
    return;
  }
  if (state.activeSide !== previous.activeSide) {
    // Showing the other side is not an edit: the preview turns over with it.
    void usePreviewStore.getState().sideShown();
  } else if (state.revision !== previous.revision) {
    usePreviewStore.setState({ outOfDate: true });
  }
});
