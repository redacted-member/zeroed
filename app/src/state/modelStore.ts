// Mirror of the relief model: the component list and the baked composite
// the 3D view draws. Follows the document revision like the other mirrors.

import { create } from 'zustand';

import { api } from '../bridge/api';
import type { PaletteColours } from '../bridge/componentTree';
import type { ReplaceBelowArgs } from '../bridge/modelRefine';
import { exportsApi, type MeshFormat, type MeshOptions } from '../bridge/exports';
import type { Point } from '../bridge/document';
import type {
  BrushParams,
  CustomBrushParams,
  Combine,
  Level,
  LevelPatch,
  ComponentSummary,
  Composite,
  ComponentEdit,
  ComponentEditOp,
  ComponentTransformParams,
  MeshParams,
} from '../bridge/model';
import type { PropertiesTarget } from '../panels/ComponentProperties/model';
import { useDocumentStore } from './documentStore';

/** What Set Anchor picks two points for. */
export type AnchorPurpose = 'fade' | 'tilt';

export interface AnchorPick {
  purpose: AnchorPurpose;
  /** Job XY of the clicks so far, first anchor first. */
  points: Point[];
}

interface ModelState {
  components: ComponentSummary[];
  /** Model levels, bottom first. */
  levels: Level[];
  /** The level new components go on. */
  activeLevel: number | null;
  composite: Composite | null;
  /** Component colours per composite cell, or null when none has one. */
  colors: PaletteColours | null;
  /** The kernel's model stamp the composite was fetched at. */
  stamp: number | null;
  selectedId: number | null;
  /** Components picked together in the tree (Ctrl or Shift click); the
   * selected one is always among them, see {@link pickedComponents}. */
  picked: number[];
  /** The selected components are tinted in the 3D view. Component
   * Properties turns it off while it is open. */
  highlight: boolean;
  /** What the Component Properties form edits while it is open. */
  propertiesTarget: PropertiesTarget | null;
  setPropertiesTarget: (target: PropertiesTarget | null) => void;
  refresh: () => Promise<void>;
  select: (id: number | null) => void;
  setPicked: (ids: number[]) => void;
  setHighlight: (on: boolean) => void;
  /** Brings a 3D model file in as a component. Answers success. */
  importMesh: (path: string, params: MeshParams, combine: Combine) => Promise<boolean>;
  /** The two anchor points a fade or tilt is picking in the 3D view (Set
   * Anchor); null when none is. Done when it holds two points. */
  anchorPick: AnchorPick | null;
  startAnchorPick: (purpose: AnchorPurpose) => void;
  /** A click in the 3D view while picking; ignored once two are in. */
  addAnchorPoint: (point: Point) => void;
  cancelAnchorPick: () => void;
  /** The edit the Edit Component form opens on. */
  editOp: ComponentEditOp;
  setEditOp: (op: ComponentEditOp) => void;
  /** Applies one edit to a component, one undo step. Answers success. */
  editComponent: (id: number, edit: ComponentEdit) => Promise<boolean>;
  /** Moves, scales and turns a component in XY, one undo step. Answers success. */
  transformComponent: (id: number, params: ComponentTransformParams) => Promise<boolean>;
  /** Applies one brush stroke to a component, one undo step. Answers success. */
  sculpt: (
    id: number,
    points: Point[],
    params: BrushParams,
    custom?: CustomBrushParams | null,
    pressures?: number[],
  ) => Promise<boolean>;
  /** Writes components (or the composite) to a binary STL file, with a
   * solid base `base` mm thick under the lowest point. */
  exportStl: (path: string, ids: number[] | undefined, base?: number) => Promise<boolean>;
  /** Writes the model, or components `ids`, as an STL, OBJ or POV-Ray mesh. */
  exportMesh: (
    path: string,
    format: MeshFormat,
    ids: number[] | undefined,
    options?: MeshOptions,
  ) => Promise<boolean>;
  /** Writes the model over the job, or components `ids`, as a greyscale PNG. */
  exportHeightmap: (path: string, bits: 8 | 16, ids: number[] | undefined) => Promise<boolean>;
  /** Bakes components into one new component and selects it. Answers success. */
  bake: (ids: number[], name: string) => Promise<boolean>;
  /** Replace Below on components or a level, one undo step. Answers success. */
  replaceBelow: (args: ReplaceBelowArgs) => Promise<boolean>;
  /** The Replace Below plane the 3D view shows while the form is open:
   * its model height (mm) and whether the form or a drag in the view set
   * it last. */
  cuttingPlane: { height: number; from: 'form' | 'view' } | null;
  setCuttingPlane: (plane: { height: number; from: 'form' | 'view' } | null) => void;
  update: (
    id: number,
    patch: Partial<
      Pick<ComponentSummary, 'name' | 'visible' | 'combine' | 'heightScale' | 'level'>
    >,
  ) => Promise<void>;
  remove: (id: number) => Promise<void>;
  /** Removes every component and group, one undo step. Answers success. */
  clear: () => Promise<boolean>;
  /** Moves a component one place down (-1) or up (1) the stack. */
  move: (id: number, by: -1 | 1) => Promise<void>;
  /** Adds a level on top; it becomes the active level. */
  addLevel: () => Promise<void>;
  updateLevel: (id: number, patch: LevelPatch) => Promise<void>;
  /** Deletes a level; its components move to the level below. */
  deleteLevel: (id: number) => Promise<void>;
  /** Moves a level one place down (-1) or up (1). */
  moveLevel: (id: number, by: -1 | 1) => Promise<void>;
  setActiveLevel: (id: number) => Promise<void>;
}

