// Document level shapes exchanged with the kernel: geometry, entities, layers,
// selection, snapping. Mirrors the JSON shapes of the kernel API contract.

import type { Side, XyOrigin, ZZero } from './types';

export interface Point {
  x: number;
  y: number;
}

export type Segment =
  | { kind: 'line'; to: Point }
  | { kind: 'arc'; to: Point; bulge: number }
  | { kind: 'cubic'; c1: Point; c2: Point; to: Point };

export interface SubPath {
  start: Point;
  segments: Segment[];
  closed: boolean;
}

export interface Path {
  subpaths: SubPath[];
}

/** Affine matrix [a, b, c, d, e, f]: (x, y) maps to (a x + c y + e, b x + d y + f). */
export type Matrix = [number, number, number, number, number, number];

/** [minX, minY, maxX, maxY] in job millimetres. */
export type Bbox = [number, number, number, number];

export type EntityKind = 'path' | 'text' | 'bitmap' | 'dimension' | 'group';

export interface EntitySummary {
  id: number;
  kind: EntityKind;
  layer: number;
  bbox: Bbox;
  closed: boolean;
  revision: number;
  name?: string;
  children?: number[];
  /** The vector a Text On A Curve text follows. */
  onCurve?: number;
  /** Nest Parts marks kept with the object (Individual Part Properties). */
  nest?: NestMark;
}

/** An object's Nest Parts copies and filler mark. */
export interface NestMark {
  copies: number;
  filler: boolean;
}

/** Where a text's position sits on the box round its letters. */
export type TextAnchor =
  | 'topLeft'
  | 'topCentre'
  | 'topRight'
  | 'middleLeft'
  | 'centre'
  | 'middleRight'
  | 'bottomLeft'
  | 'bottomCentre'
  | 'bottomRight';

export interface TextSpec {
  text: string;
  fontFamily: string;
  bold: boolean;
  italic: boolean;
  sizeMm: number;
  alignment: 'left' | 'centre' | 'right';
  lineSpacing: number;
  charSpacing: number;
  kerning: boolean;
  /** Column width in millimetres for a text block; plain text when absent. */
  wrapWidth?: number;
  /** Auto Layout Text: the box the text is sized to fill, and how. */
  autoLayout?: AutoLayout;
  /** Text Selection: per line, extra space after each drawn letter (fraction of the height). */
  letterGaps?: number[][];
  /** Text Selection: extra space under each line (fraction of the height). */
  lineGaps?: number[];
  /** Text Selection: bend height in mm, positive lifts the middle of the line. */
  arch?: number;
  /** The point of the text's box at its position; absent, the first baseline's start. */
  anchor?: TextAnchor;
  /** Text On A Curve: the vector the text follows and how. */
  onCurve?: OnCurve;
}

/** Where text sits against its curve. */
export type CurvePosition = 'above' | 'on' | 'below';

/** Text On A Curve settings kept on the text (kernel OnCurve). */
export interface OnCurve {
  pathId: number;
  position: CurvePosition;
  /** Text on other side: reads the other way along the curve. */
  otherSide: boolean;
  /** Offset Distance in millimetres. */
  offset: number;
  align: 'left' | 'centre' | 'right';
  keepVertical: boolean;
  /** Scale text to fill curve. */
  fill: boolean;
  /** Text Spacing, percent. */
  spacing: number;
  /** Millimetres the text was slid along the curve. */
  startAt: number;
}

/** A drawn letter of a text, as text.glyphs reports it. */
/** The circle arched text follows (text.glyphs), in job space. Angles in radians. */
export interface TextArc {
  centre: Point;
  radius: number;
  /** Angle of the left end of the baseline. */
  start: number;
  /** Angle of the right end of the baseline. */
  end: number;
  /** Length of the line along the arc. */
  width: number;
  /** Letters stand out from the circle (arched up). */
  outward: boolean;
}

