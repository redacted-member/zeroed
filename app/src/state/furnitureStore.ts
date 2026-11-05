// The Furniture Designer: the template library, the designs in the job and
// the design being edited. A change previews at once (furniture.preview,
// one request at a time, the newest waiting one replacing older ones) and,
// for a design already made, regenerates it (furniture.update, one undo
// step) when the change settles: on release or after 400 ms idle. Seam
// picks in the Assembly View change a joint the same way.

import { create } from 'zustand';

import type { Path } from '../bridge/document';
import { api } from '../bridge/formApi';
import type {
  AttachmentRow,
  ConnectionRow,
  ModelSource,
  DesignRow,
  DesignSummary,
  FurnitureJoint,
  FurnitureJoints,
  FurniturePreview,
  FurnitureTemplate,
  FurnitureValues,
  JoineryStyle,
  SetJointArgs,
  TemplateSummary,
} from '../bridge/furniture';
import { jointsFromConnections } from '../forms/FurnitureDesigner/model';
import type { FaceHit } from '../views/View3D/panelFaces';
import { madeOf } from '../views/View3D/panelFaces';
import { furnitureStrings } from '../lib/strings/furniture';
import { useAssemblyStore } from './assemblyStore';
import { IdleSettler, LatestQueue } from './latestQueue';
import { useDocumentStore } from './documentStore';
import { leaveFace, useFaceEditStore } from './faceEditStore';
import { useUiStore } from './uiStore';
import { useViewStore } from './viewStore';

/** Changes settle this long after the last one, ms. */
export const SETTLE_MS = 400;

/** The design open in the designer. */
export interface Session {
  /** The design in the document, or null before Create. */
  design: number | null;
  template: FurnitureTemplate;
  name: string;
  values: FurnitureValues;
  joints: FurnitureJoints;
  /** The values the document's design was last made with (null before Create). */
  made: FurnitureValues | null;
}

/** A seam under the pointer, or opened by a click, with where it was in the view (CSS px). */
export interface SeamPick {
  key: string;
  x: number;
  y: number;
}

/** A broad face of a furniture panel, picked in the Assembly View. */
export interface FaceTarget {
  design: number;
  panel: string;
  /** The panel's name, such as "Door left". */
  name: string;
  hit: Extract<FaceHit, { kind: 'face' }>;
}

/** The face context menu, at a point of the view (CSS px from its top left). */
export interface FaceMenu {
  x: number;
  y: number;
  target: FaceTarget;
}

/** A share of the panel's smaller side kept clear round a model put on it. */
export const FACE_MARGIN = 0.1;

interface PreviewArgs {
  source: { template: string } | { design: number };
  values: FurnitureValues;
  joints: FurnitureJoints;
}

interface FurnitureState {
  templates: TemplateSummary[] | null;
  loadingTemplates: boolean;
  /** A thumbnail per template id: an image URL, or null when it could not be drawn. */
  thumbnails: Readonly<Record<string, string | null>>;
  designs: DesignRow[];
  session: Session | null;
  preview: FurniturePreview | null;
  /** Whether a preview or regenerate is on its way. */
  busy: boolean;
  /** The made design's connections (the preview's before Create). */
  connections: ConnectionRow[];
  summary: DesignSummary | null;
  hover: SeamPick | null;
  popover: SeamPick | null;
  error: string | null;
  notice: string | null;
  faceMenu: FaceMenu | null;
  /** The models and vectors on the open design's panels. */
  attachments: AttachmentRow[];

