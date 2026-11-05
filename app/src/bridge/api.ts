// Typed wrappers over the kernel commands the front end uses. Each function
// maps one kernel API command; argument names follow the contract.

import { heightsOf } from './blob';
import { floatHeights } from './heights';
import { getKernel } from './client';
import type {
  EmbossComponentArgs,
  OffsetModelArgs,
  PreviewComponentMode,
  ReplaceBelowArgs,
  ScaleHeightArgs,
  ScaleHeightReply,
  SmoothComponentArgs,
} from './modelRefine';
import type { DuplicatesReply } from './drawTools';
import type { PanelUnder } from './furniture';
import type {
  ArrayCopyArgs,
  Bbox,
  BooleanKind,
  BoundaryKind,
  CircularArrayArgs,
  CloseMode,
  TrimSide,
  DimensionSpec,
  CopyAlongArgs,
  FilletKind,
  FitKind,
  OffsetArgs,
  OnCurveTextArgs,
  Path,
  PlainTextArgs,
  TextSpec,
  ValidationIssue,
  DisplayHeader,
  DisplayReply,
  DocInfo,
  GuideLine,
  EntityJson,
  EntitySummary,
  HitTestArgs,
  Layer,
  Matrix,
  NewEntity,
  Point,
  ProbeArgs,
  ProbeReply,
  Revision,
  SelectionState,
  Sheet,
  SheetMaterial,
  UndoInfo,
  NewLayerOptions,
} from './document';
import type {
  Heightfield,
  HeightfieldHeader,
  MovesHeader,
  MovesReply,
  PostSummary,
  PreviewQuality,
  Tiling,
  TilingReply,
  SimPatch,
  SimCheck,
  SimPatchHeader,
  SimRunReply,
  SimStartReply,
  SimStepArgs,
  SimStepReply,
  ArrayCopySpec,
  ToolDatabaseList,
  DepthCheckReply,
  ToolpathGroup,
  ToolpathRequest,
  ToolpathSummary,
  MergeOptions,
} from './cam';
import type {
  BrushParams,
  CustomBrushParams,
  Combine,
  ComponentSummary,
  Composite,
  CompositeHeader,
  LevelPatch,
  ModelList,
  MeshParams,
  BoundaryArgs,
  ComponentEdit,
  ComponentTransformParams,
} from './model';
import type {
  ColorsHeader,
  ComponentProps,
  GroupPatch,
  GroupTransformParams,
  HighlightReply,
  PaletteColours,
  TreeTarget,
} from './componentTree';
import type { SplitArgs, SplitReply } from './modelTools';
import type {
  BlobHeader,
  BlobReply,
  JobSetup,
  KernelWarning,
  PingReply,
  Side,
  VectorFormat,
} from './types';
import type { PrefsPatch, PrefsReply } from './prefs';
import type { GadgetSummary } from './gadgets';
import type { ClipartInsertArgs, ClipartInsertReply, ClipartScan } from './clipart';
import type { TemplateList, TemplateSaveArgs } from './templates';
import type { EditPictureArgs } from '../forms/EditPictureForm/model';
import type { VectorTextureRequest } from '../forms/VectorTextureForm/model';

/** Pixel size of a bitmap file. */

/** What loading a template does with toolpaths whose layers the job lacks. */
export type MissingLayersChoice = 'create' | 'delete' | 'asIs';
export interface BitmapInfo {
  width: number;
  height: number;
}

export interface TraceIndicesHeader {
  width: number;
  height: number;
  sections: Array<{ name: string; dtype: 'u8'; offset: number; count: number }>;
}

export interface BitmapPixelsHeader {
  imageId: number;
  width: number;
  height: number;
  mime: string;
  sections: Array<{ name: string; dtype: 'u8'; offset: number; count: number }>;
}

export interface BitmapImportReply extends Revision {
  /** The placed bitmap entity. */
  id: number;
  imageId: number;
  /** Image size in pixels. */
  width: number;
  height: number;
}

export type TraceFit = 'lines' | 'curves';

/** Trace settings, see geom/trace.rs for their meaning. */
export interface TraceParams {
  threshold: number;
  invert: boolean;
  noise: number;
  cornerFit: TraceFit;
  smoothness: number;
}

export type PlateSeparator = 'comma' | 'tab' | 'semicolon' | 'space';

/** Where a Plate Production variable's text comes from. */
export type PlateField =
  | { kind: 'column'; column: number }
  | { kind: 'counter'; start: number; step: number; digits: number };

export interface PlatesProduceArgs {
  ids: number[];
  path?: string;
  text?: string;
  separator: PlateSeparator;
  header: boolean;
  fields: Record<string, PlateField>;
  count?: number;
  margins: { left: number; right: number; top: number; bottom: number };
  spacing: { x: number; y: number };
  ownSheets: boolean;
  /** Where plates that overflow a sheet go: new sheets or new layers. */
  mode?: 'sheets' | 'layers';
  /** Calculate the template's toolpaths for the plates of each sheet. */
  toolpaths?: boolean;
}

export interface PlatesData {
  columns: string[];
  rows: number;
  sample: string[][];
}

/** Colour tracing and the trace area for bitmap.traceImage. */
export interface TraceImageOptions {
  /** Reduce to `count` colours and trace the `trace` palette indices together. */
  colours?: { count: number; trace: number[] };
  /** Only the pixels under this job box are traced. */
  region?: [number, number, number, number];
}

/** A traced outline shown before it is made (Preview). */
export type TracePreview = { paths: Path[] };