export interface GlyphBox {
  line: number;
  index: number;
  bbox: Bbox;
}

export type LayoutMargin = 'none' | 'normal' | 'wide';
export type VerticalStretch = 'none' | 'lineSpacing' | 'characterHeight';
export type HorizontalStretch =
  'none' | 'wordSpacing' | 'letterSpacing' | 'characterWidth' | 'characterSize';

export interface AutoLayout {
  bounds: Bbox;
  margin: LayoutMargin;
  vertical: VerticalStretch;
  horizontal: HorizontalStretch;
}

interface EntityBase {
  id: number;
  layer: number;
  sheet: number;
  revision: number;
  name?: string;
}

export type EntityJson =
  | (EntityBase & { kind: 'path'; path: Path })
  | (EntityBase & { kind: 'text'; spec: TextSpec; origin: Point; paths: Path[] })
  | (EntityBase & { kind: 'bitmap'; imageId: number; quad: Point[]; opacity: number })
  | (EntityBase & {
      kind: 'dimension';
      spec: DimensionSpec;
      customText?: string;
      /** Where the text was dragged along the dimension line; the middle when absent. */
      textAt?: Point;
      /** Box round the value text, for picking it. */
      textBox?: Bbox;
    })
  | (EntityBase & { kind: 'group'; children: number[] });

/** What the client sends to create an entity: the JSON without id and revision. */
export type NewEntity = Omit<Extract<EntityJson, { kind: 'path' }>, 'id' | 'revision'>;

/** How layers.add places a new layer. */
export interface NewLayerOptions {
  /** False adds it hidden. */
  visible?: boolean;
  /** Makes it the active layer. */
  active?: boolean;
  /** Puts it just before this layer in the list, else last. */
  before?: number;
}

export interface Layer {
  id: number;
  name: string;
  colour: string;
  visible: boolean;
  locked: boolean;
  /** Closed vectors on the layer mark keep-out zones (clamps). */
  keepOut?: boolean;
  active: boolean;
  count: number;
}

/** One sheet of material, as `sheets.list` answers it. */
export interface Sheet {
  id: number;
  name: string;
  active: boolean;
  /** Top level objects on the sheet. */
  count?: number;
  /** Relief components on the sheet. */
  components?: number;
  /** Size, thickness and datum of the sheet, in millimetres. */
  material?: SheetMaterial;
}

/** The part of the job setup each sheet has its own of. Lengths in mm. */
export interface SheetMaterial {
  width: number;
  height: number;
  thickness: number;
  zZero: ZZero;
  xyOrigin: XyOrigin;
  offsetX: number;
  offsetY: number;
}

export interface DisplayRun {
  offset: number;
  count: number;
}

export interface DisplayEntity {
  id: number;
  revision: number;
  layer: number;
  kind: EntityKind;
  closed: boolean;
  selected: boolean;
  runs: DisplayRun[];
  /** The image a bitmap entity places. */
  imageId?: number;
  /** Opacity of a bitmap entity, 0 to 1. */
  opacity?: number;
}

export interface DisplayHeader {
  sections: Array<{
    name: string;
    dtype: 'f32' | 'u32' | 'u16' | 'u8';
    offset: number;
    count: number;
  }>;
  revision: number;
  entities: DisplayEntity[];
  /** Ids that no longer exist when `sinceRevision` was given. */
  removed?: number[];
}

export interface DisplayReply {
  revision: number;
  entities: DisplayEntity[];
  removed: number[];
  /** Flat x, y pairs shared by every run of every entity in this reply. */
  xy: Float32Array;
}

export interface SnapOptions {
  grid: boolean;
  guides: boolean;
  nodes: boolean;
  midpoints: boolean;
  centres: boolean;
  intersections: boolean;
  jobPoints: boolean;
  smart: boolean;
}