  loadTemplates: () => Promise<void>;
  setThumbnail: (id: string, url: string | null) => void;
  loadDesigns: () => Promise<void>;
  /** Opens the designer on a new design from a template. */
  startNew: (templateId: string) => Promise<void>;
  /** Opens the designer on a design in the document. */
  edit: (design: number) => Promise<void>;
  /** Leaves the designer (the form closed). */
  endSession: () => void;
  setName: (name: string) => void;
  /** New values: previewed now; regenerated when settled (at once with `release`). */
  setValues: (values: FurnitureValues, release?: boolean) => void;
  /** Regenerates the newest waiting change now (a slider let go). */
  release: () => void;
  setStyle: (style: JoineryStyle) => Promise<void>;
  /** A class's joint (null drops the class override), with sizes. */
  setClassJoint: (
    cls: string,
    joint: FurnitureJoint | null,
    params?: SetJointArgs['params'],
  ) => Promise<void>;
  /** A pick on a seam: the connection alone, or the class for all like this. */
  pickSeamJoint: (key: string, joint: FurnitureJoint, forAll: boolean) => Promise<void>;
  create: () => Promise<boolean>;
  remove: (keepVectors: boolean) => Promise<boolean>;
  saveTemplate: (id: string, name: string) => Promise<boolean>;
  importTemplate: (json: string) => Promise<boolean>;
  removeTemplate: (id: string) => Promise<boolean>;
  setHover: (hover: SeamPick | null) => void;
  setPopover: (popover: SeamPick | null) => void;
  setError: (error: string | null) => void;
  setNotice: (notice: string | null) => void;
  setFaceMenu: (menu: FaceMenu | null) => void;
  /** Puts clipart, a component or a model file on a panel's face; one undo step. */
  addModelToFace: (target: FaceTarget, source: ModelSource) => Promise<boolean>;
  /** Shows the panel's sheet in the 2D or 3D view, zoomed to the panel, so every Modeling tool applies. */
  editFace: (target: FaceTarget, view: '2d' | '3d') => Promise<void>;
  /** Back to the Assembly View after editing a face. */
  backToAssembly: () => void;
  loadAttachments: () => Promise<void>;
  /** Makes 3D roughing and finishing for the models on the open design. */
  makeCarving: () => Promise<boolean>;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function previewArgs(session: Session): PreviewArgs {
  return {
    source:
      session.design === null ? { template: session.template.id } : { design: session.design },
    values: session.values,
    joints: session.joints,
  };
}

/** The joint choices a document design keeps, read from its connections where the list leaves them out. */
function jointsOf(design: DesignRow, connections: readonly ConnectionRow[]): FurnitureJoints {
  const read = jointsFromConnections(design.style, connections);
  return {
    style: design.style,
    classes: design.classJoints === undefined ? read.classes : { ...design.classJoints },
    connections:
      design.connectionJoints === undefined ? read.connections : { ...design.connectionJoints },
  };
}

/** A path's box as [minX, minY, maxX, maxY], or null when it has no points. */
function pathBox(path: Path): [number, number, number, number] | null {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const sub of path.subpaths) {
    xs.push(sub.start.x);
    ys.push(sub.start.y);
    for (const seg of sub.segments) {
      xs.push(seg.to.x);
      ys.push(seg.to.y);
    }
  }
  return xs.length === 0
    ? null
    : [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

export const useFurnitureStore = create<FurnitureState>()((set, get) => {
  const previews = new LatestQueue<PreviewArgs, FurniturePreview>(
    (args) => api.furniture.preview(args.source, args.values, args.joints),
    (preview) => {
      const session = get().session;
      set({
        preview,
        busy: previews.pending || settler.waiting,
        // Before Create the preview's connections are the ones to pick.
        ...(session?.design === null ? { connections: preview.connections } : {}),
      });
    },
    (error) => set({ error: message(error), busy: previews.pending }),
  );

  const regenerate = async (values: FurnitureValues) => {
    const session = get().session;
    if (session === null || session.design === null) {
      return;
    }
    set({ busy: true });
    try {
      const name = get().session?.name ?? session.name;
      const reply = await api.furniture.update({ design: session.design, values, name });
      const now = get().session;
      if (now !== null && now.design === session.design) {
        set({ session: { ...now, made: values }, summary: reply.summary, error: null });
      }
      await afterChange(session.design);
    } catch (error) {
      set({ error: message(error) });
    } finally {
      set({ busy: previews.pending });
    }
  };

  const settlerCore = new IdleSettler<FurnitureValues>(
    (values) => void regenerate(values),
    SETTLE_MS,
  );
  const settler = {
    waiting: false,
    change(values: FurnitureValues) {
      settler.waiting = true;
      settlerCore.change(values);
    },
    release() {
      settler.waiting = false;
      settlerCore.release();
    },
    cancel() {
      settler.waiting = false;
      settlerCore.cancel();
    },
  };

  /** Reloads the document, the designs and the made design's connections. */
  const afterChange = async (design: number | null) => {
    await useDocumentStore.getState().refresh();
    await get().loadDesigns();
    if (design !== null) {
      const { connections } = await api.furniture.connections(design);
      const session = get().session;
      if (session?.design === design) {
        const row = get().designs.find((d) => d.id === design);
        set({
          connections,
          session: row === undefined ? session : { ...session, joints: jointsOf(row, connections) },
        });
      }
    }
  };

  /**
   * Applies a joint change: kept in the session before Create, else made
   * in the kernel in one undo step (setJoint, or with `whole` the joint
   * choices replaced together through update).
   */
  const changeJoint = async (
    args: Omit<SetJointArgs, 'design'>,
    local: FurnitureJoints,
    whole = false,
  ) => {
    const session = get().session;
    if (session === null) {
      return;
    }
    if (session.design === null) {
      const next = { ...session, joints: local };
      set({ session: next, busy: true });
      previews.push(previewArgs(next));
      return;
    }
    set({ busy: true, session: { ...session, joints: local } });
    try {
      const reply = whole
        ? await api.furniture.update({ design: session.design, joints: local })
        : await api.furniture.setJoint({ ...args, design: session.design });
      set({ summary: reply.summary, error: null });
      await afterChange(session.design);
      const now = get().session;
      if (now !== null) {
        previews.push(previewArgs(now));
      }
    } catch (error) {
      set({ error: message(error) });
    } finally {
      set({ busy: previews.pending });
    }
  };

  return {
    templates: null,
    loadingTemplates: false,
    thumbnails: {},
    designs: [],
    session: null,
    preview: null,
    busy: false,
    connections: [],
    summary: null,
    hover: null,
    popover: null,
    error: null,
    notice: null,
    faceMenu: null,
    attachments: [],

    loadTemplates: async () => {
      set({ loadingTemplates: true });
      try {
        const { templates } = await api.furniture.templates();
        set({ templates, error: null });
      } catch (error) {
        set({ error: message(error) });
      } finally {
        set({ loadingTemplates: false });
      }
    },

    setThumbnail: (id, url) => set((s) => ({ thumbnails: { ...s.thumbnails, [id]: url } })),

    loadDesigns: async () => {
      try {
        const { designs } = await api.furniture.list();
        set({ designs });
      } catch (error) {
        set({ error: message(error) });
      }
    },

    startNew: async (templateId) => {
      try {
        const template = await api.furniture.template(templateId);
        const values: FurnitureValues = {};
        for (const p of template.params) {
          values[p.key] = p.default;
        }
        const session: Session = {
          design: null,
          template,
          name: template.name,
          values,
          joints: { style: 'template', classes: {}, connections: {} },
          made: null,
        };
        previews.cancel();
        settler.cancel();
        set({
          session,
          preview: null,
          connections: [],
          summary: null,
          popover: null,
          hover: null,
          error: null,
          notice: null,
          busy: true,
        });
        useUiStore.getState().openForm('furnitureDesigner');
        previews.push(previewArgs(session));
      } catch (error) {
        set({ error: message(error) });
      }
    },

    edit: async (design) => {
      try {
        await get().loadDesigns();
        const row = get().designs.find((d) => d.id === design);
        if (row === undefined) {
          return;
        }
        const [template, { connections }] = await Promise.all([
          api.furniture.template(row.template),
          api.furniture.connections(design),
        ]);
        const session: Session = {
          design,
          template,
          name: row.name,
          values: { ...row.values },
          joints: jointsOf(row, connections),
          made: { ...row.values },
        };
        previews.cancel();
        settler.cancel();
        set({
          session,
          preview: null,
          summary: null,
          popover: null,
          hover: null,
          error: null,
          notice: null,
          busy: true,
        });
        set({ connections });
        useUiStore.getState().openForm('furnitureDesigner');
        previews.push(previewArgs(session));
      } catch (error) {
        set({ error: message(error) });
      }
    },

    endSession: () => {
      // A change still waiting is made before the designer goes.
      settler.release();
      previews.cancel();
      set({
        session: null,
        preview: null,
        connections: [],
        hover: null,
        popover: null,
        busy: false,
      });
    },

    setName: (name) => {
      const session = get().session;
      if (session !== null) {
        set({ session: { ...session, name } });
      }
    },

    setValues: (values, release = false) => {
      const session = get().session;
      if (session === null) {
        return;
      }
      const next = { ...session, values };
      set({ session: next, busy: true });
      previews.push(previewArgs(next));
      if (session.design !== null) {
        settler.change(values);
        if (release) {
          settler.release();
        }
      }
    },

    release: () => settler.release(),

    setStyle: async (style) => {
      const session = get().session;
      if (session === null) {
        return;
      }
      await changeJoint({ scope: 'style', key: style, joint: null }, { ...session.joints, style });
    },

    setClassJoint: async (cls, joint, params) => {
      const session = get().session;
      if (session === null) {
        return;
      }
      const classes = { ...session.joints.classes };
      if (joint === null) {
        delete classes[cls];
      } else {
        classes[cls] = params === undefined ? { joint } : { joint, params };
      }
      await changeJoint(
        params === undefined
          ? { scope: 'class', key: cls, joint }
          : { scope: 'class', key: cls, joint, params },
        { ...session.joints, classes },
      );
    },

    pickSeamJoint: async (key, joint, forAll) => {
      const { session, connections } = get();
      const seam = connections.find((c) => c.key === key);
      if (session === null || seam === undefined) {
        return;
      }
      const classes = { ...session.joints.classes };
      const own = { ...session.joints.connections };
      if (forAll) {
        classes[seam.class] = { joint };
        for (const c of connections) {
          if (c.class === seam.class) {
            delete own[c.key];
          }
        }
      } else {
        own[key] = { joint };
      }
      const local = { ...session.joints, classes, connections: own };
      // For all like this also drops the class's seams set by hand, so the
      // choices go together in one step.
      await changeJoint(
        forAll ? { scope: 'class', key: seam.class, joint } : { scope: 'connection', key, joint },
        local,
        forAll,
      );
    },

    create: async () => {
      const session = get().session;
      if (session === null || session.design !== null) {
        return false;
      }
      set({ busy: true });
      try {
        const reply = await api.furniture.create({
          template: session.template.id,
          values: session.values,
          joints: session.joints,
          name: session.name,
        });
        set({
          session: { ...session, design: reply.design, made: { ...session.values } },
          summary: reply.summary,
          error: null,
        });
        await afterChange(reply.design);
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      } finally {
        set({ busy: previews.pending });
      }
    },

    remove: async (keepVectors) => {
      const session = get().session;
      if (session === null || session.design === null) {
        return false;
      }
      try {
        settler.cancel();
        await api.furniture.remove(session.design, keepVectors);
        await afterChange(null);
        set({ session: null, preview: null, connections: [], summary: null });
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
    },

    saveTemplate: async (id, name) => {
      const session = get().session;
      if (session === null || session.design === null) {
        return false;
      }
      try {
        await api.furniture.saveTemplate(session.design, id, name);
        await get().loadTemplates();
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
    },

    importTemplate: async (json) => {
      try {
        await api.furniture.importTemplate(json);
        await get().loadTemplates();
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
    },

    removeTemplate: async (id) => {
      try {
        await api.furniture.removeTemplate(id);
        set((s) => {
          const thumbnails = { ...s.thumbnails };
          delete thumbnails[id];
          return { thumbnails };
        });
        await get().loadTemplates();
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      }
    },

    setHover: (hover) => {
      const old = get().hover;
      if (old?.key !== hover?.key || old?.x !== hover?.x || old?.y !== hover?.y) {
        set({ hover });
      }
    },
    setPopover: (popover) => set({ popover, hover: null }),
    setError: (error) => set({ error }),
    setNotice: (notice) => set({ notice }),
    setFaceMenu: (faceMenu) => set({ faceMenu }),

    addModelToFace: async (target, source) => {
      set({ faceMenu: null });
      try {
        await api.furniture.addModel({
          design: target.design,
          panel: target.panel,
          face: target.hit.face,
          source,
          point: target.hit.at,
          fit: { margin: FACE_MARGIN },
        });
        await useDocumentStore.getState().refresh();
        await useAssemblyStore.getState().refresh();
        await get().loadDesigns();
        const now = get().session;
        if (now !== null && now.design === target.design) {
          // The preview raises the new model on its panel.
          previews.push(previewArgs(now));
        }
        if (get().session?.design === target.design) {
          await get().loadAttachments();
        }
        set({ error: null });
        return true;
      } catch (error) {
        const text = message(error);
        set({ error: text });
        useAssemblyStore.getState().setError(text);
        return false;
      }
    },

    editFace: async (target, view) => {
      set({ faceMenu: null });
      await get().loadDesigns();
      const design = get().designs.find((d) => d.id === target.design);
      const made = design === undefined ? null : madeOf(design, target.panel);
      if (made === null) {
        return;
      }
      const doc = useDocumentStore.getState();
      const ui = useUiStore.getState();
      // The face in furniture words (front, left...), read where the panel lies.
      let word: string | null = null;
      try {
        const entity = await api.entities.get(made.vector);
        const box = entity.kind === 'path' ? pathBox(entity.path) : null;
        if (box !== null) {
          const under = await api.furniture.panelOf(made.sheet, [
            (box[0] + box[2]) / 2,
            (box[1] + box[3]) / 2,
          ]);
          word = target.hit.face === 'top' ? under.top : under.bottom;
        }
      } catch {
        word = null;
      }
      const fromDesigner = get().session?.design === target.design;
      useFaceEditStore.getState().setEditing({
        design: target.design,
        panel: target.panel,
        label: furnitureStrings.furniture.face.label(
          target.name,
          word ?? furnitureStrings.furniture.face.side[target.hit.face],
        ),
        // Back to the designer when the face was edited from it.
        back: fromDesigner ? () => void get().edit(target.design) : undefined,
      });
      ui.closeForm();
      await doc.setActiveSheet(made.sheet);
      await useDocumentStore.getState().select([made.vector]);
      ui.setViewMode(view);
      useViewStore.getState().zoomToBounds(useDocumentStore.getState().selection.bbox);
    },

    backToAssembly: () => {
      const ui = useUiStore.getState();
      ui.setViewMode('3d');
      leaveFace(() => ui.openForm('assemblyView'));
    },

    loadAttachments: async () => {
      const design = get().session?.design ?? null;
      if (design === null) {
        set({ attachments: [] });
        return;
      }
      try {
        const { attachments } = await api.furniture.attachments(design);
        set({ attachments });
      } catch (error) {
        set({ error: message(error) });
      }
    },

    makeCarving: async () => {
      const design = get().session?.design ?? null;
      if (design === null) {
        return false;
      }
      set({ busy: true });
      try {
        const reply = await api.furniture.makeCarving(design);
        await useDocumentStore.getState().refresh();
        await get().loadDesigns();
        set({
          error: null,
          notice: furnitureStrings.furniture.face.carvingMade(reply.ids.length),
        });
        return true;
      } catch (error) {
        set({ error: message(error) });
        return false;
      } finally {
        set({ busy: false });
      }
    },
  };
});
