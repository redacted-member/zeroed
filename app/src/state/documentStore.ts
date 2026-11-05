// Mirror of the kernel's document. The kernel stays the source of truth: every
// change goes through it, then the mirror is refreshed from it.

import { create } from 'zustand';

import { api, type FitOptions } from '../bridge/api';
import type {
  ArrayCopyArgs,
  BooleanKind,
  BoundaryKind,
  CircularArrayArgs,
  CloseMode,
  TrimSide,
  CopyAlongArgs,
  FilletKind,
  DimensionSpec,
  FitKind,
  OffsetArgs,
  OnCurveTextArgs,
  PlainTextArgs,
  TextSpec,
  ValidationIssue,
  EntitySummary,
  Layer,
  NewLayerOptions,
  Bbox,
  Matrix,
  Point,
  SelectionState,
  Sheet,
  SheetMaterial,
  UndoInfo,
  Path,
} from '../bridge/document';
import { DEFAULT_JOB, type JobSetup, type Side, type VectorFormat } from '../bridge/types';
import { rotationAbout, translation } from '../lib/geometry';
import { PASTE_OFFSET_MM, useClipboardStore } from './clipboardStore';
import { copyRequest, type CopyRequest } from './copyRequest';
import { selectionTransformed } from './selectionTransforms';
import type { EditPictureArgs } from '../forms/EditPictureForm/model';
import type { VectorTextureRequest } from '../forms/VectorTextureForm/model';

export type KernelStatus = 'connecting' | 'ready' | 'offline';
export type SelectMode = 'set' | 'add' | 'toggle' | 'remove';

const NO_SELECTION: SelectionState = { ids: [], bbox: null };

/** Components selected along with the vectors (their 2D previews): their
 * box joins the selection's and transforms of the selection move them too.
 * Registered by state/componentSelection. */
export interface SelectionExtras {
  box: () => Bbox | null;
  /** `scaleZ`: Auto Scale Z, the heights scale with the size. */
  transform: (matrix: Matrix, scaleZ?: boolean) => Promise<void>;
  /** The selected components' ids. */
  picked: () => number[];
  /** Components just made (pasted, arrayed): the model is read again and
   * they become the ones selected. */
  adopt: (ids: number[]) => Promise<void>;
}

let extras: SelectionExtras | null = null;
/** The selection as the kernel last gave it, before the components join. */
let kernelSelection: SelectionState = NO_SELECTION;

export function setSelectionExtras(next: SelectionExtras | null): void {
  extras = next;
}

/** The components picked along with the vectors. */
export function pickedComponentIds(): number[] {
  return extras?.picked() ?? [];
}

/** The kernel's selection with the selected components' box joined. */
function withExtras(selection: SelectionState): SelectionState {
  kernelSelection = selection;
  const box = extras?.box() ?? null;
  if (box === null) {
    return selection;
  }
  const b = selection.bbox;
  return {
    ids: selection.ids,
    bbox:
      b === null
        ? box
        : [
            Math.min(b[0], box[0]),
            Math.min(b[1], box[1]),
            Math.max(b[2], box[2]),
            Math.max(b[3], box[3]),
          ],
  };
}
/** Undo label of a mirror of vectors and components together. */
const MIRROR_LABEL = 'Mirror';

/** Runs `action` so that everything it changes undoes as one step. */
export async function undoGroup(label: string | undefined, action: () => Promise<void>) {
  await api.undo.beginGroup(label);
  try {
    await action();
  } finally {
    await api.undo.endGroup();
  }
}

/** The quick mirror of a selection about the middle of its box: 'h' turns
 * it left to right, 'v' upside down. */
export function mirrorAbout(axis: 'h' | 'v', bbox: Bbox): Matrix {
  const cx = (bbox[0] + bbox[2]) / 2;
  const cy = (bbox[1] + bbox[3]) / 2;
  return axis === 'h' ? [-1, 0, 0, 1, 2 * cx, 0] : [1, 0, 0, -1, 0, 2 * cy];
}

const NO_UNDO: UndoInfo = { canUndo: false, canRedo: false, undoLabel: null, redoLabel: null };