export type SnapKind =
  | 'node'
  | 'mid'
  | 'centre'
  | 'quadrant'
  | 'on'
  | 'intersection'
  | 'guide'
  | 'grid'
  | 'jobCorner'
  | 'jobCentre';

export interface ProbeHit {
  kind: SnapKind;
  x: number;
  y: number;
  dist: number;
  id?: number;
}

export interface Guide {
  id: number;
  axis: 'x' | 'y';
  position: number;
  locked: boolean;
}

/** A guide line as the job file keeps it. */
export type GuideLine = Omit<Guide, 'id'>;

export interface ProbeArgs {
  x: number;
  y: number;
  radius: number;
  snap: SnapOptions;
  gridSpacing: number;
  guides: Guide[];
  exclude?: number[];
}

export interface ProbeReply {
  hits: ProbeHit[];
  nearestEntity?: number;
}

export type HitTestArgs =
  { x: number; y: number; tolerance: number } | { rect: Bbox; mode: 'inside' | 'touch' };

/**
 * A rotated selection's bounds: `extent` is the box in the frame turned by
 * `angle` degrees about the job origin.
 */
export interface SelectionFrame {
  angle: number;
  extent: Bbox;
}

export interface SelectionState {
  ids: number[];
  bbox: Bbox | null;
  /** Present when the whole selection shares one non zero rotation. */
  frame?: SelectionFrame;
}

export interface UndoInfo {
  canUndo: boolean;
  canRedo: boolean;
  undoLabel: string | null;
  redoLabel: string | null;
}

export interface DocInfo {
  revision: number;
  path: string | null;
  dirty: boolean;
  activeLayer: number;
  activeSheet: number;
  /** The side being shown and edited; top on a single sided job. */
  activeSide?: Side;
  counts: { entities: number; components: number; toolpaths: number };
}

export interface Revision {
  revision: number;
}

// Text.

export interface PlainTextArgs {
  spec: TextSpec;
  /** Where the first line's baseline starts, job millimetres. */
  x: number;
  y: number;
  layer?: number;
}

export interface ArcTextArgs {
  spec: TextSpec;
  cx: number;
  cy: number;
  radius: number;
  /** Degrees, counter clockwise from +X, where the text is centred. */
  startAngleDeg: number;
  /** Degrees of arc the text may spread over. */
  sweepDeg: number;
  inside: boolean;
  layer?: number;
}

/** text.onCurve: sets an existing text along a vector, keeping the link. */
export interface OnCurveTextArgs {
  textId: number;
  curve: OnCurve;
}

export type TextPreviewArgs =
  | ({ kind: 'plain' } & PlainTextArgs)
  | ({ kind: 'arc' } & ArcTextArgs)
  | ({ kind: 'onCurve' } & OnCurveTextArgs);

// Vector operations (Edit Objects, Offset and Layout).

/** smoothShallow: the symmetrical, shallower smooth method. */
export type CloseMode = 'straight' | 'smooth' | 'smoothShallow' | 'moveEnd';
export type BooleanKind = 'weld' | 'subtract' | 'intersect' | 'xor' | 'crop';
/** Which side of the boundary Trim Objects clears. */
export type TrimSide = 'inside' | 'outside';
export type FilletKind = 'normal' | 'dogbone' | 'tbone' | 'plasma';
export type FitKind = 'arcs' | 'beziers' | 'lines';
export type BoundaryKind = 'selection' | 'each';

export interface OffsetArgs {
  ids: number[];
  /** Millimetres, positive. */
  distance: number;
  side: 'outward' | 'inward';
  corner: 'round' | 'sharp';
  deleteOriginal: boolean;
}

export interface ValidationIssue {
  id: number;
  kind:
    | 'open'
    | 'selfIntersecting'
    | 'intersection'
    | 'zeroLengthSegment'
    | 'duplicateNode'
    | 'overlappingDuplicate';
  x?: number;
  y?: number;
  message: string;
  /** Every place the view marks for it (both ends of an open vector, each
   * crossing, ...). */
  marks?: Point[];
}