/** The selected components: the picked set when it holds the selected
 * component, else just that one (a selection made elsewhere, such as in
 * the 3D view, starts a fresh set). */
export function pickedComponents(state: Pick<ModelState, 'selectedId' | 'picked'>): number[] {
  const { selectedId, picked } = state;
  if (selectedId === null) {
    return [];
  }
  return picked.includes(selectedId) ? picked : [selectedId];
}

/** The component colours, or null when no cell has one or they cannot be read. */
async function fetchColors(): Promise<PaletteColours | null> {
  try {
    const colors = await api.model.tree.colors();
    return colors.any === false ? null : colors;
  } catch {
    return null;
  }
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function trimmed(name: string): string | undefined {
  return name.trim() === '' ? undefined : name.trim();
}

export const useModelStore = create<ModelState>()((set, get) => {
  const run = async (action: () => Promise<unknown>): Promise<boolean> => {
    let ok = true;
    try {
      await action();
      useDocumentStore.setState({ lastError: null });
    } catch (error) {
      useDocumentStore.setState({ lastError: message(error) });
      ok = false;
    }
    await useDocumentStore.getState().refresh();
    await get().refresh();
    return ok;
  };

  /** Reads the model from the kernel; the large composite and colours only
   * when the model changed. */
  const load = async (): Promise<void> => {
    try {
      const { stamp, components, levels, activeLevel } = await api.model.list();
      // Read when the reply is stored, so a selection made during the
      // fetches below is kept.
      const selectedId = (): number | null => {
        const selected = get().selectedId;
        return components.some((c) => c.id === selected) ? selected : null;
      };
      // The composite is large: fetch it only when the model changed.
      if (stamp === get().stamp) {
        set({ components, levels, activeLevel, selectedId: selectedId() });
      } else {
        const composite = components.length > 0 ? await api.model.composite() : null;
        const colors = composite === null ? null : await fetchColors();
        set({
          components,
          levels,
          activeLevel,
          composite,
          colors,
          stamp,
          selectedId: selectedId(),
        });
      }
      // The selected components' boxes may have moved with the model.
      useDocumentStore.getState().refreshSelectionBox();
    } catch {
      set({
        components: [],
        levels: [],
        activeLevel: null,
        composite: null,
        colors: null,
        stamp: null,
      });
    }
  };
  let refreshing: Promise<void> | null = null;
  let refreshAgain: Promise<void> | null = null;

  return {
    components: [],
    levels: [],
    activeLevel: null,
    composite: null,
    colors: null,
    stamp: null,
    selectedId: null,
    picked: [],
    highlight: true,
    propertiesTarget: null,
    setPropertiesTarget: (propertiesTarget) => set({ propertiesTarget }),

    refresh: () => {
      // One refresh at a time: a call while one runs waits for it and then
      // runs once more for everyone who asked, so a model change fetches
      // the composite and colours once however many mirrors ask.
      if (refreshing === null) {
        refreshing = load().finally(() => {
          refreshing = null;
        });
        return refreshing;
      }
      if (refreshAgain === null) {
        refreshAgain = refreshing.then(() => {
          refreshAgain = null;
          return get().refresh();
        });
      }
      return refreshAgain;
    },

    select: (selectedId) => {
      set({ selectedId });
      useDocumentStore.getState().refreshSelectionBox();
    },
    setPicked: (picked) => {
      set({ picked });
      useDocumentStore.getState().refreshSelectionBox();
    },
    setHighlight: (highlight) => set({ highlight }),

    importMesh: (path, params, combine) =>
      run(async () => {
        const reply = await api.model.importMesh(path, params, combine);
        set({ selectedId: reply.id });
      }),

    anchorPick: null,
    startAnchorPick: (purpose) => set({ anchorPick: { purpose, points: [] } }),
    addAnchorPoint: (point) => {
      const pick = get().anchorPick;
      if (pick !== null && pick.points.length < 2) {
        set({ anchorPick: { ...pick, points: [...pick.points, point] } });
      }
    },
    cancelAnchorPick: () => set({ anchorPick: null }),

    editOp: 'smooth',
    setEditOp: (editOp) => set({ editOp }),

    editComponent: (id, edit) => run(() => api.model.edit(id, edit)),

    transformComponent: (id, params) => run(() => api.model.transform(id, params)),

    sculpt: (id, points, params, custom, pressures) =>
      run(() => api.model.sculpt(id, points, params, custom, pressures)),

    exportStl: (path, ids, base) => run(() => api.model.exportStl(path, ids, base)),

    exportMesh: (path, format, ids, options) =>
      run(() => exportsApi.exportMesh(path, format, ids, options)),

    exportHeightmap: (path, bits, ids) => run(() => exportsApi.exportHeightmap(path, bits, ids)),

    bake: (ids, name) =>
      run(async () => {
        const reply = await api.model.bake(ids, trimmed(name));
        set({ selectedId: reply.id });
      }),

    replaceBelow: (args) =>
      run(async () => {
        const reply = await api.model.replaceBelow(args);
        set({ selectedId: reply.id, picked: [reply.id] });
      }),
    cuttingPlane: null,
    setCuttingPlane: (cuttingPlane) => set({ cuttingPlane }),

    update: async (id, patch) => {
      await run(() => api.model.update(id, patch));
    },
    remove: async (id) => {
      await run(() => api.model.delete([id]));
    },
    clear: () =>
      run(async () => {
        await api.model.clear();
        set({ selectedId: null });
      }),
    move: async (id, by) => {
      const ids = get().components.map((c) => c.id);
      const at = ids.indexOf(id);
      const to = at + by;
      if (at < 0 || to < 0 || to >= ids.length) {
        return;
      }
      [ids[at], ids[to]] = [ids[to] ?? id, id];
      await run(() => api.model.reorder(ids));
    },
    addLevel: async () => {
      await run(() => api.model.levels.add());
    },
    updateLevel: async (id, patch) => {
      await run(() => api.model.levels.update(id, patch));
    },
    deleteLevel: async (id) => {
      await run(() => api.model.levels.delete(id));
    },
    moveLevel: async (id, by) => {
      const ids = get().levels.map((l) => l.id);
      const at = ids.indexOf(id);
      const to = at + by;
      if (at < 0 || to < 0 || to >= ids.length) {
        return;
      }
      [ids[at], ids[to]] = [ids[to] ?? id, id];
      await run(() => api.model.levels.reorder(ids));
    },
    setActiveLevel: async (id) => {
      await run(() => api.model.levels.setActive(id));
    },
  };
});

useDocumentStore.subscribe((state, previous) => {
  if (state.revision !== previous.revision) {
    void useModelStore.getState().refresh();
  }
});