export interface TracePalette {
  colour: boolean;
  colours: Array<{ rgb: [number, number, number]; pixels: number }>;
}

/** Without origin and pixelSize the kernel fits the image inside the job, centred. */
export interface BitmapTraceArgs {
  path: string;
  params: TraceParams;
  /** Job position of the image's bottom left corner, in millimetres. */
  origin?: Point;
  /** Millimetres per pixel. */
  pixelSize?: number;
}

export function k() {
  return getKernel();
}

/** Arguments of model.fromBitmap: grey levels become heights, black at the
 * base height and white at base plus height (the other way when inverted). */
export interface BitmapReliefArgs {
  /** The image file; omitted with id. */
  path?: string;
  /** A placed bitmap to convert in place of a file; the component lies
   * exactly under it and origin and pixelSize are ignored. */
  id?: number;
  /** Job position of the image's bottom left corner, mm; centred when absent. */
  origin?: Point;
  /** Millimetres per pixel; the image fits the job when absent. */
  pixelSize?: number;
  /** Height between black and white, mm. */
  height: number;
  baseHeight: number;
  /** Blur passes before sampling, 0 to 20. */
  smoothing: number;
  invert: boolean;
  combine: Combine;
  name?: string;
}

/** Arguments of ops.distort: envelope edges in job millimetres, bottom and
 * top left to right, sides bottom to top (straight when left out). */
export interface DistortArgs {
  ids: number[];
  componentIds: number[];
  /** Bounding Box (the envelope below), Along a Single Curve or Between
   * Two Curves (the envelope comes from `curves`). */
  mode?: DistortMode;
  curves?: number[];
  /** The original box the envelope was drawn round, in the frame. */
  bbox?: Bbox;
  envelope?: {
    bottom: Point[];
    top: Point[];
    left?: Point[];
    right?: Point[];
    /** Straight spans, one flag per span of each edge; curves when absent. */
    lines?: EnvelopeLines;
  };
  /** Use Rotated Bounds: the frame angle in degrees (box and envelope are
   * in the frame turned by it about the job origin). */
  angle?: number;
  /** Fit tolerance for the rebuilt vectors, mm. */
  tolerance?: number;
}

export type DistortMode = 'box' | 'curve' | 'twoCurves';

/** Which spans of the envelope edges are straight lines. */
export type EnvelopeLines = Partial<Record<'bottom' | 'top' | 'left' | 'right', boolean[]>>;

/** Fit Curves to Vectors options besides the tolerance and the kind. */
export interface FitOptions {
  sharpAngle?: number;
  replace?: boolean;
}

/** A distortion whose envelope can still be edited (ops.distortion). */
export interface StoredDistortion {
  ids: number[];
  mode: DistortMode;
  curves?: number[];
  angle: number;
  bbox: Bbox;
  bottom: Point[];
  top: Point[];
  left?: Point[];
  right?: Point[];
  /** Straight spans of the edges; every span a curve when absent. */
  lines?: EnvelopeLines;
}

/** A preview reply's laser marks, when it has any. */
export function burnOf(reply: BlobReply<BlobHeader>): { burn?: Uint8Array } {
  const burn = reply.sections['burn'];
  return burn instanceof Uint8Array ? { burn } : {};
}

/** The changed cells of a `sim.patch` or `sim.advance` reply. */
export function patchOf(reply: BlobReply<SimPatchHeader>): SimPatch {
  const owner = reply.sections['owner'];
  const none = floatHeights(new Float32Array(0));
  return {
    rects: (reply.header.rects ?? []).map(([x0, y0, width, height]) => ({
      x0,
      y0,
      width,
      height,
    })),
    z: heightsOf(reply, 'z') ?? none,
    owner: owner instanceof Uint16Array ? owner : new Uint16Array(0),
    floor: heightsOf(reply, 'floor') ?? none,
    ...burnOf(reply),
  };
}

