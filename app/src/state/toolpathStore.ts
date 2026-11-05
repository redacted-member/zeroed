// Mirror of the document's toolpath list and the preview moves the 2D view
// draws. Refreshes whenever the document revision moves, which every
// toolpath command (and undo) does.

import { create } from 'zustand';

import { api, type MissingLayersChoice } from '../bridge/api';
import type { IntersectionCheck } from '../bridge/prefs';
import type { JobSetup, KernelWarning } from '../bridge/types';
import { depthsMayChange, depthWarning } from '../lib/depthCheck';
import {
  crossingIds,
  crossingOutcome,
  crossingQuestion,
  type CheckPurpose,
} from '../lib/intersectionCheck';
import {
  gatherWarnings,
  toolpathName,
  type CalculationWarnings,
  type ToolpathWarning,
} from '../lib/toolpathWarnings';
import { strings } from '../lib/strings';
import type {
  ArrayCopySpec,
  MovesReply,
  Tiling,
  TilingReply,
  ToolpathGroup,
  ToolpathRequest,
  ToolpathSummary,
  MergeOptions,
} from '../bridge/cam';
import { useDialogStore } from './dialogStore';
import { useDocumentStore } from './documentStore';
import { usePrefsStore } from './prefsStore';
import { useUiStore } from './uiStore';

