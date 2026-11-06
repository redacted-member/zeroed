// The kernel commands only forms, dialogs and the other modules that load
// on demand use, added to the start bundle's `api` (bridge/api.ts). Those
// modules import `api` from here, so these wrappers load with the first
// form instead of at start.

import {
  pieceRef,
  type AddableComponent,
  type ComponentPlace,
  type ComponentRelief,
  type DropLanding,
  type PiecePose,
  type ExplodeStep,
  type MateArgs,
  type MateEdit,
  type MateReply,
  type MateRow,
  type PieceAction,
  type PieceCutRow,
  type PieceFace,
  type PieceFaceHit,
  type PieceIssue,
  type PieceRef,
  type PieceRow,
  type SolvePlaceReply,
  type Triple,
} from './assembly';
import type { ExportAssemblyReply, MeasureReply, VerifyFitReply } from './assemblyFit';
import type {
  EdgeLine,
  JointList,
  JointOutcome,
  JointParams,
  JointPreview,
  JointSettings,
  JointSpec,
  PickEdgeReply,
  PickFaceReply,
} from './joints';
import type {
  AddModelArgs,
  AttachmentRow,
  StockSize,
  StockThickness,
  ConnectionRow,
  CreateArgs,
  CutListRow,
  DesignReply,
  DesignRow,
  FurnitureEstimate,
  FurnitureJoints,
  FurniturePreview,
  FurnitureTemplate,
  FurnitureValues,
  HardwareRow,
  SetJointArgs,
  TemplateSummary,
  UpdateArgs,
  Vec3,
} from './furniture';
import { heightsOf } from './blob';
import { floatHeights } from './heights';
import type { FreehandArgs, MeasureHit, SnipReply } from './drawTools';
import type {
  ImportSource,
  MeshInfo,
  MeshOrientation,
  MeshPlacement,
  MeshPreview,
  MeshPreviewHeader,
  RotaryFit,
  SegmentPlan,
} from './meshImport';
import type {
  ArcTextArgs,
  DimensionSpec,
  DimensionStyleSettings,
  FilletKind,
  Path,
  TextPreviewArgs,
  GlyphBox,
  TextArc,
  NestParams,
  NestReply,
  NestSheetUse,
  Point,
  Revision,
} from './document';
import type {
  Heightfield,
  HeightfieldHeader,
  PostText,
  PostChangeLog,
  CuttingData,
  NameFormats,
  ToolImportMode,
  SaveToolpathsArgs,
  SimPatch,
  SimPatchHeader,
  SimStepArgs,
  SimStepReply,
  TabOp,
  TabSpec,
  Tool,
  ToolpathDetail,
  ImportedEdit,
  ImportedToolpath,
} from './cam';
import type {
  Combine,
  AppliedComponent,
  ApplyTarget,
  ShapeArgs,
  SpinParams,
  SweepParams,
  ExtrudeParams,
  TextureParams,
  TileBox,
  TurnParams,
} from './model';
import type {
  CrossSectionArgs,
  CrossSectionReply,
  SliceArgs,
  SlicePreview,
  SliceReply,
} from './modelTools';
import type { BlobReply, JobSetup } from './types';
import type { GadgetDescription, GadgetRunReply, GadgetValue } from './gadgets';
import type { ThumbnailHeader } from './clipart';
import type { JobScaledReply } from './templates';
import type { SelectedVectors, SketchTrace, VectorFilter } from './sketch';
import {
  api as startApi,
  k,
  patchOf,
  type BitmapInfo,
  type BitmapPixelsHeader,
  type BitmapTraceArgs,
  type DistortArgs,
  type PlateSeparator,
  type PlatesData,
  type PlatesProduceArgs,
  type StoredDistortion,
  type TraceImageOptions,
  type TraceIndicesHeader,
  type TracePalette,
  type TraceParams,
  type TracePreview,
} from './api';

/** Unpacks a mesh preview reply into typed arrays. */
async function meshPreview(pending: Promise<BlobReply<MeshPreviewHeader>>): Promise<MeshPreview> {
  const reply = await pending;
  const { xyz, flags, segment } = reply.sections;
  return {
    size: reply.header.size,
    centre: reply.header.centre,
    xyz: xyz instanceof Float32Array ? xyz : new Float32Array(0),
    flags: flags instanceof Uint8Array ? flags : new Uint8Array(0),
    segment: segment instanceof Uint16Array ? segment : new Uint16Array(0),
    segments: reply.header.segments ?? [],
    ...(reply.header.rotary === true && reply.header.cylinder !== undefined
      ? { cylinder: reply.header.cylinder }
      : {}),
  };
}