interface DocumentState {
  job: JobSetup;
  /** The side of a double sided job being shown and edited. */
  activeSide: Side;
  kernelStatus: KernelStatus;
  kernelVersion: string | null;
  revision: number;
  /** File the document was last opened from or saved to. */
  path: string | null;
  /** True when there are changes since the last open or save. */
  dirty: boolean;
  layers: Layer[];
  /** Every sheet; the view shows the active one. */
  sheets: Sheet[];
  entities: EntitySummary[];
  selection: SelectionState;
  undo: UndoInfo;
  lastError: string | null;
  /** Bumped whenever another job takes the open one's place (new, open,
   * a template, the first connect), so what is kept per job reloads. */
  generation: number;
  /** Marks that another job took the open one's place. */
  jobReplaced: () => void;

  connect: () => Promise<void>;
  refresh: () => Promise<void>;
  applyJob: (job: JobSetup) => Promise<void>;
  /** Shows and edits the other side of a double sided job. */
  setSide: (side: Side) => Promise<void>;
  /** Exchanges everything between the top and bottom sides. */
  swapSides: () => Promise<void>;
  newDocument: () => Promise<void>;
  openDocument: (path: string) => Promise<boolean>;
  /** Opens an autosave backup as the job at `path` (null: never saved), unsaved. */
  restoreDocument: (backup: string, path: string | null) => Promise<boolean>;
  saveDocument: (path: string) => Promise<boolean>;
  /** Imports a vector file; `page` picks the page of a PDF, counting from 1. */
  importVectors: (path: string, page?: number) => Promise<void>;
  /** Exports the selection, or every vector when nothing is selected. */
  exportVectors: (path: string, format: VectorFormat) => Promise<boolean>;

  select: (ids: number[], mode?: SelectMode) => Promise<void>;
  selectAll: () => Promise<void>;
  clearSelection: () => Promise<void>;

  addLayer: (name?: string, colour?: string, options?: NewLayerOptions) => Promise<number>;
  updateLayer: (
    id: number,
    patch: Partial<Pick<Layer, 'name' | 'colour' | 'visible' | 'locked' | 'keepOut'>>,
  ) => Promise<void>;
  deleteLayer: (id: number, moveTo?: number) => Promise<void>;
  duplicateLayer: (id: number) => Promise<void>;
  setActiveLayer: (id: number) => Promise<void>;
  reorderLayers: (ids: number[]) => Promise<void>;
  moveSelectionToLayer: (id: number) => Promise<void>;
  /** Selects the selected closed vectors and every vector lying inside them. */
  selectInside: () => Promise<void>;
  /** Edit Picture on a placed bitmap. */
  editPicture: (args: EditPictureArgs) => Promise<void>;
  /** Crops the selected bitmap to the selected closed vectors. */
  cropBitmap: () => Promise<void>;
  /** Adds a vector texture inside the selected closed vectors or the job. */
  createVectorTexture: (request: VectorTextureRequest) => Promise<void>;
  /** The texture's paths, without changing the document. */
  previewVectorTexture: (request: VectorTextureRequest) => Promise<Path[]>;

  /** Adds a sheet and makes it the active one. */
  addSheet: (name?: string) => Promise<void>;
  /** Copies a sheet with its objects and shows the copy. */
  duplicateSheet: (id: number) => Promise<void>;
  /** Moves the selected vectors, and the components given, onto a sheet. */
  moveSelectionToSheet: (id: number, components?: number[]) => Promise<void>;
  /** Adds a sheet with the size of `source` without showing it. */
  addSheetLike: (name: string, source: Sheet) => Promise<void>;
  /** Gives sheets a new size, thickness and datum in one undo step. */
  /** Edit Sheet; `scaleDesign` stretches each sheet's design to its new size. */
  editSheets: (ids: number[], material: SheetMaterial, scaleDesign?: boolean) => Promise<void>;
  renameSheet: (id: number, name: string) => Promise<void>;
  /** Deletes a sheet and everything on it; the kernel keeps the last one. */
  deleteSheet: (id: number) => Promise<void>;
  /** Shows another sheet, dropping the selection first since it cannot cross sheets. */
  setActiveSheet: (id: number) => Promise<void>;

