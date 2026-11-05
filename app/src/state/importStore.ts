// The 3D model import flow shared by its three forms (Orientate Model,
// Position Model, 3D Segmenting): the file, the settings of each step, and
// the preview the 3D view draws while a step is open. Settings are kept as
// the kernel takes them; the forms hold their own text fields.

import { create } from 'zustand';

import { api } from '../bridge/formApi';
import {
  DEFAULT_ORIENTATION,
  DEFAULT_PLACEMENT,
  DEFAULT_PLAN,
  DEFAULT_ROTARY_FIT,
  type CutPlane,
  type ImportSource,
  type MeshInfo,
  type MeshOrientation,
  type MeshPlacement,
  type MeshPreview,
  type RotaryFit,
  type SegmentPlan,
  type Vec3,
} from '../bridge/meshImport';
import type { Combine } from '../bridge/model';
import type { Units } from '../bridge/types';
import { useDocumentStore } from './documentStore';
import { useModelStore } from './modelStore';

export type ImportStep = 'orientate' | 'position' | 'segment';

/** What the 3D view shows of the flow besides the model. */
export interface ImportView {
  step: ImportStep;
  /** Tint undercuts (Position: Highlight Undercuts; Segment: Visualize). */
  undercuts: boolean;
  /** Segment step: segments laid out rather than in the model. */
  layout: boolean;
  /** Segment step: the chosen tree node; only segments under it show. */
  selected: string;
  /** Segment step, edit mode: the plane the next cut uses. */
  plane: CutPlane | null;
  /** Segment step, layout mode: the segment the rotation rings turn (O),
   * or none. */
  ringSegment: string | null;
}

/** On a rotary job: a full 3D model laid on the axis, or a flat one
 * placed like clipart. */
export type RotaryModelType = 'full' | 'flat';

interface ImportState {
  path: string | null;
  modelType: RotaryModelType;
  rotaryFit: RotaryFit;
  /** Full 3D on a rotary job: make the blank the model's size on OK. */
  resizeMaterial: boolean;
  /** Rotation Axis Movement: panning the 3D view moves the model round the
   * axis instead of the view. */
  axisMove: boolean;
  setAxisMove: (on: boolean) => void;
  orient: MeshOrientation;
  /** Millimetres per file unit along each turned axis. */
  scale: Vec3;
  place: MeshPlacement;
  plan: SegmentPlan;
  combine: Combine;
  /** Components a reimport replaces. */
  replace: number[];
  /** Size wanted by a reimport, turned into a scale once the file is read. */
  pendingSize: Vec3 | null;
  info: MeshInfo | null;
  preview: MeshPreview | null;
  /** Set by begin and resume, cleared by clear. */
  active: boolean;
  view: ImportView;
  busy: boolean;
  error: string | null;
  /** Starts a new import of `path` (or of no file yet) in the job units. */
  begin: (path: string | null, units: Units) => void;
  /** Starts a reimport from a component's saved settings. */
  resume: (source: ImportSource, ids: number[]) => void;
  setOrient: (patch: Partial<MeshOrientation>) => void;
  setModelType: (modelType: RotaryModelType) => void;
  setRotaryFit: (patch: Partial<RotaryFit>) => void;
  setResizeMaterial: (resizeMaterial: boolean) => void;
  /** Imports a full 3D model onto the rotary axis. Answers success. */
  importRotary: () => Promise<boolean>;
  setScale: (scale: Vec3) => void;
  setPlace: (patch: Partial<MeshPlacement>) => void;
  setPlan: (plan: SegmentPlan) => void;
  setView: (patch: Partial<ImportView>) => void;
  /** Reads the model size and the preview for the current settings. */
  refresh: () => Promise<void>;
  /** Imports as Position Model says. Answers success. */
  importPlaced: () => Promise<boolean>;
  /** Imports every segment. Answers success. */
  importSegments: () => Promise<boolean>;
  /** Drops the flow and its preview. */
  clear: () => void;
}

const INITIAL_VIEW: ImportView = {
  step: 'orientate',
  undercuts: false,
  layout: false,
  selected: 'r',
  plane: null,
  ringSegment: null,
};

/** The orientation the kernel gets: the size from the scale. */
export function sizedOrientation(
  orient: MeshOrientation,
  natural: Vec3,
  scale: Vec3,
): MeshOrientation {
  return {
    ...orient,
    size: [natural[0] * scale[0], natural[1] * scale[1], natural[2] * scale[2]],
    unitScale: 1,
  };
}