/** Nesting settings, as `ops.nest` takes them. Lengths in millimetres. */
export interface NestParams {
  toolDiameter: number;
  gap: number;
  border: number;
  /** 0 keeps parts as drawn. */
  rotationStepDeg: number;
  allowMirror: boolean;
  /** 0 by 0 nests onto sheets the size of the job. */
  sheetWidth: number;
  sheetHeight: number;
  startCorner: 'bottomLeft' | 'bottomRight' | 'topLeft' | 'topRight';
  direction: 'x' | 'y';
  maxSheets: number;
  /** Let parts go inside the holes of parts already placed. */
  nestInHoles: boolean;
}

/** A row of the Custom Sheet Selection: a sheet and how many of it (left out for No Limit). */
export interface NestSheetUse {
  sheet: number;
  quantity?: number;
}

export interface NestUnplaced {
  partId: number;
  copy: number;
  reason: 'noClosedOutline' | 'tooLarge' | 'outOfSheets';
}

export interface NestReply {
  ids: number[];
  sheetCount: number;
  unplaced: NestUnplaced[];
}

export interface ArrayCopyArgs {
  rows: number;
  cols: number;
  /** Millimetres: the space between copies, or their pitch in offset mode. */
  gapX: number;
  gapY: number;
  mode: 'gap' | 'offset';
  stagger: 'none' | 'rows' | 'columns';
  group: boolean;
  /** Block symmetry, the four places of a two by two block (first row
   * first); the first is the original. */
  symmetry?: BlockTurn[];
  /** Row and column displacement: every other row or column moves this
   * far, [x, y] mm. */
  rowShift?: [number, number];
  colShift?: [number, number];
}

/** How a copy in a symmetry block is turned about its middle. */
export type BlockTurn = 'none' | 'mirrorX' | 'mirrorY' | 'rotate';

export interface CircularArrayArgs {
  cx: number;
  cy: number;
  /** Copies including the original. */
  count: number;
  /** Total angle the copies span, degrees. */
  angle: number;
  rotateCopies: boolean;
  radius: number | null;
  group: boolean;
  /** Angle between neighbours, degrees, counter clockwise; replaces `angle` when set. */
  step?: number | null;
}

export interface CopyAlongArgs {
  /** The vector the copies follow (when pathIds is absent). */
  pathId?: number;
  /** The vectors the copies follow. */
  pathIds?: number[];
  distributeMode: 'count' | 'spacing';
  count: number;
  spacing: number;
  /** Force even spacing: copies land on both ends of each vector. */
  forceEven?: boolean;
  /** Copy Circles of this diameter (mm) instead of the selection. */
  circleDiameter?: number;
  /** Create Copies on new layer. */
  newLayer?: boolean;
  alignToCurve: boolean;
  startOffset: number;
  reverse: boolean;
  deleteOriginal: boolean;
}

/** A dimension as `dims.create` takes it. Horizontal and vertical offsets
 * are the job Y or X of the dimension line; a linear offset is to the left
 * of the direction from `from` to `to`. */
export type DimensionSpec =
  | { kind: 'linear' | 'horizontal' | 'vertical'; from: Point; to: Point; offset: number }
  | { kind: 'angular'; centre: Point; from: Point; to: Point; radius: number }
  | { kind: 'note'; at: Point; textAt: Point; text: string }
  | { kind: 'radius'; centre: Point; radius: number; at: Point; diameter: boolean };

/** Dimension settings shared by every new dimension (dims.style). */
export interface DimensionStyleSettings {
  fontFamily: string;
  textHeight: number;
  precision: number;
  anglePrecision: number;
  units: 'millimetres' | 'inches';
  showUnits: boolean;
  offset: number;
  arrowSize: number;
  /** Place Dimensions on Layer, and that layer's name. */
  useLayer: boolean;
  layerName: string;
}