export const api = {
  ...startApi,
  doc: {
    ...startApi.doc,
    setNotes: (notes: string) => k().call<Revision>('doc.notes.set', { notes }),
    /** Writes the variables to a text file, one `name = value` a line. */
    exportVariables: (path: string) =>
      k().call<{ count: number }>('doc.variables.export', { path }),
    /** Reads variables from such a file; same names are replaced. */
    importVariables: (path: string) =>
      k().call<Revision & { count: number }>('doc.variables.import', { path }),
  },
  text: {
    ...startApi.text,
    /** Every font family, and those of them that are single line (engraving) fonts. */
    fonts: () => k().call<{ families: string[]; singleLine?: string[] }>('text.fonts'),
    preview: (args: TextPreviewArgs) => k().call<{ paths: Path[] }>('text.preview', args),
    arc: (args: ArcTextArgs) => k().call<Revision & { id: number }>('text.arc', args),
    /** The drawn letters of a text and its height, for the Text Selection tool. */
    glyphs: (id: number) =>
      k().call<{ glyphs: GlyphBox[]; height: number; arc: TextArc | null; rotation?: number }>(
        'text.glyphs',
        { id },
      ),
  },
  tools: {
    ...startApi.tools,
    save: (tool: Tool, group: number) =>
      k().call<{ id: number; dbRevision: number }>('tools.save', { tool, group }),
    delete: (ids: number[]) => k().call<{ deleted: number[] }>('tools.delete', { ids }),
    copy: (id: number, group?: number) => k().call<{ id: number }>('tools.copy', { id, group }),
    /** Adds the shipped tools the database has no tool of the same name for. */
    addDefaults: () => k().call<{ toolIds: number[] }>('tools.addDefaults'),
    move: (id: number, group: number) => k().call<unknown>('tools.move', { id, group }),
    addGroup: (name: string, parent: number | null) =>
      k().call<{ id: number }>('tools.groups.add', { name, parent }),
    renameGroup: (id: number, name: string) =>
      k().call<unknown>('tools.groups.rename', { id, name }),
    deleteGroup: (id: number) => k().call<unknown>('tools.groups.delete', { id }),
    addMachine: (name: string) => k().call<{ added: boolean }>('tools.machines.add', { name }),
    renameMachine: (from: string, to: string) =>
      k().call<unknown>('tools.machines.rename', { from, to }),
    deleteMachine: (name: string) =>
      k().call<{ removed: boolean }>('tools.machines.delete', { name }),
    /** Drops a tool or group into a group, before `before` or last. */
    place: (item: { kind: 'tool' | 'group'; id: number }, group: number, before: number | null) =>
      k().call<unknown>(
        'tools.place',
        item.kind === 'tool'
          ? { kind: 'tool', id: item.id, group, before }
          : { kind: 'group', id: item.id, parent: group === 0 ? null : group, before },
      ),
    addMaterial: (name: string) => k().call<{ added: boolean }>('tools.materials.add', { name }),
    renameMaterial: (from: string, to: string) =>
      k().call<unknown>('tools.materials.rename', { from, to }),
    deleteMaterial: (name: string) =>
      k().call<{ removed: boolean }>('tools.materials.delete', { name }),
    activateMaterial: (name: string) => k().call<unknown>('tools.materials.activate', { name }),
    duplicateMaterial: (from: string, to: string, copyCuttingData: boolean) =>
      k().call<{ copied: boolean }>('tools.materials.duplicate', { from, to, copyCuttingData }),
    setCuttingData: (id: number, data: CuttingData) =>
      k().call<unknown>('tools.cuttingData.set', { id, ...data }),
    nameFormats: () => k().call<NameFormats>('tools.nameFormats.list'),
    setNameFormat: (kind: string, format: string, updateExisting: boolean) =>
      k().call<{ renamed: number[] }>('tools.nameFormats.set', { kind, format, updateExisting }),
    previewName: (tool: Tool) => k().call<{ name: string }>('tools.previewName', { tool }),
    import: (path: string, mode: ToolImportMode, group: number) =>
      k().call<{ toolIds: number[]; updated: number[] }>('tools.import', { path, mode, group }),
    export: (path: string, target: { group: number } | { toolIds: number[] }) =>
      k().call<{ path: string }>('tools.export', { path, ...target }),
    /** The half section of a form cutter from the selected vector. */
    formProfile: () => k().call<{ profile: unknown }>('tools.formProfile'),
  },
  posts: {
    ...startApi.posts,
    text: (id: string) => k().call<PostText>('posts.text', { id }),
    /** The History table in a post's leading comments. */
    changelog: (id: string) => k().call<PostChangeLog>('posts.changelog', { id }),
    remove: (id: string) => k().call<unknown>('posts.remove', { id }),
    /** Copies a post into the user's posts under a new id, to edit. */
    customize: (id: string) => k().call<{ id: string }>('posts.customize', { id }),
  },
  cam: {
    ...startApi.cam,
    get: (id: number) => k().call<ToolpathDetail>('cam.get', { id }),
    save: (args: SaveToolpathsArgs) => k().call<{ paths: string[] }>('cam.save', args),
    imported: {
      get: (id: number) => k().call<ImportedToolpath>('cam.imported.get', { id }),
      /** Moves, deepens, renames or refeeds an imported toolpath; one undo step. */
      edit: (change: ImportedEdit) => k().call<Revision>('cam.imported.edit', change),
    },
    /** The depth that carves the selected vectors into full prisms with the tool. */
    prismDepth: (tool: Tool, ids: readonly number[]) =>
      k().call<{ depth: number }>('cam.prismDepth', { tool, ids }),
    /** Edit Tabs: tab positions after placing them, a click, a drag or Delete All Tabs. */
    tabs: (args: {
      ids: readonly number[];
      tabs: TabSpec;
      positions: Point[];
      op: TabOp;
      at?: Point;
      from?: Point;
      /** How near a click must be to a tab or vector, in millimetres. */
      reach: number;
    }) => k().call<{ positions: Point[] }>('cam.tabs', args),
    /** The sketch lines Sketch Carving or Laser Sketch would machine, for the form's preview. */
    sketchPreview: (params: SketchTrace, ids: number[]) =>
      k().call<{ paths: Path[] }>('cam.sketchPreview', { params, ids }),
    /** The objects on the active sheet a Vector Selector filter picks. */
    selectVectors: (filter: VectorFilter) =>
      k().call<SelectedVectors>('cam.selectVectors', { filter }),
    /** Plug Destination Sheet: mirrored copies of the inlay vectors on `sheet`
     * (the active sheet when absent), centered on it when it is another sheet. */
    inlayPlugVectors: (ids: readonly number[], sheet?: number) =>
      k().call<{ ids: number[] }>('cam.inlayPlugVectors', { ids, sheet }),
  },
  model: {
    ...startApi.model,
    createShape: (args: ShapeArgs) => k().call<AppliedComponent>('model.createShape', args),
    sweep: (
      args: {
        rails: [number, number];
        sections: number[];
        params: SweepParams;
        combine: Combine;
        name?: string;
      } & ApplyTarget,
    ) => k().call<AppliedComponent>('model.sweep', args),
    /** Extrude and Weave: `sections` along `rails`, or `component` when
     * given. */
    extrude: (
      args: {
        rails: number[];
        sections: number[];
        component?: number;
        params: ExtrudeParams;
        combine: Combine;
        name?: string;
      } & ApplyTarget,
    ) => k().call<AppliedComponent>('model.extrude', args),
    textureArea: (
      args: {
        /** The seed component. */
        component: number;
        /** Closed vectors bounding the area; none fills the job (or keeps
         * the edited area's outline). */
        ids: number[];
        params: TextureParams;
        combine: Combine;
        name?: string;
      } & ApplyTarget,
    ) => k().call<AppliedComponent & { tile: TileBox | null }>('model.textureArea', args),
    turn: (
      args: { id: number; params: TurnParams; combine: Combine; name?: string } & ApplyTarget,
    ) => k().call<AppliedComponent>('model.turn', args),
    spin: (
      args: { id: number; params: SpinParams; combine: Combine; name?: string } & ApplyTarget,
    ) => k().call<AppliedComponent>('model.spin', args),
    /** Forgets what the undo brush restores to; the next stroke starts afresh. */
    sculptEnd: () => k().call<Revision>('model.sculptEnd', {}),
    /** Keep: the sculpting so far is what Discard goes back to. */
    sculptKeep: (id: number) => k().call<Record<string, never>>('model.sculptKeep', { id }),
    /** Discard (back to the last Keep) or Cancel (back to the start). */
    sculptRevert: (id: number, to: 'kept' | 'start') =>
      k().call<Revision & { reverted: boolean }>('model.sculptRevert', { id, to }),
    /** Writes the component into the brushes folder as a sculpting brush. */
    createBrush: (id: number, name?: string) =>
      k().call<{ path: string }>('model.createBrush', { id, name }),
    brushFolder: () => k().call<{ path: string }>('model.brushFolder', {}),
    /** Draws the model's profile along a line as an open vector. */
    crossSection: (args: CrossSectionArgs) =>
      k().call<Revision & CrossSectionReply>('model.crossSection', args),
    /** The Z slabs a slice would make, without changing anything. */
    slicePreview: (args: SliceArgs) => k().call<SlicePreview>('model.slicePreview', args),
    /** Cuts the model into stacked slab components, one undo step. */
    slice: (args: SliceArgs) => k().call<Revision & SliceReply>('model.slice', args),
    /** 3D model import: Orientate, Position and Segment steps, and Reimport. */
    import: {
      info: (path: string, orient: MeshOrientation, rotary?: RotaryFit) =>
        k().call<MeshInfo>('model.import.info', { path, orient, rotary }),
      preview: (path: string, orient: MeshOrientation, rotary?: RotaryFit) =>
        meshPreview(
          k().callBlob<MeshPreviewHeader>('model.import.preview', { path, orient, rotary }),
        ),
      apply: (args: {
        path: string;
        orient: MeshOrientation;
        place: MeshPlacement;
        combine: Combine;
        replace?: number[];
        /** A full 3D model on a rotary job. */
        rotary?: RotaryFit;
        resizeMaterial?: boolean;
      }) =>
        k().call<Revision & { ids: number[]; otherSide: number | null }>(
          'model.import.apply',
          args,
        ),
      segmentPreview: (path: string, orient: MeshOrientation, plan: SegmentPlan, layout: boolean) =>
        meshPreview(
          k().callBlob<MeshPreviewHeader>('model.import.segmentPreview', {
            path,
            orient,
            plan,
            layout,
          }),
        ),
      segmentApply: (args: {
        path: string;
        orient: MeshOrientation;
        plan: SegmentPlan;
        combine: Combine;
        replace?: number[];
      }) =>
        k().call<Revision & { ids: number[]; vectors: number[] }>(
          'model.import.segmentApply',
          args,
        ),
      settings: (id: number) =>
        k().call<{ source: ImportSource | null; ids: number[] }>('model.import.settings', { id }),
    },
  },
  sim: {
    ...startApi.sim,
    /** A material texture's picture file, to decode for the 3D view. */
    texture: async (path: string): Promise<Blob> => {
      const reply = await k().callBlob<Omit<BitmapPixelsHeader, 'imageId' | 'width' | 'height'>>(
        'sim.texture',
        { path },
      );
      const data = reply.sections['bytes'];
      const bytes = data instanceof Uint8Array ? new Uint8Array(data) : new Uint8Array(0);
      return new Blob([bytes], { type: reply.header.mime });
    },
    step: (args: SimStepArgs) => k().call<SimStepReply>('sim.step', args),
    /** The stock cells the animated preview changed since the last patch. */
    patch: async (): Promise<SimPatch> => {
      const reply = await k().callBlob<SimPatchHeader>('sim.patch', {});
      return patchOf(reply);
    },
  },
  io: {
    ...startApi.io,
    /** Writes `text` to `path` as UTF-8 (cut list CSV). */
    saveText: (path: string, text: string) =>
      k().call<Record<string, never>>('io.saveText', { path, text }),
    /** Reads a small UTF-8 file (saved form settings). */
    readText: (path: string) => k().call<{ text: string }>('io.readText', { path }),
  },
  assembly: {
    /** The Assembly View's pieces: one per part each sheet's toolpaths cut free, with its place. */
    pieces: () =>
      k().call<
        Revision & {
          pieces: PieceRow[];
          components?: AddableComponent[];
          mates?: MateRow[];
          explodeSteps?: ExplodeStep[];
        }
      >('assembly.pieces'),
    /** Keeps the named explode steps, in order (id 0 for a new one); one undo step named `label`. */
    setExplodeSteps: (steps: ExplodeStep[], label: string) =>
      k().call<Revision & { steps: ExplodeStep[] }>('assembly.setExplodeSteps', { steps, label }),
    /** A piece's simulation relief: its part of the sheet's simulation, all else cut away. */
    pieceRelief: async (piece: PieceRef): Promise<Heightfield> => {
      const reply = await k().callBlob<HeightfieldHeader>('assembly.pieceRelief', pieceRef(piece));
      const owner = reply.sections['owner'];
      return {
        ...reply.header,
        z: heightsOf(reply, 'z') ?? floatHeights(new Float32Array(0)),
        owner: owner instanceof Uint16Array ? owner : new Uint16Array(0),
        floor: heightsOf(reply, 'floor') ?? 'flat',
      };
    },
    /** Places a piece; one undo step. */
    placePiece: (piece: PieceRef, position: Triple, rotation: Triple) =>
      k().call<Revision>('assembly.placePiece', { ...pieceRef(piece), position, rotation }),
    /** Where every piece would end were this one placed there, the mates solved; reads only. */
    solvePlace: (piece: PieceRef, position: Triple, rotation: Triple) =>
      k().call<SolvePlaceReply>('assembly.solvePlace', { ...pieceRef(piece), position, rotation }),
    /** Mates a picked face of a piece to one of another and solves the mates; one undo step. */
    matePieces: (args: MateArgs) => k().call<MateReply>('assembly.matePieces', args),
    /** Changes a mate and solves the mates again; one undo step. */
    editMate: (id: number, edit: MateEdit) =>
      k().call<Revision>('assembly.editMate', { id, ...edit }),
    /** Deletes a mate; the pieces stay. One undo step. */
    deleteMate: (id: number) => k().call<Revision>('assembly.deleteMate', { id }),
    /** Hides, shows, isolates or grounds a piece (Show All: every piece); one undo step. */
    pieceState: (action: PieceAction, piece: PieceRef | null) =>
      k().call<Revision>('assembly.pieceState', {
        action,
        piece: piece === null ? null : pieceRef(piece),
      }),
    /** Pieces that overlap or come closer than `clearance` mm. */
    checkPieces: (clearance: number) =>
      k().call<{ issues: PieceIssue[]; clearance: number }>('assembly.checkPieces', {
        clearance,
      }),
    pieceCutList: () => k().call<{ rows: PieceCutRow[] }>('assembly.pieceCutList'),
    /** The face of a piece at a point on its surface (the one a mate would take), or null. */
    pieceFace: (face: PieceFace) =>
      k().call<{ face: PieceFaceHit | null }>('assembly.pieceFace', face),
    /** Whether a sheet's piece is its raw model or its toolpath simulation; one undo step. */
    pieceSource: (sheet: number, model: boolean) =>
      k().call<Revision>('assembly.pieceSource', { sheet, model }),
    /**
     * Shows components as pieces of their own; one undo step. `at` stands
     * the first on a point (the middle of its underside), `beside` stands
     * each on the ground beside the pieces.
     */
    addComponentPieces: (components: number[], place: ComponentPlace = {}) =>
      k().call<Revision & { added: number[] }>('assembly.addComponentPieces', {
        components,
        ...place,
      }),
    /**
     * Where a component let go lands (reads only): flush against the face
     * under the pointer, clear of every piece.
     */
    dropPlace: (component: number, landing: DropLanding) =>
      k().call<PiecePose & Revision>('assembly.dropPlace', {
        component,
        point: landing.point,
        ...(landing.onto === null ? {} : { onto: pieceRef(landing.onto) }),
      }),
    /** A component's relief standing alone and its box, before it is a piece. */
    componentRelief: async (component: number): Promise<ComponentRelief> => {
      const reply = await k().callBlob<HeightfieldHeader & { size: Triple; component: number }>(
        'assembly.componentRelief',
        { component },
      );
      const owner = reply.sections['owner'];
      return {
        ...reply.header,
        z: heightsOf(reply, 'z') ?? floatHeights(new Float32Array(0)),
        owner: owner instanceof Uint16Array ? owner : new Uint16Array(0),
        floor: heightsOf(reply, 'floor') ?? 'flat',
      };
    },
    /** A component's shaded thumbnail as a PNG blob. */
    componentThumbnail: async (component: number, size?: number): Promise<Blob> => {
      const reply = await k().callBlob<ThumbnailHeader>('assembly.componentThumbnail', {
        component,
        size,
      });
      const png = reply.sections['png'];
      const bytes = png instanceof Uint8Array ? new Uint8Array(png) : new Uint8Array(0);
      return new Blob([bytes], { type: 'image/png' });
    },
    /** Takes a component piece out of the Assembly View; one undo step. */
    removeComponentPiece: (component: number) =>
      k().call<Revision>('assembly.removeComponentPiece', { component }),
    /** Every joint between the placed pieces with its clearance, and each piece's explode. */
    verifyFit: (target: number, tolerance: number, leaveOut: PieceRef[] = []) =>
      k().call<VerifyFitReply>('assembly.verifyFit', {
        target,
        tolerance,
        leaveOut: leaveOut.map(pieceRef),
      }),
    /** The distance between two points picked on pieces, snapped to their faces. */
    measure: (a: PieceFace, b: PieceFace) => k().call<MeasureReply>('assembly.measure', { a, b }),
    /** Writes the placed pieces, as assembled, to one STL or 3MF file. */
    exportAssembly: (path: string, format: 'stl' | '3mf') =>
      k().call<ExportAssemblyReply>('assembly.exportAssembly', { path, format }),
  },
  bitmap: {
    ...startApi.bitmap,
    info: (path: string) => k().call<BitmapInfo>('bitmap.info', { path }),
    trace: (args: BitmapTraceArgs) => k().call<Revision & { ids: number[] }>('bitmap.trace', args),
    /** Traces a placed bitmap into vectors laid over it. */
    traceImage: (id: number, params: TraceParams, options: TraceImageOptions = {}) =>
      k().call<Revision & { ids: number[] }>('bitmap.traceImage', { id, params, ...options }),
    /** The outlines Trace Bitmap would make, without making them. */
    tracePreview: (id: number, params: TraceParams, options: TraceImageOptions = {}) =>
      k().call<TracePreview>('bitmap.traceImage', { id, params, ...options, preview: true }),
    /** Whether a placed bitmap is in colour, and its palette reduced to `colours`. */
    tracePalette: (id: number, colours: number) =>
      k().call<TracePalette>('bitmap.tracePalette', { id, colours }),
    /** Each pixel's palette index, row by row from the top; 255 where transparent. */
    traceIndices: async (id: number, colours: number) => {
      const reply = await k().callBlob<TraceIndicesHeader>('bitmap.traceIndices', { id, colours });
      const data = reply.sections['indices'];
      return {
        width: reply.header.width,
        height: reply.header.height,
        indices: data instanceof Uint8Array ? data : new Uint8Array(0),
      };
    },
    /** The palette index under a job point, null off the picture. */
    colourAt: (id: number, colours: number, x: number, y: number) =>
      k().call<{ index: number | null }>('bitmap.colourAt', { id, colours, x, y }),
  },
  clipart: {
    ...startApi.clipart,
    /** The clip's 256 px thumbnail as a PNG blob. */
    thumbnail: async (path: string, size?: number): Promise<Blob> => {
      const reply = await k().callBlob<ThumbnailHeader>('clipart.thumbnail', { path, size });
      const png = reply.sections['png'];
      const bytes = png instanceof Uint8Array ? new Uint8Array(png) : new Uint8Array(0);
      return new Blob([bytes], { type: 'image/png' });
    },
  },
  gadgets: {
    ...startApi.gadgets,
    describe: (name: string) => k().call<GadgetDescription>('gadgets.describe', { name }),
    /** Runs a gadget as one undo step; a script error rejects with its message.
     * With `toolpathId`, the run edits that toolpath (which the gadget made) in place. */
    run: (name: string, values: Record<string, GadgetValue>, toolpathId?: number) =>
      k().call<GadgetRunReply>(
        'gadgets.run',
        toolpathId === undefined ? { name, values } : { name, values, toolpathId },
      ),
  },
  samples: {
    /** Opens a finished job built into the app as a new untitled job. */
    open: (id: string) => k().call<Revision>('samples.open', { id }),
  },

  /** Furniture Designer: parametric templates made into panels, joints, toolpaths and pieces. */
  furniture: {
    ...startApi.furniture,
    templates: () => k().call<{ templates: TemplateSummary[] }>('furniture.templates'),
    template: (id: string) => k().call<FurnitureTemplate>('furniture.template', { id }),
    importTemplate: (json: string) =>
      k().call<{ id: string }>('furniture.importTemplate', { json }),
    /** A user template from a design, its current values as the new defaults. */
    saveTemplate: (design: number, id: string, name: string) =>
      k().call<{ id: string }>('furniture.saveTemplate', { design, id, name }),
    removeTemplate: (id: string) => k().call<unknown>('furniture.removeTemplate', { id }),
    /** Panels as meshes for a template (or a design) at these values; changes nothing. */
    preview: (
      source: { template: string } | { design: number },
      values: FurnitureValues,
      joints: FurnitureJoints,
    ) => k().call<FurniturePreview>('furniture.preview', { ...source, values, joints }),
    /** Builds a design in the document: sheets, panels, joints, toolpaths and pieces; one undo step. */
    create: (args: CreateArgs) => k().call<DesignReply>('furniture.create', args),
    /** Regenerates a design, reusing the ids of what still exists; one undo step. */
    update: (args: UpdateArgs) => k().call<DesignReply>('furniture.update', args),
    remove: (design: number, keepVectors: boolean) =>
      k().call<Revision>('furniture.remove', { design, keepVectors }),
    list: () => k().call<{ designs: DesignRow[] }>('furniture.list'),
    connections: (design: number) =>
      k().call<{ connections: ConnectionRow[] }>('furniture.connections', { design }),
    /** Sets (or with a null joint drops) a joint override; one undo step. */
    setJoint: (args: SetJointArgs) => k().call<DesignReply>('furniture.setJoint', args),
    /** The connection whose seam is nearest a pick ray, within 8 mm. */
    at: (design: number, point: Vec3, direction: Vec3) =>
      k().call<{ connection: ConnectionRow | null }>('furniture.at', { design, point, direction }),
    cutList: (design: number) => k().call<{ rows: CutListRow[] }>('furniture.cutList', { design }),
    hardware: (design: number) =>
      k().call<{ rows: HardwareRow[] }>('furniture.hardware', { design }),
    estimate: (design: number) => k().call<FurnitureEstimate>('furniture.estimate', { design }),
    /** Stock sheet thicknesses and sizes. */
    materials: () =>
      k().call<{ thicknesses: StockThickness[]; sizes: StockSize[] }>('furniture.materials', {}),
    /** Puts clipart, a component or a model file on a panel's broad face; one undo step. */
    addModel: (args: AddModelArgs) =>
      k().call<Revision & { design: number; ids: number[] }>('furniture.addModel', args),
    attachments: (design: number) =>
      k().call<{ attachments: AttachmentRow[] }>('furniture.attachments', { design }),
    /** Stretch a model on a face with its panel, or keep its size; one undo step. */
    setStretch: (design: number, kind: AttachmentRow['kind'], id: number, stretch: boolean) =>
      k().call<Revision & { design: number }>('furniture.setStretch', {
        design,
        kind,
        id,
        stretch,
      }),
    /** 3D roughing and finishing for the models on the design's panels; one undo step. */
    makeCarving: (design: number) =>
      k().call<Revision & { ids: number[] }>('furniture.makeCarving', { design }),
  },
  /** Joint Creator: joints between flat parts, made on their vectors. */
  joints: {
    list: () => k().call<JointList>('joints.list'),
    pickEdge: (x: number, y: number, tolerance: number) =>
      k().call<PickEdgeReply>('joints.pickEdge', { x, y, tolerance }),
    pickFace: (x: number, y: number, a: EdgeLine | null) =>
      k().call<PickFaceReply>('joints.pickFace', { x, y, a }),
    at: (x: number, y: number, tolerance: number) =>
      k().call<{ joint: number | null }>('joints.at', { x, y, tolerance }),
    preview: (spec: JointSpec, id: number | null) =>
      k().call<JointPreview>('joints.preview', { ...spec, id }),
    add: (spec: JointSpec) =>
      k().call<Revision & { id: number; outcome: JointOutcome; mates: number }>('joints.add', spec),
    update: (id: number, spec: JointSpec) =>
      k().call<Revision & { id: number; outcome: JointOutcome }>('joints.update', {
        ...spec,
        id,
      }),
    remove: (id: number) => k().call<Revision>('joints.remove', { id }),
    setSettings: (settings: JointSettings) =>
      k().call<Revision>('joints.setSettings', { settings }),
    box: (ids: number[], kind: JointSpec['kind'], params: JointParams, dryRun = false) =>
      k().call<Revision & { ids?: number[]; pairs: Array<{ a: EdgeLine; b: EdgeLine }> }>(
        'joints.box',
        { ids, kind, params, dryRun },
      ),
    /** Also keeps an Assembly View mate for each joint whose parts are now pieces. */
    makeToolpaths: (tool: Tool) =>
      k().call<Revision & { ids: number[]; mates: number }>('joints.makeToolpaths', { tool }),
    /** Puts a joint's second part against its first in the Assembly View, keeping its mate. */
    assemble: (id: number) => k().call<Revision & { mates: number }>('joints.assemble', { id }),
    coupon: (kind: JointSpec['kind'], params: JointParams) =>
      k().call<Revision & { id: number; parts: number[] }>('joints.coupon', { kind, params }),
  },
  job: {
    ...startApi.job,
    /** Job Setup with Scale Design: the design stretches with the job; one undo step. */
    setScaled: (job: JobSetup) => k().call<JobScaledReply>('job.setScaled', { job }),
    /** Rotary axis change that turns the design with the material, one undo step. */
    setFlipped: (job: JobSetup) =>
      k().call<{ job: JobSetup; revision: number }>('job.setFlipped', { job }),
    /** Vector Unwrapper of a rotary job; one undo step. */
    unwrapVectors: (ids: number[], direction: 'unwrap' | 'wrap') =>
      k().call<Revision & { ids: number[] }>('rotary.unwrapVectors', { ids, direction }),
    /** Vector Unwrapper: unrolls a closed contour about a centre into a new
     * open vector; one undo step. */
    unwrapContour: (id: number, centre: Point, refine: boolean) =>
      k().call<Revision & { id: number }>('rotary.unwrapContour', { id, centre, refine }),
  },
  dims: {
    ...startApi.dims,
    /** Edits an existing dimension in one step: its points, where its text
     * sits (null in the middle) and its own text (empty shows its value). */
    edit: (id: number, spec: DimensionSpec, textAt: Point | null, customText: string) =>
      k().call<Revision & { id: number }>('dims.update', { id, spec, textAt, customText }),
    /** Moves a dimension's line and text (a text drag); textAt null puts the text in the middle. */
    move: (id: number, spec: DimensionSpec, textAt: Point | null) =>
      k().call<Revision & { id: number }>('dims.update', { id, spec, textAt }),
    /** Reads (with no changes) or changes the settings new dimensions use. */
    style: (changes: Partial<DimensionStyleSettings> = {}) =>
      k().call<DimensionStyleSettings>('dims.style', changes),
  },
  drawTools: {
    ...startApi.drawTools,
    snip: (x0: number, y0: number, x1: number, y1: number) =>
      k().call<SnipReply>('ops.snip', { x0, y0, x1, y1 }),
    freehand: (args: FreehandArgs) =>
      k().call<{ id: number; revision: number }>('ops.freehand', args),
    measureAt: (x: number, y: number, tolerance: number) =>
      k().call<{ hit: MeasureHit | null }>('measure.at', { x, y, tolerance }),
  },
  plates: {
    /** The !!name!! variables in the selected template's text. */
    /** The !!name!! variables in the selected template's text, and how many toolpaths machine it. */
    variables: (ids: number[]) =>
      k().call<{ variables: string[]; toolpaths: number }>('plates.variables', { ids }),
    /** Columns, row count and the first rows of a data file. */
    data: (path: string, separator: PlateSeparator, header: boolean) =>
      k().call<PlatesData>('plates.data', { path, separator, header }),
    /** Makes the plates, one undo step. */
    produce: (args: PlatesProduceArgs) =>
      k().call<
        Revision & {
          ids: number[];
          plates: number;
          sheets: number;
          perSheet: number;
          toolpaths: number[];
        }
      >('plates.produce', args),
  },
  ops: {
    ...startApi.ops,
    duplicate: (ids: number[]) => k().call<Revision & { ids: number[] }>('ops.duplicate', { ids }),
    /** Extend Vectors: carries each open end straight on to the point. */
    extendTo: (
      ends: Array<{ id: number; subpath: number; atEnd: boolean }>,
      x: number,
      y: number,
    ) => k().call<Revision>('ops.extendTo', { ends, x, y }),
    /** What a Create Fillets click here would do; changes nothing. */
    filletProbe: (id: number, x: number, y: number, radius: number, kind: FilletKind) =>
      k().call<{ action: 'fillet' | 'remove' | 'none' }>('ops.filletProbe', {
        id,
        x,
        y,
        radius,
        kind,
      }),
    /** Whether an Interactive Vector Trim click here would trim; changes nothing. */
    trimProbe: (id: number, x: number, y: number) =>
      k().call<{ trims: boolean }>('ops.trimProbe', { id, x, y }),
    trim: (id: number, x: number, y: number) => k().call<Revision>('ops.trim', { id, x, y }),
    /** Fix zero length Spans: takes them out; one undo step. */
    fixZeroLength: (ids: number[]) =>
      k().call<Revision & { removed: number }>('ops.fixZeroLength', { ids }),
    /** Packs the parts onto sheets; copies after the first are new vectors. */
    /** Individual Part Properties Apply: the objects keep copies and a filler mark; one undo step. */
    nestMarks: (ids: number[], copies: number, filler: boolean) =>
      k().call<Revision & { ids: number[] }>('ops.nestMarks', { ids, copies, filler }),
    nest: (
      ids: number[],
      params: NestParams,
      quantities: Record<string, number>,
      fillers: number[] = [],
      options: {
        components?: number[];
        removeOriginals?: boolean;
        twoSided?: boolean;
        /** The sheet to start on; the active sheet when left out. */
        sheet?: number;
        /** False keeps every part on that sheet instead of adding sheets. */
        addSheets?: boolean;
        /** Nest boundary layer: its first closed vector bounds the nest. */
        boundaryLayer?: number;
        /** Custom Sheet Selection: the sheets to fill in order; over `sheet` and `addSheets`. */
        sheetPlan?: NestSheetUse[];
      } = {},
    ) =>
      k().call<Revision & NestReply>('ops.nest', { ids, params, quantities, fillers, ...options }),
    /** Bends vectors and components through an envelope, one undo step. */
    distort: (args: DistortArgs) =>
      k().call<Revision & { ids: number[]; componentIds: number[] }>('ops.distort', args),
    /** The live distortion of exactly these objects, or null. */
    distortion: (ids: number[]) =>
      k().call<{ distortion: StoredDistortion | null }>('ops.distortion', { ids }),
    /** Bakes the distortion of these objects (Bake Distortion). */
    bakeDistortion: (ids: number[]) =>
      k().call<Revision & { baked: number }>('ops.bakeDistortion', { ids }),
  },
};