  /** Creates a path entity on the active layer, selects it and returns its id. */
  createPath: (path: Path, name?: string) => Promise<number | null>;
  createDimension: (spec: DimensionSpec, customText?: string, textAt?: Point) => Promise<void>;
  undoLast: () => Promise<void>;
  redoLast: () => Promise<void>;
  deleteSelection: () => Promise<void>;
  /** Transforms the selection, or just `ids` among it. */
  /** `scaleZ`: the selected components' heights scale with the size too
   * (Set Size's Auto Scale Z). */
  transformSelection: (matrix: Matrix, ids?: number[], scaleZ?: boolean) => Promise<void>;
  /** The selected components changed: their box joins the selection's. */
  refreshSelectionBox: () => void;
  moveSelection: (dx: number, dy: number) => Promise<void>;
  rotateSelection: (angleDeg: number) => Promise<void>;
  mirrorSelection: (axis: 'h' | 'v', copy: boolean) => Promise<void>;
  groupSelection: () => Promise<void>;
  ungroupSelection: (options?: { deep?: boolean; toGroupLayer?: boolean }) => Promise<void>;
  /** Convert Text to Curves on the selected text. */
  textToCurvesSelection: () => Promise<void>;
  copySelection: () => Promise<void>;
  cutSelection: () => Promise<void>;
  paste: () => Promise<void>;
  duplicateSelection: () => Promise<void>;
  alignSelection: (mode: string, target: 'selection' | 'material' | 'last') => Promise<void>;
  distributeSelection: (mode: string, spacing: number | null) => Promise<void>;
  /** Transforms a copy of the selection, leaving the original in place. */
  transformSelectionCopy: (matrix: Matrix, ids?: number[]) => Promise<void>;