/** Per axis scale that turns `natural` into `size`; 1 where it is flat. */
export function scaleFor(natural: Vec3, size: Vec3): Vec3 {
  const one = (n: number, s: number) => (n > 1e-12 && s > 0 ? s / n : 1);
  return [one(natural[0], size[0]), one(natural[1], size[1]), one(natural[2], size[2])];
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Bumped by every refresh so late answers are dropped. */
let generation = 0;

export const useImportStore = create<ImportState>()((set, get) => {
  const kernelOrient = (natural: Vec3) => sizedOrientation(get().orient, natural, get().scale);

  const afterImport = async () => {
    await useDocumentStore.getState().refresh();
    await useModelStore.getState().refresh();
  };

  /** Whether the flow lays a full 3D model on a rotary axis. */
  const onAxis = () =>
    get().modelType === 'full' && useDocumentStore.getState().job.kind === 'rotary';
  /** The orientation a rotary import sends: file numbers times the unit. */
  const bareRotary = (): MeshOrientation => ({
    ...get().orient,
    size: null,
    unitScale: get().scale[0],
    perspective: 0,
    centre: null,
  });

  return {
    path: null,
    modelType: 'full',
    rotaryFit: DEFAULT_ROTARY_FIT,
    resizeMaterial: false,
    axisMove: false,
    orient: DEFAULT_ORIENTATION,
    scale: [1, 1, 1],
    place: DEFAULT_PLACEMENT,
    plan: DEFAULT_PLAN,
    combine: 'add',
    replace: [],
    pendingSize: null,
    info: null,
    preview: null,
    active: false,
    view: INITIAL_VIEW,
    busy: false,
    error: null,

    begin: (path, units) => {
      // A mesh file carries no units: its numbers are read in the job's.
      const unit = units === 'inch' ? 25.4 : 1;
      set({
        path,
        modelType: 'full',
        rotaryFit: DEFAULT_ROTARY_FIT,
        resizeMaterial: false,
        orient: DEFAULT_ORIENTATION,
        scale: [unit, unit, unit],
        place: DEFAULT_PLACEMENT,
        plan: DEFAULT_PLAN,
        combine: 'add',
        replace: [],
        pendingSize: null,
        info: null,
        preview: null,
        view: INITIAL_VIEW,
        active: true,
        error: null,
      });
    },

    resume: (source, ids) => {
      set({
        path: source.path,
        modelType: (source.rotary ?? null) === null ? 'flat' : 'full',
        rotaryFit: source.rotary ?? DEFAULT_ROTARY_FIT,
        resizeMaterial: false,
        orient: { ...DEFAULT_ORIENTATION, ...source.orient },
        scale: [1, 1, 1],
        pendingSize: source.orient.size,
        place: { ...DEFAULT_PLACEMENT, ...(source.place ?? {}) },
        plan: { ...DEFAULT_PLAN, ...(source.plan ?? {}) },
        combine: source.combine,
        replace: ids,
        info: null,
        preview: null,
        view: { ...INITIAL_VIEW, step: source.plan ? 'segment' : 'orientate' },
        active: true,
        error: null,
      });
    },

    setOrient: (patch) => set({ orient: { ...get().orient, ...patch } }),
    setScale: (scale) => set({ scale }),
    setModelType: (modelType) => set({ modelType }),
    setRotaryFit: (patch) => set({ rotaryFit: { ...get().rotaryFit, ...patch } }),
    setResizeMaterial: (resizeMaterial) => set({ resizeMaterial }),
    setAxisMove: (axisMove) => set({ axisMove }),
    setPlace: (patch) => set({ place: { ...get().place, ...patch } }),
    setPlan: (plan) => set({ plan }),
    setView: (patch) => set({ view: { ...get().view, ...patch } }),

    refresh: async () => {
      const { path } = get();
      if (path === null) {
        set({ info: null, preview: null });
        return;
      }
      const mine = (generation += 1);
      set({ busy: true });
      if (onAxis()) {
        try {
          const orient = bareRotary();
          const fit = get().rotaryFit;
          const info = await api.model.import.info(path, orient, fit);
          const preview = await api.model.import.preview(path, orient, fit);
          if (mine === generation) {
            set({ info, preview, busy: false, error: null });
          }
        } catch (error) {
          if (mine === generation) {
            set({ busy: false, error: message(error), preview: null });
          }
        }
        return;
      }
      try {
        const bare = { ...get().orient, size: null, unitScale: 1 };
        const info = await api.model.import.info(path, bare);
        const pending = get().pendingSize;
        if (pending !== null) {
          set({ scale: scaleFor(info.natural, pending), pendingSize: null });
        }
        const orient = kernelOrient(info.natural);
        if (get().view.step === 'segment' && get().view.plane === null) {
          // The first segmenting plane is level through the middle.
          const size = orient.size ?? info.size;
          const centre = orient.centre ?? info.centre;
          set({
            view: {
              ...get().view,
              plane: { point: [centre.x, centre.y, size[2] / 2], angleX: 0, angleY: 0 },
            },
          });
        }
        const { view, plan } = get();
        const preview =
          view.step === 'segment'
            ? await api.model.import.segmentPreview(path, orient, plan, view.layout)
            : await api.model.import.preview(path, orient);
        if (mine !== generation) {
          return;
        }
        set({
          info: { ...info, size: orient.size ?? info.size },
          preview,
          busy: false,
          error: null,
        });
      } catch (error) {
        if (mine === generation) {
          set({ busy: false, error: message(error), preview: null });
        }
      }
    },

    importPlaced: async () => {
      const { path, info, place, combine, replace } = get();
      if (path === null || info === null) {
        return false;
      }
      try {
        await api.model.import.apply({
          path,
          orient: kernelOrient(info.natural),
          place,
          combine,
          replace,
        });
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
      await afterImport();
      get().clear();
      return true;
    },

    importRotary: async () => {
      const { path, info, combine, replace, rotaryFit, resizeMaterial } = get();
      if (path === null || info === null) {
        return false;
      }
      try {
        await api.model.import.apply({
          path,
          orient: bareRotary(),
          place: DEFAULT_PLACEMENT,
          combine,
          replace,
          rotary: rotaryFit,
          resizeMaterial,
        });
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
      await afterImport();
      get().clear();
      return true;
    },

    importSegments: async () => {
      const { path, info, plan, combine, replace } = get();
      if (path === null || info === null) {
        return false;
      }
      try {
        await api.model.import.segmentApply({
          path,
          orient: kernelOrient(info.natural),
          plan,
          combine,
          replace,
        });
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
      await afterImport();
      get().clear();
      return true;
    },

    clear: () => {
      generation += 1;
      set({
        preview: null,
        info: null,
        axisMove: false,
        view: INITIAL_VIEW,
        busy: false,
        error: null,
        active: false,
      });
    },
  };
});