export const api = {
  ping: () => k().call<PingReply>('ping'),

  /** The furniture panel under a point of a sheet, for the status line. */
  furniture: {
    panelOf: (sheet: number, point: [number, number]) =>
      k().call<PanelUnder>('furniture.panelOf', { sheet, point }),
  },

  doc: {
    new: () => k().call<Revision>('doc.new'),
    info: () => k().call<DocInfo>('doc.info'),
    open: (path: string) => k().call<Revision>('doc.open', { path }),
    /** Opens an autosave backup as the job `as` (null: never saved) with unsaved changes. */
    restore: (path: string, as: string | null) =>
      k().call<Revision>('doc.restore', as === null ? { path } : { path, as }),
    /** The job notes (Edit > Notes). */
    getNotes: () => k().call<{ notes: string }>('doc.notes.get'),
    /** The 2D view's guide lines, saved with the job. */
    guides: () => k().call<{ guides: GuideLine[] }>('doc.guides.get'),
    /** Replaces the guide lines; not an undo step. Answers whether the job
     * now has unsaved changes. */
    setGuides: (guides: GuideLine[]) => k().call<{ dirty: boolean }>('doc.guides.set', { guides }),
    save: (path: string) => k().call<Revision>('doc.save', { path }),
    /** Writes a backup copy; the document keeps its path and unsaved state. */
    saveCopy: (path?: string) => k().call<{ path: string }>('doc.saveCopy', path ? { path } : {}),
    /** Every variable's value, and the formula of each defined by one. */
    variables: () =>
      k().call<{ variables: Record<string, number>; formulas?: Record<string, string> }>(
        'doc.variables.get',
      ),
    /** Replaces the variables: a value, or a formula kept live ({w}/2+1). */
    setVariables: (variables: Record<string, number | string>) =>
      k().call<Revision>('doc.variables.set', { variables }),
  },

  templates: {
    list: () => k().call<TemplateList>('templates.list'),
    save: (args: TemplateSaveArgs) => k().call<{ path: string }>('templates.save', args),
    /** Opens a template as a new untitled job. */
    newFrom: (path: string) => k().call<Revision & { name: string }>('templates.new', { path }),
  },

  text: {
    create: (args: PlainTextArgs) => k().call<Revision & { id: number }>('text.create', args),
    /** `merge`: the revision the previous live edit of this text answered,
     * so the kernel folds this edit into that step. */
    update: (id: number, spec: TextSpec, x?: number, y?: number, merge?: number) =>
      k().call<Revision>('text.update', { id, spec, x, y, merge }),
    onCurve: (args: OnCurveTextArgs) => k().call<Revision & { id: number }>('text.onCurve', args),
    removeFromCurve: (id: number) =>
      k().call<Revision & { id: number }>('text.removeFromCurve', { id }),
    /** Replaces text with plain vectors, a group of one per letter. */
    toCurves: (ids: number[]) => k().call<Revision & { ids: number[] }>('text.toCurves', { ids }),
  },

  tools: {
    list: () => k().call<ToolDatabaseList>('tools.list'),
  },

  posts: {
    list: () => k().call<PostSummary[]>('posts.list'),
    /** Copies a post processor file into the user's posts. */
    import: (path: string) => k().call<{ id: string }>('posts.import', { path }),
  },

  cam: {
    /** `selectedGroup` is the group picked in the list, which new toolpaths
     * join when Append new toolpaths to selected group is on. */
    compute: (request: ToolpathRequest, selectedGroup?: number) =>
      k().call<Revision & { id: number; clearanceId?: number | null; warnings: KernelWarning[] }>(
        'cam.compute',
        selectedGroup === undefined ? request : { ...request, selectedGroup },
      ),
    /** Calculates a copy of a toolpath under `name`, placed as Append
     * duplicated toolpath says. */
    duplicate: (id: number, name: string) =>
      k().call<Revision & { id: number; warnings: KernelWarning[] }>('cam.duplicate', { id, name }),
    /** Toolpaths cutting below their material, visible ones only unless
     * Check depth of all toolpaths is on. */
    depthCheck: () => k().call<DepthCheckReply>('cam.depthCheck'),
    recalculate: (id: number, request?: ToolpathRequest) =>
      k().call<Revision & { id: number; warnings: KernelWarning[] }>('cam.recalculate', {
        id,
        request,
      }),
    list: () => k().call<ToolpathSummary[]>('cam.list'),
    groups: () => k().call<ToolpathGroup[]>('cam.groups'),
    /** A group holding `ids` (none for an empty group) on the active sheet. */
    createGroup: (ids: number[], name?: string) =>
      k().call<Revision & { id: number }>('cam.groups.create', { ids, name }),
    updateGroup: (id: number, patch: { name?: string; expanded?: boolean; sheet?: number }) =>
      k().call<Revision>('cam.groups.update', { id, ...patch }),
    /** Ungroups, or deletes the group's toolpaths with it. */
    deleteGroup: (id: number, deleteToolpaths: boolean) =>
      k().call<Revision>('cam.groups.delete', { id, deleteToolpaths }),
    /** Moves toolpaths into `group` (0 for none) in front of `before`, or at the group's end. */
    place: (ids: number[], group: number, before?: number) =>
      k().call<Revision>('cam.place', { ids, group, before }),
    setSheet: (ids: number[], sheet: number) => k().call<Revision>('cam.setSheet', { ids, sheet }),
    delete: (ids: number[]) => k().call<Revision>('cam.delete', { ids }),
    rename: (id: number, name: string) => k().call<Revision>('cam.rename', { id, name }),
    setVisible: (ids: number[], visible: boolean) =>
      k().call<Revision>('cam.setVisible', { ids, visible }),
    reorder: (ids: number[]) => k().call<Revision>('cam.reorder', { ids }),
    moves: async (): Promise<MovesReply> => {
      const reply = await k().callBlob<MovesHeader>('cam.moves', {});
      const xyz = reply.sections['xyz'];
      const kind = reply.sections['kind'];
      const width = reply.sections['width'];
      const points = kind instanceof Uint8Array ? kind.length : 0;
      return {
        revision: reply.header.revision,
        toolpaths: reply.header.toolpaths,
        xyz: xyz instanceof Float32Array ? xyz : new Float32Array(0),
        width: width instanceof Float32Array ? width : new Float32Array(points),
        kind: kind instanceof Uint8Array ? kind : new Uint8Array(0),
      };
    },
    /** Toolpath Tiling: the settings and the tiles they make. */
    tiling: {
      get: () => k().call<TilingReply>('cam.tiling.get', {}),
      set: (tiling: Tiling) => k().call<TilingReply & Revision>('cam.tiling.set', { tiling }),
      view: (view: { active?: number; originalPosition?: boolean }) =>
        k().call<TilingReply & Revision>('cam.tiling.view', view),
    },
    /** Writes an HTML job sheet summing up every toolpath. */
    jobSheet: (path: string, title?: string) =>
      k().call<{ path: string }>('cam.jobSheet', { path, title }),
    /** Joins same tool toolpaths into a new one and hides the originals. */
    merge: (ids: number[], name?: string, options?: MergeOptions) =>
      k().call<Revision & { id: number }>('cam.merge', { ids, name, options }),
    /** Array Copy Toolpath: copies of the toolpaths in a grid, grouped; hides the originals. */
    arrayCopy: (ids: number[], spec: ArrayCopySpec, name?: string) =>
      k().call<Revision & { ids: number[]; group: number }>('cam.arrayCopy', { ids, spec, name }),
    /** Import Toolpaths: a G-code file as one toolpath per tool; one undo step. */
    importToolpaths: (path: string) =>
      k().call<Revision & { ids: number[]; warnings: KernelWarning[] }>('cam.importToolpaths', {
        path,
      }),
    /** Writes the toolpaths' settings, with their vectors' layer names, to a template file. */
    saveTemplate: (path: string, ids?: number[]) =>
      k().call<{ count: number }>('cam.saveTemplate', { path, ids }),
    /** The layers a template's toolpaths use that this job lacks. */
    templateLayers: (path: string) =>
      k().call<{ missing: string[]; count: number }>('cam.templateLayers', { path }),
    /**
     * Calculates a template's toolpaths on the vectors in same named layers;
     * the rest wait uncalculated. `missing` says what to do about toolpaths
     * whose layers the job lacks.
     */
    loadTemplate: (path: string, missing: MissingLayersChoice = 'delete') =>
      k().call<
        Revision & {
          ids: number[];
          skipped: { name: string; reason: string }[];
          uncalculated: number[];
        }
      >('cam.loadTemplate', { path, missing }),
  },

  model: {
    importMesh: (path: string, params: MeshParams, combine: Combine) =>
      k().call<Revision & { id: number; triangles: number }>('model.importMesh', {
        path,
        params,
        combine,
      }),
    /** A component from a greyscale image file. */
    fromBitmap: (args: BitmapReliefArgs) =>
      k().call<Revision & { id: number }>('model.fromBitmap', args),
    edit: (id: number, edit: ComponentEdit) => k().call<Revision>('model.edit', { id, ...edit }),
    boundary: (args: BoundaryArgs) =>
      k().call<Revision & { ids: number[]; vectors: number[] }>('model.boundary', args),
    /** Components moved, scaled, turned or mirrored with the selection by
     * the affine `matrix` (as entities.transform takes it), one undo step.
     * A shape moved after its own vectors by the same matrix still lies on
     * them (the kernel records the vectors' move when they move). */
    /** `scaleZ`: Auto Scale Z, the heights scale with the size. */
    transformMatrix: (ids: number[], matrix: Matrix, replace?: number, scaleZ?: boolean) =>
      k().call<Revision>('model.transformMatrix', { ids, matrix, replace, scaleZ }),
    /** Copies of components moved by the affine `matrix`, one undo step.
     * `vectors` are copied with them (any kind, groups whole) and become the
     * selection; a shape copied with its vectors is made from their copies. */
    copyMatrix: (ids: number[], matrix: Matrix, vectors?: number[]) =>
      k().call<Revision & { ids: number[]; vectors: number[] }>('model.copyMatrix', {
        ids,
        matrix,
        vectors,
      }),
    /** With `replace`, the step made at that revision (the same live drag)
     * is taken back first, so a whole drag is one undo step. */
    transform: (id: number, params: ComponentTransformParams, replace?: number) =>
      k().call<Revision>('model.transform', { id, params, replace }),
    /** `pressures`: a graphics tablet pen's pressure at each point, 0 to 1. */
    sculpt: (
      id: number,
      points: Point[],
      params: BrushParams,
      custom?: CustomBrushParams | null,
      pressures?: number[],
    ) =>
      k().call<Revision>('model.sculpt', {
        id,
        points,
        params,
        custom: custom ?? undefined,
        pressures,
      }),
    exportStl: (path: string, ids?: number[], base?: number) =>
      k().call<Revision>('model.exportStl', { path, ids, base }),
    /** Bakes components, in model order, into one new Add component at
     * height scale 1 and deletes them, one undo step. Needs two or more. */
    /** Moves (or with `copy` copies) components to the other side of a
     * double sided job, turned over. */
    toOtherSide: (ids: number[], copy: boolean) =>
      k().call<Revision & { ids: number[] }>('model.toOtherSide', { ids, copy }),
    bake: (ids: number[], name?: string) =>
      k().call<Revision & { id: number }>('model.bake', { ids, name }),
    /** Empties every cell of a component lower than `height` mm, measured
     * on the model with its height scale applied. */
    replaceBelow: (args: ReplaceBelowArgs) =>
      k().call<Revision & { id: number }>('model.replaceBelow', args),
    /** Splits a component by closed vectors, or clears one side. */
    split: (args: SplitArgs) => k().call<Revision & SplitReply>('model.split', args),
    list: () => k().call<ModelList>('model.list'),
    levels: {
      add: (name?: string, combine?: Combine) =>
        k().call<Revision & { id: number }>('model.levels.add', { name, combine }),
      update: (id: number, patch: LevelPatch) =>
        k().call<Revision>('model.levels.update', { id, ...patch }),
      delete: (id: number) => k().call<Revision>('model.levels.delete', { id }),
      reorder: (ids: number[]) => k().call<Revision>('model.levels.reorder', { ids }),
      setActive: (id: number) => k().call<Revision>('model.levels.setActive', { id }),
    },
    update: (
      id: number,
      patch: Partial<
        Pick<ComponentSummary, 'name' | 'visible' | 'combine' | 'heightScale' | 'level'>
      >,
    ) => k().call<Revision>('model.update', { id, ...patch }),
    delete: (ids: number[]) => k().call<Revision>('model.delete', { ids }),
    /** Removes every component and group, one undo step. */
    clear: () => k().call<Revision & { deleted: number[] }>('model.clear', {}),
    reorder: (ids: number[]) => k().call<Revision>('model.reorder', { ids }),
    /** Component tree: properties, groups, drag and drop, clipping. */
    tree: {
      /** `replace`: a live drag's previous step, replaced by this one. */
      props: (ids: number[], props: ComponentProps, replace?: number) =>
        k().call<Revision>('model.props', { ids, ...props, replace }),
      group: (ids: number[], name?: string) =>
        k().call<Revision & { id: number }>('model.groups.create', { ids, name }),
      updateGroup: (id: number, patch: GroupPatch) =>
        k().call<Revision>('model.groups.update', { id, ...patch }),
      ungroup: (id: number) =>
        k().call<Revision & { ids: number[] }>('model.groups.ungroup', { id }),
      transformGroup: (id: number, params: GroupTransformParams) =>
        k().call<Revision>('model.groups.transform', { id, params }),
      move: (ids: number[], target: TreeTarget) =>
        k().call<Revision>('model.tree.move', { ids, ...target }),
      setClip: (id: number, vectorIds: number[]) =>
        k().call<Revision>('model.levels.setClip', { id, vectorIds }),
      updateClip: (id: number, vectorIds: number[]) =>
        k().call<Revision>('model.levels.updateClip', { id, vectorIds }),
      fromVisible: (ids?: number[], name?: string) =>
        k().call<Revision & { id: number; hidden: number[] }>('model.fromVisible', { ids, name }),
      /** Where the components `ids` show (1) or are hidden (2) per model cell. */
      highlight: async (ids: number[]): Promise<HighlightReply> => {
        const reply = await k().callBlob<ColorsHeader>('model.highlight', { ids });
        const mask = reply.sections['mask'];
        return {
          width: reply.header.width,
          height: reply.header.height,
          mask: mask instanceof Uint8Array ? mask : new Uint8Array(0),
        };
      },
      colors: async (): Promise<PaletteColours> => {
        const reply = await k().callBlob<ColorsHeader>('model.colors', {});
        const palette = reply.sections['palette'];
        const index = reply.sections['index'];
        return {
          width: reply.header.width,
          height: reply.header.height,
          palette: palette instanceof Uint8Array ? palette : new Uint8Array(4),
          index:
            index instanceof Uint8Array || index instanceof Uint16Array ? index : new Uint8Array(0),
          any: reply.header.any,
          materials: reply.header.materials,
        };
      },
    },
    /** Model menu refinements; each change is one undo step. */
    refine: {
      addZeroPlane: () => k().call<Revision & { id: number }>('model.addZeroPlane', {}),
      /** Volume of the composite model above its zero plane, mm cubed. */
      /** The composite model's volume, or that of the components `ids`
       * baked together when some are given. */
      volume: (ids: number[] = []) =>
        k().call<{ volume: number; empty: boolean; selected: number }>('model.volume', { ids }),
      smooth: (args: SmoothComponentArgs) =>
        k().call<Revision & { id: number }>('model.smoothComponent', args),
      emboss: (args: EmbossComponentArgs) =>
        k().call<Revision & { id: number }>('model.embossComponent', args),
      scaleHeight: (args: ScaleHeightArgs) =>
        k().call<Revision & ScaleHeightReply>('model.scaleHeight', args),
      offset: (args: OffsetModelArgs) =>
        k().call<Revision & { id: number }>('model.offsetModel', args),
      addDraft: (angleDeg: number) =>
        k().call<Revision & { id: number }>('model.addDraft', { angleDeg }),
      fromPreview: (mode: PreviewComponentMode) =>
        k().call<Revision & { id: number }>('model.fromPreview', { mode }),
      /** Takes back the preview step made at `revision` if it is still the newest. */
      revertPreview: (revision: number) =>
        k().call<Revision & { reverted: boolean }>('model.revertPreview', { revision }),
    },
    /** The visible model, or only the components `ids` as they show alone;
     * with `otherSide`, the other side's model of a double sided job turned
     * over onto this side's grid. */
    composite: async (ids?: number[], otherSide = false): Promise<Composite> => {
      const reply = await k().callBlob<CompositeHeader>('model.composite', { ids, otherSide });
      return { ...reply.header, z: heightsOf(reply, 'z') ?? floatHeights(new Float32Array(0)) };
    },
  },

  sim: {
    /** Previews the toolpaths; `keep` cuts into the preview shown. */
    run: (quality: PreviewQuality, ids?: number[], keep?: boolean) =>
      k().call<SimRunReply>('sim.run', { quality, ids, keep }),
    reset: () => k().call<unknown>('sim.reset'),
    /** Starts an animated preview of the given toolpaths, all of them, or the visible ones. */
    start: (quality: PreviewQuality, ids?: number[], all?: boolean, keep?: boolean) =>
      k().call<SimStartReply>('sim.start', { quality, ids, all, keep }),
    /** Undo Last: the preview as it was before the last preview or waste removal. */
    undo: () => k().call<{ restored: boolean; empty: boolean }>('sim.undo', {}),
    /** Removes the piece cut free under job point (x, y); answers the cells removed. */
    removeWaste: (x: number, y: number) =>
      k().call<{ removed: number }>('sim.removeWaste', { x, y }),
    /** One animation frame: sim.step then sim.patch in a single call. */
    advance: async (args: SimStepArgs): Promise<{ step: SimStepReply; patch: SimPatch }> => {
      const reply = await k().callBlob<SimPatchHeader & { step: SimStepReply }>(
        'sim.advance',
        args,
      );
      return { step: reply.header.step, patch: patchOf(reply) };
    },
    /** Compares the preview shown with the design: gouges, leftover and undercuts. */
    check: async (tolerance: number): Promise<SimCheck> => {
      const reply = await k().callBlob<BlobHeader & Omit<SimCheck, 'map'>>('sim.check', {
        tolerance,
      });
      const map = reply.sections['check'];
      return { ...reply.header, map: map instanceof Uint8Array ? map : new Uint8Array(0) };
    },
    heightfield: async (): Promise<Heightfield> => {
      const reply = await k().callBlob<HeightfieldHeader>('sim.heightfield', {});
      const owner = reply.sections['owner'];
      return {
        ...reply.header,
        z: heightsOf(reply, 'z') ?? floatHeights(new Float32Array(0)),
        owner: owner instanceof Uint16Array ? owner : new Uint16Array(0),
        // Without a floor section the underside is the material bottom.
        floor: heightsOf(reply, 'floor') ?? 'flat',
        ...burnOf(reply),
      };
    },
  },

  io: {
    /** Imports a vector file; `page` picks the page of a PDF, counting from 1. */
    importVectors: (path: string, options: { layer?: number; page?: number } = {}) =>
      k().call<Revision & { ids: number[]; warnings: KernelWarning[] }>('io.importVectors', {
        path,
        options,
      }),
    /** Pages a vector file has to choose from: a PDF's page count, 1 otherwise. */
    vectorPages: (path: string) => k().call<{ pages: number }>('io.vectorPages', { path }),
    exportVectors: (path: string, format: VectorFormat, ids?: number[]) =>
      k().call<Revision>('io.exportVectors', { path, format, ids }),
  },

  bitmap: {
    /** Places an image file on the job as a selected bitmap. */
    import: (path: string) => k().call<BitmapImportReply>('io.importBitmap', { path }),
    /** The stored image file of a placed bitmap, for drawing. */
    pixels: async (imageId: number): Promise<Blob> => {
      const reply = await k().callBlob<BitmapPixelsHeader>('bitmaps.pixels', { imageId });
      const data = reply.sections['bytes'];
      const bytes = data instanceof Uint8Array ? new Uint8Array(data) : new Uint8Array(0);
      return new Blob([bytes], { type: reply.header.mime });
    },
    /** Fades bitmaps: 0 is invisible, 1 opaque. */
    setOpacity: (ids: number[], opacity: number) =>
      k().call<Revision & { ids: number[] }>('bitmap.setOpacity', { ids, opacity }),
    /** Edit Picture: tone, invert, greyscale and a faded border, one undo step. */
    editPicture: (args: EditPictureArgs) =>
      k().call<Revision & { id: number; imageId: number }>('bitmap.editPicture', args),
    /** Crop Bitmap: clears the picture outside closed vectors and trims it. */
    crop: (id?: number, boundaries?: number[]) =>
      k().call<Revision & { id: number; imageId: number }>('bitmap.crop', { id, boundaries }),
  },

  clipart: {
    scan: (dirs?: string[]) =>
      k().call<ClipartScan>('clipart.scan', dirs === undefined ? {} : { dirs }),
    insert: (args: ClipartInsertArgs) => k().call<ClipartInsertReply>('clipart.insert', args),
    save: (componentId: number, path: string, name?: string) =>
      k().call<{ path: string }>('clipart.save', { componentId, path, name }),
  },

  prefs: {
    get: () => k().call<PrefsReply>('prefs.get'),
    set: (patch: PrefsPatch) => k().call<PrefsReply>('prefs.set', patch),
    reset: () => k().call<PrefsReply>('prefs.reset'),
  },

  gadgets: {
    list: () => k().call<{ gadgets: GadgetSummary[] }>('gadgets.list'),
  },

  job: {
    get: () => k().call<JobSetup>('job.get'),
    set: (job: JobSetup) => k().call<JobSetup>('job.set', job),
    setSide: (side: Side) => k().call<Revision & { side: Side }>('job.setSide', { side }),
    swapSides: () => k().call<Revision & { side: Side }>('job.swapSides', {}),
    /** The other side's vectors turned over into this side's job space. */
    otherSidePaths: () => k().call<Revision & { paths: Path[] }>('job.otherSidePaths', {}),
  },

  undo: {
    undo: () => k().call<Revision & { label: string | null }>('undo'),
    redo: () => k().call<Revision & { label: string | null }>('redo'),
    info: () => k().call<UndoInfo>('undo.info'),
    /** The commands until endGroup undo as one step, called `label` or
     * after the first of them. */
    beginGroup: (label?: string) => k().call<Record<string, never>>('undo.beginGroup', { label }),
    endGroup: () => k().call<{ merged: boolean }>('undo.endGroup', {}),
  },

  layers: {
    list: () => k().call<Layer[]>('layers.list'),
    /** Adds a layer, in one undo step hidden, active or before `before` as asked. */
    add: (name?: string, colour?: string, options: NewLayerOptions = {}) =>
      k().call<Revision & { id: number }>('layers.add', { name, colour, ...options }),
    update: (
      id: number,
      patch: Partial<Pick<Layer, 'name' | 'colour' | 'visible' | 'locked' | 'keepOut'>>,
    ) => k().call<Revision>('layers.update', { id, ...patch }),
    delete: (id: number, moveTo?: number) => k().call<Revision>('layers.delete', { id, moveTo }),
    duplicate: (id: number) => k().call<Revision & { id: number }>('layers.duplicate', { id }),
    setActive: (id: number) => k().call<Revision>('layers.setActive', { id }),
    reorder: (ids: number[]) => k().call<Revision>('layers.reorder', { ids }),
  },

  sheets: {
    list: () => k().call<Sheet[]>('sheets.list'),
    /** Adds a sheet; `activate` also shows it, in the same undo step. */
    add: (name?: string, activate = false) =>
      k().call<Revision & { id: number }>('sheets.add', { name, activate }),
    /** Copies a sheet and everything on it, and shows the copy. */
    duplicate: (id: number) =>
      k().call<Revision & { id: number; ids: number[] }>('sheets.duplicate', { id }),
    /** Moves objects, and components when given, onto another sheet. */
    moveTo: (ids: number[], sheet: number, components?: number[]) =>
      k().call<Revision>(
        'entities.setSheet',
        components === undefined ? { ids, sheet } : { ids, sheet, components },
      ),
    /** Gives every sheet in `ids` the same size, thickness and datum. */
    edit: (ids: number[], material: SheetMaterial, scaleDesign = false) =>
      k().call<Revision>('sheets.edit', { ids, material, scaleDesign }),
    delete: (id: number) => k().call<Revision>('sheets.delete', { id }),
    setActive: (id: number) => k().call<Revision>('sheets.setActive', { id }),
    rename: (id: number, name: string) => k().call<Revision>('sheets.rename', { id, name }),
  },

  dims: {
    create: (spec: DimensionSpec, customText?: string, textAt?: Point) =>
      k().call<Revision & { id: number }>('dims.create', { ...spec, customText, textAt }),
  },

  entities: {
    /** Top level entities of a sheet, the active one when left out. */
    list: (sheet?: number) =>
      k().call<EntitySummary[]>('entities.list', sheet === undefined ? undefined : { sheet }),
    display: async (args: {
      ids?: number[];
      sinceRevision?: number;
      tolerance?: number;
    }): Promise<DisplayReply> => {
      const reply = await k().callBlob<DisplayHeader>('entities.display', args);
      const xy = reply.sections['xy'];
      return {
        revision: reply.header.revision,
        entities: reply.header.entities,
        removed: reply.header.removed ?? [],
        xy: xy instanceof Float32Array ? xy : new Float32Array(0),
      };
    },
    get: (id: number) => k().call<EntityJson>('entities.get', { id }),
    create: (entity: NewEntity) =>
      k().call<Revision & { id: number }>('entities.create', { entity }),
    replace: (id: number, entity: EntityJson) =>
      k().call<Revision>('entities.replace', { id, entity }),
    /** Deletes the entities and the relief `components` in one undo step. */
    delete: (ids: number[], components: number[] = []) =>
      k().call<Revision>('entities.delete', { ids, components }),
    transform: (ids: number[], matrix: Matrix) =>
      k().call<Revision>('entities.transform', { ids, matrix }),
    setLayer: (ids: number[], layer: number) =>
      k().call<Revision>('entities.setLayer', { ids, layer }),
    group: (ids: number[]) => k().call<Revision & { id: number }>('entities.group', { ids }),
    /** Ungroups; `deep` also takes nested groups apart, `toGroupLayer`
     * puts the released objects on the group's layer. */
    ungroup: (ids: number[], options: { deep?: boolean; toGroupLayer?: boolean } = {}) =>
      k().call<Revision & { ids: number[] }>('entities.ungroup', { ids, ...options }),
    probe: (args: ProbeArgs) => k().call<ProbeReply>('entities.probe', args),
    hitTest: (args: HitTestArgs) => k().call<{ ids: number[] }>('entities.hitTest', args),
  },

  /** The in-app clipboard, kept in the kernel so a cut can be pasted. */
  clipboard: {
    /** Keeps copies of the top level entities `ids` and the components. */
    copy: (ids: number[], components: number[]) =>
      k().call<{ count: number }>('clipboard.copy', { ids, components }),
    /** New copies moved by (dx, dy); the pasted entities are selected. */
    paste: (dx: number, dy: number) =>
      k().call<Revision & { ids: number[]; components: number[] }>('clipboard.paste', {
        dx,
        dy,
      }),
  },

  selection: {
    get: () => k().call<SelectionState>('selection.get'),
    set: (ids: number[]) => k().call<SelectionState>('selection.set', { ids }),
    add: (ids: number[]) => k().call<SelectionState>('selection.add', { ids }),
    remove: (ids: number[]) => k().call<SelectionState>('selection.remove', { ids }),
    toggle: (ids: number[]) => k().call<SelectionState>('selection.toggle', { ids }),
    clear: () => k().call<SelectionState>('selection.clear'),
    all: () => k().call<SelectionState>('selection.all'),
    /** Selects the closed boundaries (given or selected) and every vector inside them. */
    inside: (ids?: number[]) =>
      k().call<{ ids: number[]; added: number[]; revision: number }>('selection.inside', { ids }),
  },

  vectors: {
    /** Create Vector Texture; with `preview` it only answers the paths. */
    texture: (args: VectorTextureRequest & { ids?: number[]; preview?: boolean }) =>
      k().call<Revision & { ids?: number[]; paths?: Path[] }>('vectors.texture', args),
  },

  drawTools: {
    selectDuplicates: (tolerance: number, ids?: number[]) =>
      k().call<DuplicatesReply>('ops.selectDuplicates', { tolerance, ids }),
  },

  ops: {
    /** `smooth` bridges gaps with a curve tangent to both ends. */
    join: (ids: number[], tolerance: number, smooth = false) =>
      k().call<Revision & { ids: number[] }>('ops.join', { ids, tolerance, smooth }),
    close: (ids: number[], mode: CloseMode) => k().call<Revision>('ops.close', { ids, mode }),
    offset: (args: OffsetArgs) => k().call<Revision & { ids: number[] }>('ops.offset', args),
    /** Trim Objects: the last id is the boundary, which stays. */
    trimObjects: (ids: number[], componentIds: number[], side: TrimSide, refit: boolean) =>
      k().call<Revision & { ids: number[]; componentIds: number[] }>('ops.trimObjects', {
        ids,
        componentIds,
        side,
        refit,
      }),
    boolean: (ids: number[], kind: BooleanKind) =>
      k().call<Revision & { ids: number[] }>(`ops.${kind}`, { ids }),
    fillet: (id: number, x: number, y: number, radius: number, kind: FilletKind) =>
      k().call<Revision>('ops.fillet', { id, x, y, radius, kind }),
    /** `sharpAngle`: Keep Sharp Corners' Max Angle (degrees), or absent;
     * `replace` false keeps the originals and adds the fitted copies. */
    fitCurves: (ids: number[], tolerance: number, kind: FitKind, options: FitOptions = {}) =>
      k().call<Revision & { ids: number[] }>('ops.fitCurves', { ids, tolerance, kind, ...options }),
    validate: (ids: number[], vgroove = false) =>
      k().call<{ issues: ValidationIssue[] }>('ops.validate', { ids, vgroove }),
    /** Where the vectors cross themselves or each other (at most 100). */
    crossings: (ids: number[]) =>
      k().call<{ crossings: { ids: number[]; x: number; y: number }[] }>('ops.crossings', {
        ids,
      }),
    boundary: (ids: number[], offset: number, kind: BoundaryKind, rubberBand = false) =>
      k().call<Revision & { ids: number[] }>('ops.boundary', { ids, offset, kind, rubberBand }),
    /** `components` are copied into the grid with the vectors. */
    arrayCopy: (ids: number[], args: ArrayCopyArgs, components: number[] = []) =>
      k().call<Revision & { ids: number[]; components: number[] }>('ops.arrayCopy', {
        ids,
        components,
        ...args,
      }),
    /** `components` go round the ring with the vectors. */
    circularArray: (ids: number[], args: CircularArrayArgs, components: number[] = []) =>
      k().call<Revision & { ids: number[]; components: number[] }>('ops.circularArray', {
        ids,
        components,
        ...args,
      }),
    /** `components` are copied along the vector with the others (and deleted
     * with them when the originals are). */
    copyAlong: (ids: number[], args: CopyAlongArgs, components: number[] = []) =>
      k().call<Revision & { ids: number[]; components: number[] }>('ops.copyAlong', {
        ids,
        components,
        ...args,
      }),
    move: (ids: number[], dx: number, dy: number) =>
      k().call<Revision>('ops.move', { ids, dx, dy }),
    rotate: (ids: number[], angleDeg: number, cx: number, cy: number) =>
      k().call<Revision>('ops.rotate', { ids, angleDeg, cx, cy }),
    mirror: (ids: number[], axis: 'h' | 'v', copy: boolean) =>
      k().call<Revision & { ids: number[] }>('ops.mirror', { ids, axis, copy }),
    /** Mirrors (or copies mirrored) about the line from `line` to `lineTo`. */
    mirrorLine: (ids: number[], line: [number, number], lineTo: [number, number], copy: boolean) =>
      k().call<Revision & { ids: number[] }>('ops.mirror', {
        ids,
        axis: 'line',
        line,
        lineTo,
        copy,
      }),
    align: (ids: number[], mode: string, target: 'selection' | 'material' | 'last') =>
      k().call<Revision>('ops.align', { ids, mode, target }),
    distribute: (ids: number[], mode: string, spacing: number | null) =>
      k().call<Revision>('ops.distribute', { ids, mode, spacing }),
    node: {
      insert: (id: number, x: number, y: number) =>
        k().call<Revision>('ops.node.insert', { id, x, y }),
      delete: (id: number, subpath: number, node: number) =>
        k().call<Revision>('ops.node.delete', { id, subpath, node }),
      convert: (id: number, subpath: number, segment: number, to: 'line' | 'arc' | 'cubic') =>
        k().call<Revision>('ops.node.convert', { id, subpath, segment, to }),
      smooth: (id: number, subpath: number, node: number) =>
        k().call<Revision>('ops.node.smooth', { id, subpath, node }),
      setStart: (id: number, subpath: number, node: number) =>
        k().call<Revision>('ops.node.setStart', { id, subpath, node }),
      cut: (id: number, subpath: number, node: number) =>
        k().call<Revision>('ops.node.cut', { id, subpath, node }),
      /** Joins an open end to the nearest other open end (J in node editing). */
      join: (id: number, subpath: number, atStart: boolean) =>
        k().call<Revision & { id: number }>('ops.node.join', { id, subpath, atStart }),
    },
  },
};

export type Api = typeof api;
export type { Bbox };