interface ToolpathState {
  toolpaths: ToolpathSummary[];
  /** Folders in the list. */
  groups: ToolpathGroup[];
  /** The row picked in the toolpath list. */
  selectedId: number | null;
  /** The group row picked in the list; new toolpaths join it when
   * Append new toolpaths to selected group is on. */
  selectedGroupId: number | null;
  /** Picks a group row (null for none), leaving no toolpath picked. */
  selectGroup: (id: number | null) => void;
  moves: MovesReply | null;
  /** Toolpath Tiling settings and tiles; null until read. */
  tiling: TilingReply | null;
  /** Applies Toolpath Tiling settings (one undo step). Answers success. */
  setTiling: (tiling: Tiling) => Promise<boolean>;
  /** Picks the tile the views show, or where they draw it. */
  viewTile: (view: { active?: number; originalPosition?: boolean }) => Promise<void>;
  /** Warnings from the last calculation, shown in the Toolpath Warnings
   * dialog when there are any. */
  warnings: ToolpathWarning[];
  /** Toolpaths the Save Toolpaths form should start with ticked, when it
   * was opened for particular ones; null means every visible toolpath. */
  saveChoice: number[] | null;
  setSaveChoice: (ids: number[] | null) => void;
  refresh: () => Promise<void>;
  select: (id: number | null) => void;
  /** Calculates a new toolpath, or recalculates `id` when given. Answers success. */
  calculate: (request: ToolpathRequest, id?: number) => Promise<boolean>;
  recalculateStale: () => Promise<void>;
  /** After a job or material change: recalculates the toolpaths it made
   * stale, first asking when Ask before recalculating toolpaths is on, then
   * reports toolpaths that now cut further below the material than they
   * were set to. `before` is the job as it was: a change that cannot change
   * depths (XY origin, size) skips the depth check. */
  followJobChange: (before?: JobSetup) => Promise<void>;
  /** Recalculates one toolpath with its stored settings. */
  recalculate: (id: number) => Promise<void>;
  /** Calculates a copy of a toolpath under `name`. Answers success. */
  duplicate: (id: number, name: string) => Promise<boolean>;
  remove: (id: number) => Promise<void>;
  rename: (id: number, name: string) => Promise<void>;
  setVisible: (id: number, visible: boolean) => Promise<void>;
  /** Moves a toolpath one place up (-1) or down (1) the list. */
  move: (id: number, by: -1 | 1) => Promise<void>;
  /** Shows or hides several toolpaths at once. */
  setVisibleMany: (ids: number[], visible: boolean) => Promise<void>;
  removeMany: (ids: number[]) => Promise<void>;
  /** Recalculates each toolpath with its stored settings. */
  recalculateMany: (ids: number[]) => Promise<void>;
  /** Makes a group holding `ids` (none for an empty group). */
  createGroup: (ids: number[]) => Promise<void>;
  updateGroup: (id: number, patch: { name?: string; expanded?: boolean }) => Promise<void>;
  deleteGroup: (id: number, deleteToolpaths: boolean) => Promise<void>;
  /** Moves toolpaths into `group` (0 for none) in front of `before`, or at its end. */
  place: (ids: number[], group: number, before?: number) => Promise<void>;
  setSheet: (ids: number[], sheet: number) => Promise<void>;
  saveTemplate: (path: string) => Promise<boolean>;
  /** Merges same tool toolpaths into a new one. Answers success. */
  merge: (ids: number[], name: string, options?: MergeOptions) => Promise<boolean>;
  /** Array Copy Toolpath of the given toolpaths. Answers success. */
  arrayCopy: (ids: number[], spec: ArrayCopySpec, name: string) => Promise<boolean>;
  /**
   * Applies a template. Without a choice, a template naming layers the job
   * lacks first asks what to do about them. Toolpaths left out become
   * warnings.
   */
  loadTemplate: (path: string, missing?: MissingLayersChoice) => Promise<boolean>;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * The crossing vectors check before `ids` are machined or offset, as
 * `setting` asks. Answers whether to go on; when not, the Vector Validator
 * may be open on the crossing vectors.
 */
export async function passesCrossingCheck(
  ids: number[],
  setting: IntersectionCheck,
  purpose: CheckPurpose = 'toolpath',
): Promise<boolean> {
  if (setting === 'skip' || setting === 'proceed' || ids.length === 0) {
    return true;
  }
  const { crossings } = await api.ops.crossings(ids).catch(() => ({ crossings: [] }));
  const outcome = await crossingOutcome(setting, crossings, (count) =>
    window.confirm(crossingQuestion(count, purpose)),
  );
  if (outcome === 'validator') {
    await useDocumentStore.getState().select(crossingIds(crossings));
    useUiStore.getState().openForm('validator');
  }
  return outcome === 'go';
}

export const useToolpathStore = create<ToolpathState>()((set, get) => {
  /** Runs a toolpath command, then refreshes the document and the list. */
  const run = async (action: () => Promise<unknown>): Promise<boolean> => {
    const doc = useDocumentStore.getState();
    let ok = true;
    try {
      await action();
      useDocumentStore.setState({ lastError: null });
    } catch (error) {
      useDocumentStore.setState({ lastError: message(error) });
      ok = false;
    }
    // The revision subscription refreshes too, but callers read the list
    // straight after, so wait for it here.
    await doc.refresh();
    await get().refresh();
    return ok;
  };

  /** Keeps what the calculations warned about and shows it, if anything. */
  const report = (results: readonly CalculationWarnings[]) => {
    const warnings = gatherWarnings(results);
    set({ warnings });
    if (warnings.length > 0) {
      useDialogStore.getState().show('toolpathWarnings');
    }
  };

  /** Reports the warnings of one calculation's reply, if it came. */
  const reportReply = async (reply: { id: number; warnings: KernelWarning[] } | undefined) => {
    if (reply === undefined) {
      report([]);
      return;
    }
    const { warningTexts } = await import('../lib/kernelWarnings');
    report([
      {
        toolpath: toolpathName(get().toolpaths, reply.id),
        warnings: warningTexts(reply.warnings),
      },
    ]);
  };

  /** Recalculates each toolpath with its stored settings and answers what
   * each warned about. A toolpath that fails leaves its error (named, when
   * there are several) rather than one cleared by the next success. */
  const recalculateEach = async (ids: readonly number[]): Promise<CalculationWarnings[]> => {
    const before = get().toolpaths;
    const replies: { id: number; warnings: KernelWarning[] }[] = [];
    const failures: string[] = [];
    // One command, one undo step, however many toolpaths it recalculates.
    const grouped = ids.length > 1;
    if (grouped) {
      await api.undo.beginGroup(strings.toolpathShell.recalculateUndo);
    }
    try {
      for (const id of ids) {
        try {
          replies.push(await api.cam.recalculate(id));
        } catch (error) {
          const name = toolpathName(before, id);
          failures.push(
            ids.length > 1 && name !== null ? `${name}: ${message(error)}` : message(error),
          );
        }
      }
    } finally {
      if (grouped) {
        await api.undo.endGroup();
      }
    }
    useDocumentStore.setState({ lastError: failures.length === 0 ? null : failures.join('; ') });
    await useDocumentStore.getState().refresh();
    await get().refresh();
    const list = get().toolpaths;
    const { warningTexts } = await import('../lib/kernelWarnings');
    return replies.map((r) => ({
      toolpath: toolpathName(list, r.id),
      warnings: warningTexts(r.warnings),
    }));
  };

  /** Recalculates each toolpath with its stored settings, then reports. */
  const recalculateAll = async (ids: readonly number[]) => {
    report(await recalculateEach(ids));
  };

  return {
    toolpaths: [],
    groups: [],
    selectedId: null,
    selectedGroupId: null,
    moves: null,
    tiling: null,
    warnings: [],
    saveChoice: null,
    setSaveChoice: (ids) => set({ saveChoice: ids }),

    refresh: async () => {
      try {
        const [toolpaths, moves, groups, tiling] = await Promise.all([
          api.cam.list(),
          api.cam.moves(),
          api.cam.groups().catch(() => []),
          api.cam.tiling.get().catch(() => null),
        ]);
        const selected = get().selectedId;
        const selectedGroup = get().selectedGroupId;
        set({
          toolpaths,
          moves,
          groups,
          tiling,
          selectedId: toolpaths.some((t) => t.id === selected) ? selected : null,
          selectedGroupId: groups.some((g) => g.id === selectedGroup) ? selectedGroup : null,
        });
      } catch {
        set({ toolpaths: [], moves: null, groups: [] });
      }
    },

    select: (selectedId) => set({ selectedId, selectedGroupId: null }),
    selectGroup: (selectedGroupId) => set({ selectedGroupId, selectedId: null }),

    setTiling: (tiling) => run(() => api.cam.tiling.set(tiling)),
    viewTile: async (view) => {
      await run(() => api.cam.tiling.view(view));
    },

    calculate: async (request, id) => {
      if (
        (request.strategy === 'pocket' || request.strategy === 'vgroove') &&
        !(await passesCrossingCheck(
          request.ids,
          usePrefsStore.getState().prefs.confirmations.toolpathIntersections,
        ))
      ) {
        return false;
      }
      const done: { reply?: { id: number; warnings: KernelWarning[] } } = {};
      const ok = await run(async () => {
        const group = get().selectedGroupId;
        const reply =
          id === undefined
            ? await api.cam.compute(request, group ?? undefined)
            : await api.cam.recalculate(id, request);
        done.reply = reply;
        set({ selectedId: reply.id });
      });
      await reportReply(done.reply);
      // Toolpaths > Toolpath Drawing > Auto Open 3D View.
      const ui = useUiStore.getState();
      if (ok && id === undefined && ui.viewMode === '2d') {
        const prefs = usePrefsStore.getState();
        if (prefs.prefs.toolpaths.autoOpen3d) {
          ui.setViewMode('3d');
          // The view opens to show the new toolpath, so it is drawn even
          // when the 3D view's Show Toolpaths toggle was left off.
          if (!prefs.prefs.view3d.showToolpaths) {
            void prefs.update({ view3d: { showToolpaths: true } });
          }
        }
      }
      return ok;
    },

    recalculateStale: () =>
      recalculateAll(
        get()
          .toolpaths.filter((t) => t.stale)
          .map((t) => t.id),
      ),

    followJobChange: async (before) => {
      await get().refresh();
      const stale = get()
        .toolpaths.filter((t) => t.stale)
        .map((t) => t.id);
      let results: CalculationWarnings[] | null = null;
      if (stale.length > 0) {
        const ask = usePrefsStore.getState().prefs.toolpaths.askBeforeRecalculating;
        if (!ask || window.confirm(strings.toolpathShell.recalculateAsk(stale.length))) {
          results = await recalculateEach(stale);
        }
      }
      // Toolpaths > Check depth of all toolpaths decides which are checked.
      const job = useDocumentStore.getState().job;
      const check =
        before === undefined || depthsMayChange(before, job)
          ? await api.cam.depthCheck().catch(() => null)
          : null;
      const warning = check === null ? null : depthWarning(check, job.units);
      // One report, so the depth check does not hide what the recalculation said.
      if (warning !== null) {
        results = [...(results ?? []), { toolpath: null, warnings: [warning] }];
      }
      if (results !== null) {
        report(results);
      }
    },

    recalculate: (id) => recalculateAll([id]),

    saveTemplate: (path) => run(() => api.cam.saveTemplate(path)),

    merge: (ids, name, options) =>
      run(() => api.cam.merge(ids, name === '' ? undefined : name, options)),
    arrayCopy: (ids, spec, name) =>
      run(() => api.cam.arrayCopy(ids, spec, name === '' ? undefined : name)),

    loadTemplate: async (path, missing) => {
      if (missing === undefined) {
        try {
          const layers = await api.cam.templateLayers(path);
          if (layers.missing.length > 0) {
            useDialogStore.getState().askMissingLayers({ path, layers: layers.missing });
            return true;
          }
        } catch (error) {
          useDocumentStore.setState({ lastError: message(error) });
          return false;
        }
      }
      let skipped: { name: string; reason: string }[] = [];
      const ok = await run(async () => {
        skipped = (await api.cam.loadTemplate(path, missing ?? 'delete')).skipped;
      });
      report(skipped.map((s) => ({ toolpath: s.name, warnings: [s.reason] })));
      return ok;
    },

    duplicate: async (id, name) => {
      const done: { reply?: { id: number; warnings: KernelWarning[] } } = {};
      const ok = await run(async () => {
        const reply = await api.cam.duplicate(id, name);
        done.reply = reply;
        set({ selectedId: reply.id, selectedGroupId: null });
      });
      await reportReply(done.reply);
      return ok;
    },

    remove: async (id) => {
      await run(() => api.cam.delete([id]));
    },
    rename: async (id, name) => {
      await run(() => api.cam.rename(id, name));
    },
    setVisible: async (id, visible) => {
      await run(() => api.cam.setVisible([id], visible));
    },
    move: async (id, by) => {
      const ids = get().toolpaths.map((t) => t.id);
      const at = ids.indexOf(id);
      const to = at + by;
      if (at < 0 || to < 0 || to >= ids.length) {
        return;
      }
      [ids[at], ids[to]] = [ids[to] ?? id, id];
      await run(() => api.cam.reorder(ids));
    },
    setVisibleMany: async (ids, visible) => {
      if (ids.length > 0) {
        await run(() => api.cam.setVisible(ids, visible));
      }
    },
    removeMany: async (ids) => {
      if (ids.length > 0) {
        await run(() => api.cam.delete(ids));
      }
    },
    recalculateMany: (ids) => recalculateAll(ids),
    createGroup: async (ids) => {
      await run(() => api.cam.createGroup(ids));
    },
    updateGroup: async (id, patch) => {
      await run(() => api.cam.updateGroup(id, patch));
    },
    deleteGroup: async (id, deleteToolpaths) => {
      await run(() => api.cam.deleteGroup(id, deleteToolpaths));
    },
    place: async (ids, group, before) => {
      await run(() => api.cam.place(ids, group, before));
    },
    setSheet: async (ids, sheet) => {
      await run(() => api.cam.setSheet(ids, sheet));
    },
  };
});

// Every document change, undo included, can change toolpaths or their
// stale marks, so follow the document revision.
useDocumentStore.subscribe((state, previous) => {
  if (state.revision !== previous.revision) {
    void useToolpathStore.getState().refresh();
  }
});