  /** Creates text, selects it and answers its id, or null when refused. */
  createText: (args: PlainTextArgs) => Promise<number | null>;
  updateText: (id: number, spec: TextSpec, x: number, y: number) => Promise<void>;
  /** Text On A Curve: sets the text along the vector (or edits how). */
  setTextOnCurve: (args: OnCurveTextArgs) => Promise<number | null>;
  /** Remove Text From The Curve on the selected text. */
  removeTextFromCurve: () => Promise<void>;
  booleanSelection: (kind: BooleanKind) => Promise<void>;
  joinSelection: (tolerance: number, smooth?: boolean) => Promise<void>;
  /** Joins the given vectors where their ends meet (Rejoin Trimmed Sections). */
  joinSelectionOf: (ids: number[], tolerance: number) => Promise<void>;
  closeSelection: (mode: CloseMode) => Promise<void>;
  /** Trim Objects on the selection (the boundary selected last) and the
   * selected components. */
  trimObjectsSelection: (side: TrimSide, refit: boolean) => Promise<void>;
  offsetSelection: (args: Omit<OffsetArgs, 'ids'>) => Promise<void>;
  fitCurvesSelection: (tolerance: number, kind: FitKind, options?: FitOptions) => Promise<void>;
  boundarySelection: (offset: number, kind: BoundaryKind, rubberBand?: boolean) => Promise<void>;
  /** Checks the selection, or the visible vectors of the sheet when
   * nothing is selected; V-Groove Mode leaves out crossings in text. */
  validateSelection: (vgroove?: boolean) => Promise<ValidationIssue[]>;
  arrayCopySelection: (args: ArrayCopyArgs) => Promise<void>;
  circularArraySelection: (args: CircularArrayArgs) => Promise<void>;
  /** Copies every selected object but the last along the last one selected. */
  /** Copies `objects` (none for Copy Circles) along the vectors in
   * `args.pathIds`, with the picked components. */
  copyAlongSelection: (objects: number[], args: CopyAlongArgs) => Promise<void>;
  /**
   * Fillets, trims or extends the vector under a click. Answers whether a vector
   * was hit; a refusal from the kernel lands in lastError like any other.
   */
  filletAt: (
    x: number,
    y: number,
    tolerance: number,
    radius: number,
    kind: FilletKind,
  ) => Promise<boolean>;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const useDocumentStore = create<DocumentState>()((set, get) => {
  /** Runs a kernel mutation, then refreshes the mirror. Errors land in lastError. */
  /** Copies of the requested objects moved by `matrix`, one kernel step:
   * the vector copies become the selection, the component copies the
   * selected components. */
  const copyByMatrix = async (matrix: Matrix, request: CopyRequest | null): Promise<void> => {
    if (request === null) {
      return;
    }
    const reply = await api.model.copyMatrix(request.components, matrix, request.vectors);
    if (request.components.length > 0) {
      await extras?.adopt(reply.ids);
    }
  };

  const mutate = async (action: () => Promise<unknown>): Promise<void> => {
    try {
      await action();
      set({ lastError: null });
    } catch (error) {
      set({ lastError: message(error) });
    }
    await get().refresh();
  };

  /** The top level vector under a point, or null when the click missed. */
  const vectorAt = async (x: number, y: number, tolerance: number): Promise<number | null> => {
    const hit = await api.entities.hitTest({ x, y, tolerance });
    return hit.ids[0] ?? null;
  };

  /** Runs a pick operation on the vector under a point. */
  const pickOp = async (
    x: number,
    y: number,
    tolerance: number,
    action: (id: number) => Promise<unknown>,
  ): Promise<boolean> => {
    const id = await vectorAt(x, y, tolerance);
    if (id === null) {
      return false;
    }
    await mutate(() => action(id));
    return true;
  };

  /** Runs a create command and selects what it made. */
  const createAndSelect = async (action: () => Promise<{ id: number }>): Promise<number | null> => {
    let created: number | null = null;
    await mutate(async () => {
      created = (await action()).id;
      await api.selection.set([created]);
    });
    return created;
  };

  return {
    job: DEFAULT_JOB,
    activeSide: 'top',
    kernelStatus: 'connecting',
    kernelVersion: null,
    revision: 0,
    path: null,
    dirty: false,
    layers: [],
    sheets: [],
    entities: [],
    selection: NO_SELECTION,
    undo: NO_UNDO,
    lastError: null,
    generation: 0,
    jobReplaced: () => set((state) => ({ generation: state.generation + 1 })),

    connect: async () => {
      try {
        const pong = await api.ping();
        set({ kernelStatus: pong.pong ? 'ready' : 'offline', kernelVersion: pong.version });
        await get().refresh();
        get().jobReplaced();
      } catch (error) {
        console.error('kernel did not answer', error);
        set({ kernelStatus: 'offline' });
      }
    },

    refresh: async () => {
      try {
        const [info, job, layers, sheets, entities, selection, undo] = await Promise.all([
          api.doc.info(),
          api.job.get(),
          api.layers.list(),
          api.sheets.list(),
          api.entities.list(),
          api.selection.get(),
          api.undo.info(),
        ]);
        // Keep the job object while it reads the same, so what is built
        // from it (the 3D model surface) is not rebuilt on every refresh.
        const same = JSON.stringify(job) === JSON.stringify(get().job);
        set({
          revision: info.revision,
          path: info.path,
          dirty: info.dirty,
          activeSide: info.activeSide ?? 'top',
          job: same ? get().job : job,
          layers,
          sheets,
          entities,
          selection: withExtras(selection),
          undo,
        });
      } catch (error) {
        set({ lastError: message(error) });
      }
    },

    openDocument: async (path) => {
      await mutate(() => api.doc.open(path));
      const opened = get().lastError === null;
      if (opened) {
        get().jobReplaced();
      }
      return opened;
    },
    restoreDocument: async (backup, path) => {
      await mutate(() => api.doc.restore(backup, path));
      const restored = get().lastError === null;
      if (restored) {
        get().jobReplaced();
      }
      return restored;
    },
    saveDocument: async (path) => {
      await mutate(() => api.doc.save(path));
      return get().lastError === null;
    },
    importVectors: (path, page) =>
      mutate(async () => {
        const reply = await api.io.importVectors(path, page === undefined ? {} : { page });
        const warning = reply.warnings[0];
        if (warning !== undefined) {
          const { warningText } = await import('../lib/kernelWarnings');
          throw new Error(warningText(warning));
        }
      }),
    exportVectors: async (path, format) => {
      const ids = get().selection.ids;
      try {
        await api.io.exportVectors(path, format, ids.length > 0 ? ids : undefined);
        set({ lastError: null });
        return true;
      } catch (error) {
        set({ lastError: message(error) });
        return false;
      }
    },

    applyJob: async (job) => {
      const stored = await api.job.set(job);
      set({ job: stored });
      await get().refresh();
    },
    setSide: (side) => mutate(() => api.job.setSide(side)),
    swapSides: () => mutate(() => api.job.swapSides()),

    newDocument: async () => {
      await api.doc.new();
      await get().refresh();
      get().jobReplaced();
    },

    select: async (ids, mode = 'set') => {
      const call =
        mode === 'add'
          ? api.selection.add
          : mode === 'toggle'
            ? api.selection.toggle
            : mode === 'remove'
              ? api.selection.remove
              : api.selection.set;
      const selection = await call(ids);
      set({ selection: withExtras(selection) });
      await get().refresh();
    },
    selectAll: async () => {
      set({ selection: withExtras(await api.selection.all()) });
      await get().refresh();
    },
    clearSelection: async () => {
      set({ selection: withExtras(await api.selection.clear()) });
      await get().refresh();
    },

    addLayer: async (name, colour, options) => {
      const reply = await api.layers.add(name, colour, options);
      await get().refresh();
      return reply.id;
    },
    updateLayer: (id, patch) => mutate(() => api.layers.update(id, patch)),
    deleteLayer: (id, moveTo) => mutate(() => api.layers.delete(id, moveTo)),
    duplicateLayer: (id) => mutate(() => api.layers.duplicate(id)),
    setActiveLayer: (id) => mutate(() => api.layers.setActive(id)),
    reorderLayers: (ids) => mutate(() => api.layers.reorder(ids)),
    moveSelectionToLayer: (id) => mutate(() => api.entities.setLayer(get().selection.ids, id)),
    selectInside: () => mutate(() => api.selection.inside()),
    editPicture: (args) => mutate(() => api.bitmap.editPicture(args)),
    cropBitmap: () => mutate(() => api.bitmap.crop()),
    createVectorTexture: async (request) => {
      await api.vectors.texture(request);
      await get().refresh();
    },
    previewVectorTexture: async (request) => {
      const reply = await api.vectors.texture({ ...request, preview: true });
      return reply.paths ?? [];
    },

    addSheet: (name) => mutate(() => api.sheets.add(name, true)),
    duplicateSheet: (id) => mutate(() => api.sheets.duplicate(id)),
    moveSelectionToSheet: (id, components) =>
      mutate(() => api.sheets.moveTo(get().selection.ids, id, components)),
    addSheetLike: (name, source) =>
      mutate(async () => {
        const { id } = await api.sheets.add(name, false);
        // A new sheet already has the active sheet's size.
        if (!source.active && source.material !== undefined) {
          await api.sheets.edit([id], source.material);
        }
      }),
    editSheets: (ids, material, scaleDesign = false) =>
      mutate(() => api.sheets.edit(ids, material, scaleDesign)),
    renameSheet: (id, name) => mutate(() => api.sheets.rename(id, name)),
    deleteSheet: (id) => mutate(() => api.sheets.delete(id)),
    setActiveSheet: (id) =>
      mutate(async () => {
        if (get().selection.ids.length > 0) {
          await api.selection.clear();
        }
        await api.sheets.setActive(id);
      }),

    createPath: async (path, name) => {
      const layer = get().layers.find((l) => l.active)?.id ?? get().layers[0]?.id ?? 1;
      let id: number | null = null;
      await mutate(async () => {
        const entity =
          name === undefined
            ? { kind: 'path' as const, layer, sheet: 0, path }
            : { kind: 'path' as const, layer, sheet: 0, path, name };
        const reply = await api.entities.create(entity);
        id = reply.id;
        await api.selection.set([reply.id]);
      });
      return id;
    },
    createDimension: (spec, customText, textAt) =>
      mutate(async () => {
        const reply = await api.dims.create(spec, customText, textAt);
        await api.selection.set([reply.id]);
      }),
    undoLast: () => mutate(() => api.undo.undo()),
    redoLast: () => mutate(() => api.undo.redo()),
    deleteSelection: () =>
      mutate(async () => {
        // The selected components go with the vectors, in the same step.
        const components = extras?.picked() ?? [];
        if (get().selection.ids.length === 0 && components.length === 0) {
          return;
        }
        await api.entities.delete([...get().selection.ids], components);
        if (components.length > 0) {
          await extras?.adopt([]);
        }
      }),
    transformSelection: (matrix, ids, scaleZ) =>
      mutate(() =>
        undoGroup(undefined, async () => {
          const target = ids ?? get().selection.ids;
          if (target.length > 0) {
            await api.entities.transform(target, matrix);
          }
          // The selected components go with the whole selection.
          if (ids === undefined) {
            await extras?.transform(matrix, scaleZ);
            selectionTransformed(matrix);
          }
        }),
      ),
    refreshSelectionBox: () => set({ selection: withExtras(kernelSelection) }),
    moveSelection: (dx, dy) =>
      mutate(() =>
        undoGroup(undefined, async () => {
          const ids = get().selection.ids;
          if (ids.length > 0) {
            await api.ops.move(ids, dx, dy);
          }
          await extras?.transform(translation(dx, dy));
          selectionTransformed(translation(dx, dy));
        }),
      ),
    rotateSelection: (angleDeg) =>
      mutate(() =>
        undoGroup(undefined, async () => {
          const { ids, bbox } = get().selection;
          if (bbox === null) {
            return;
          }
          const cx = (bbox[0] + bbox[2]) / 2;
          const cy = (bbox[1] + bbox[3]) / 2;
          if (ids.length > 0) {
            await api.ops.rotate(ids, angleDeg, cx, cy);
          }
          await extras?.transform(rotationAbout(angleDeg, cx, cy));
          selectionTransformed(rotationAbout(angleDeg, cx, cy));
        }),
      ),
    mirrorSelection: (axis, copy) =>
      mutate(async () => {
        const { ids, bbox } = get().selection;
        const components = extras?.box() ?? null;
        if (components === null || bbox === null) {
          await api.ops.mirror(ids, axis, copy);
          if (bbox !== null) {
            selectionTransformed(mirrorAbout(axis, bbox));
          }
          return;
        }
        // Components mirror (or copy) with the vectors about the box of both.
        if (copy) {
          await copyByMatrix(mirrorAbout(axis, bbox), copyRequest(ids, extras?.picked() ?? []));
          selectionTransformed(mirrorAbout(axis, bbox));
          return;
        }
        const cx = (bbox[0] + bbox[2]) / 2;
        const cy = (bbox[1] + bbox[3]) / 2;
        await undoGroup(MIRROR_LABEL, async () => {
          if (ids.length > 0) {
            const lineTo: [number, number] = axis === 'h' ? [cx, cy + 1] : [cx + 1, cy];
            await api.ops.mirrorLine(ids, [cx, cy], lineTo, false);
          }
          const matrix = mirrorAbout(axis, bbox);
          await extras?.transform(matrix);
          selectionTransformed(matrix);
        });
      }),
    copySelection: async () => {
      const ids = get().selection.ids;
      const components = extras?.picked() ?? [];
      if (ids.length === 0 && components.length === 0) {
        return;
      }
      try {
        const reply = await api.clipboard.copy([...ids], components);
        useClipboardStore.getState().setCount(reply.count);
        set({ lastError: null });
      } catch (error) {
        set({ lastError: message(error) });
      }
    },
    cutSelection: async () => {
      await get().copySelection();
      await get().deleteSelection();
    },
    paste: () =>
      mutate(async () => {
        if (useClipboardStore.getState().count === 0) {
          return;
        }
        const reply = await api.clipboard.paste(PASTE_OFFSET_MM, PASTE_OFFSET_MM);
        if (reply.components.length > 0) {
          await extras?.adopt(reply.components);
        }
      }),
    duplicateSelection: () =>
      get().transformSelectionCopy(translation(PASTE_OFFSET_MM, PASTE_OFFSET_MM)),
    alignSelection: (mode, target) =>
      mutate(() => api.ops.align(get().selection.ids, mode, target)),
    distributeSelection: (mode, spacing) =>
      mutate(() => api.ops.distribute(get().selection.ids, mode, spacing)),
    transformSelectionCopy: (matrix, ids) =>
      mutate(async () => {
        await copyByMatrix(matrix, copyRequest(get().selection.ids, extras?.picked() ?? [], ids));
        // The copy becomes the selection, so what goes with it moves on.
        if (ids === undefined) {
          selectionTransformed(matrix);
        }
      }),
    groupSelection: () => mutate(() => api.entities.group(get().selection.ids)),
    ungroupSelection: (options) => mutate(() => api.entities.ungroup(get().selection.ids, options)),
    textToCurvesSelection: () => mutate(() => api.text.toCurves(get().selection.ids)),

    createText: (args) => createAndSelect(() => api.text.create(args)),
    updateText: (id, spec, x, y) => mutate(() => api.text.update(id, spec, x, y)),
    setTextOnCurve: (args) => createAndSelect(() => api.text.onCurve(args)),
    removeTextFromCurve: () =>
      mutate(async () => {
        const doc = get();
        for (const id of doc.selection.ids) {
          if (doc.entities.find((e) => e.id === id)?.onCurve !== undefined) {
            await api.text.removeFromCurve(id);
          }
        }
      }),
    booleanSelection: (kind) => mutate(() => api.ops.boolean(get().selection.ids, kind)),
    joinSelectionOf: (ids, tolerance) => mutate(() => api.ops.join(ids, tolerance, false)),
    joinSelection: (tolerance, smooth = false) =>
      mutate(() => api.ops.join(get().selection.ids, tolerance, smooth)),
    closeSelection: (mode) => mutate(() => api.ops.close(get().selection.ids, mode)),
    trimObjectsSelection: (side, refit) =>
      mutate(async () => {
        const components = extras?.picked() ?? [];
        const reply = await api.ops.trimObjects(get().selection.ids, components, side, refit);
        if (components.length > 0) {
          await extras?.adopt(reply.componentIds);
        }
      }),
    offsetSelection: (args) => mutate(() => api.ops.offset({ ids: get().selection.ids, ...args })),
    fitCurvesSelection: (tolerance, kind, options) =>
      mutate(() => api.ops.fitCurves(get().selection.ids, tolerance, kind, options)),
    boundarySelection: (offset, kind, rubberBand = false) =>
      mutate(() => api.ops.boundary(get().selection.ids, offset, kind, rubberBand)),
    validateSelection: async (vgroove = false) => {
      // With nothing selected the kernel checks the visible vectors of the
      // sheet shown.
      try {
        const reply = await api.ops.validate(get().selection.ids, vgroove);
        set({ lastError: null });
        return reply.issues;
      } catch (error) {
        set({ lastError: message(error) });
        return [];
      }
    },
    arrayCopySelection: (args) =>
      mutate(async () => {
        const components = extras?.picked() ?? [];
        const reply = await api.ops.arrayCopy(get().selection.ids, args, components);
        await api.selection.set([...get().selection.ids, ...reply.ids]);
        if (components.length > 0) {
          await extras?.adopt([...components, ...reply.components]);
        }
      }),
    circularArraySelection: (args) =>
      mutate(async () => {
        const components = extras?.picked() ?? [];
        const reply = await api.ops.circularArray(get().selection.ids, args, components);
        await api.selection.set([...get().selection.ids, ...reply.ids]);
        if (components.length > 0) {
          await extras?.adopt([...components, ...reply.components]);
        }
      }),
    copyAlongSelection: (objects, args) =>
      mutate(async () => {
        const components = args.circleDiameter === undefined ? (extras?.picked() ?? []) : [];
        const reply = await api.ops.copyAlong(objects, args, components);
        await api.selection.set(reply.ids);
        if (components.length > 0) {
          const kept = args.deleteOriginal ? [] : components;
          await extras?.adopt([...kept, ...reply.components]);
        }
      }),
    filletAt: (x, y, tolerance, radius, kind) =>
      pickOp(x, y, tolerance, (id) => api.ops.fillet(id, x, y, radius, kind)),
  };
});
