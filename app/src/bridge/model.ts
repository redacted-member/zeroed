// Shapes of the relief model commands. Heights are millimetres above the
// model's zero plane.

import type { ComponentGroup } from './componentTree';
import type { Heights } from './heights';
import type { BlobHeader, HeightCodeHeader } from './types';

export type Combine = 'add' | 'subtract' | 'mergeHigh' | 'mergeLow' | 'multiply';
export type ShapeProfile = 'round' | 'angular' | 'flat' | 'smooth' | 'custom';

export interface ShapeParams {
  profile: ShapeProfile;
  /** Degrees from flat at the edge (round and angular). */
  angle: number;
  /** Height of a flat shape, or the cap when `limit` is on. */
  height: number;
  limit: boolean;
  /** Base baked into the heights; the shape tools leave it at 0 and set
   * the component's own base height instead. */
  baseHeight: number;
  /** Smooth profile only: roundness of the top, 0 to 1. */
  smoothTop: number;
  /** Smooth profile only: roundness of the foot, 0 to 1. */
  smoothBottom: number;
  /** Keep the valleys at inside corners of the outline crisp. */
  sharpCorners: boolean;
  /** Smooth and custom: nested vectors are the top, not holes. */
  blendInner?: boolean;
  /** Scale the rise so its top is `scaleHeight`. */
  scale?: boolean;
  scaleHeight?: number;
}

/** A point in job space, mm. */
export interface JobPoint {
  x: number;
  y: number;
}

/** Dynamic fade: full height at `from`, `amount` (0 to 1) less at `to`. */
export interface FadeProp {
  from: JobPoint;
  to: JobPoint;
  amount: number;
}

/** Dynamic tilt: a plane through `pivot` rising towards `toward`. */
export interface TiltProp {
  pivot: JobPoint;
  toward: JobPoint;
  angleDeg: number;
}

/** What a shape component was made from. */
export interface ShapeSource {
  params: ShapeParams;
  vectors: number[];
  profileVector?: number;
}

/** How a creation form's Apply lands: the component its last Apply made
 * (or the one it edits) and the revision that Apply answered. */
export interface ApplyTarget {
  target?: number;
  replace?: number;
}

/** What a creation command answers. */
export interface AppliedComponent {
  revision: number;
  id: number;
  /** Highest point of the new heights, mm. */
  shapeHeight: number | null;
  /** Highest point with the base height, mm. */
  totalHeight: number | null;
}

/** `model.createShape` arguments. */
export interface ShapeArgs extends ApplyTarget {
  /** Closed vectors, plus the open profile vector of a custom shape. */
  ids: number[];
  params: ShapeParams;
  combine: Combine;
  name?: string;
  baseHeight?: number;
  fade?: FadeProp;
  tilt?: TiltProp;
}

export type MeshUp = 'z' | 'y' | 'x';

export interface MeshParams {
  up: MeshUp;
  /** Width along X the model is scaled to, mm; null keeps the file's size. */
  width: number | null;
}

export interface ComponentSummary {
  id: number;
  name: string;
  visible: boolean;
  combine: Combine;
  heightScale: number;
  /** Id of the level it sits on. */
  level: number;
  maxHeight: number | null;
  /** Extent of the component's grid in job space, mm: minX, minY, maxX, maxY. */
  bbox?: [number, number, number, number];
  /** Flat riser under the whole component, mm. */
  baseHeight?: number;
  /** Preview colour `#rrggbb`, null for its parent's appearance. */
  color?: string | null;
  /** Preview material the colour was taken from. */
  material?: string | null;
  /** Group id inside its level, 0 for none. */
  group?: number;
  /** Imported from a mesh file, so it can be imported again. */
  imported?: boolean;
  /** Made by a shape tool, which can edit it again. */
  shape?: ShapeSource | null;
  /** Made by Create Texture Area, which can edit it again. */
  texture?: TextureSource | null;
  fade?: FadeProp | null;
  tilt?: TiltProp | null;
}

/** A level of the model: its components bake together, then it meets the
 * levels below with its own combine mode. */
export interface Level {
  id: number;
  name: string;
  visible: boolean;
  combine: Combine;
  /** Groups of the level's components. */
  groups?: ComponentGroup[];
  /** Closed vectors the level is clipped to. */
  clip?: number[];
  /** `#rrggbb` for every component in it; absent takes theirs. */
  color?: string | null;
  /** Preview material `color` was taken from. */
  material?: string | null;
  /** Live mirroring of the level's model about the middle of the job. */
  mirror?: MirrorMode | null;
  /** Millimetres under everything the level covers; absent for 0. */
  baseHeight?: number;
}

/** Which half or quarter of the job a mirrored level keeps. */
export type MirrorMode =
  | 'leftToRight'
  | 'rightToLeft'
  | 'topToBottom'
  | 'bottomToTop'
  | 'topLeft'
  | 'topRight'
  | 'bottomLeft'
  | 'bottomRight';

export const MIRROR_MODES: readonly MirrorMode[] = [
  'leftToRight',
  'rightToLeft',
  'topToBottom',
  'bottomToTop',
  'topLeft',
  'topRight',
  'bottomLeft',
  'bottomRight',
];

/** What `model.levels.update` changes; absent fields are left alone. */
export type LevelPatch = Partial<
  Pick<Level, 'name' | 'visible' | 'combine' | 'color' | 'material' | 'mirror' | 'baseHeight'>
>;

/** What `model.list` answers. Levels are bottom first. */
export interface ModelList {
  stamp: number;
  components: ComponentSummary[];
  levels: Level[];
  activeLevel: number;
}

/** Two-Rail Sweep settings. Lengths are millimetres. */
/** Where a cross section sits: fractions of each rail's length. */
export interface SweepStation {
  /** Index into the sweep's sections. */
  section: number;
  a: number;
  b: number;
  smooth: boolean;
}

export interface SweepParams {
  scaleHeightWithWidth: boolean;
  baseHeight: number;
  reverse?: [boolean, boolean];
  /** Start of each closed rail, job mm. */
  starts?: [JobPoint | null, JobPoint | null];
  stations?: SweepStation[];
  sweepBetweenSpans?: boolean;
  fillInner?: boolean;
  exactHeight?: boolean;
  height?: number;
}

export interface TurnParams {
  baseHeight: number;
  /** Scale so the top is `height` above the base. */
  exactHeight?: boolean;
  height?: number;
}

/** Spin settings; angles are degrees counter clockwise from +X. */
export interface SpinParams {
  baseHeight: number;
  startAngle?: number;
  endAngle?: number;
  exactHeight?: boolean;
  height?: number;
}

/** Where a cross section sits on an Extrude and Weave drive rail. */
export interface ExtrudeStation {
  /** Index into the rails. */
  rail: number;
  /** Index into the sections. */
  section: number;
  /** Fraction of the rail's length. */
  at: number;
  smooth: boolean;
}

/** How a component fills a rail: one copy stretched, or copies at size. */
export type ComponentFill = 'stretch' | 'copies';

/** How a weave changes the strands at the crossings. */
export type WeaveMode = 'scaleShape' | 'addBase';

/** Extrude and Weave settings. Lengths are millimetres, the overlap and
 * the Z Under and Z Over values percentages. */
export interface ExtrudeParams {
  reverse: boolean[];
  /** Start of each closed rail, job mm. */
  starts: (JobPoint | null)[];
  stations: ExtrudeStation[];
  sharpCorners: boolean;
  sweepBetweenSpans: boolean;
  fill: ComponentFill;
  overlap: number;
  flipAlternate: boolean;
  weave: boolean;
  weaveMode: WeaveMode;
  zUnder: number;
  zOver: number;
  exactHeight: boolean;
  height: number;
}

/** How one tile of a group of four is reflected. */
export type Reflect = 'none' | 'horizontal' | 'vertical' | 'both';

/** The tile's box in job space, mm: lower left corner and size. */
export interface TileBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Create Texture Area settings: spacing and shifts in percent of the
 * tile's size; reflections for the lower left, lower right, upper left and
 * upper right tile of each group of four. */
export interface TextureParams {
  spacingX: number;
  spacingY: number;
  shiftX: number;
  shiftY: number;
  reflect: [Reflect, Reflect, Reflect, Reflect];
  /** The tile's box; the seed's own box when absent. */
  tile?: TileBox | null;
  /** Turn of the tile lattice about the tile box's lower left corner,
   * degrees counter clockwise; a turned texture area keeps it. */
  angle?: number;
}

/** What a texture area component was made from. */
export interface TextureSource {
  seed: number;
  params: TextureParams;
}

/** One component edit as the kernel takes it. Lengths are millimetres,
 * directions degrees counter clockwise from +X. */
export type ComponentEdit =
  | { op: 'smooth'; params: { strength: number; iterations: number } }
  | { op: 'draft'; params: { angleDeg: number } }
  | {
      op: 'tilt';
      params: { angleDeg: number; directionDeg: number; pivot: { x: number; y: number } };
    }
  | { op: 'fade'; params: { directionDeg: number; startFraction: number; endFraction: number } }
  | { op: 'mirror'; params: { horizontal: boolean } }
  | { op: 'flip' }
  | { op: 'scale'; params: { scale: number; baseOffset: number } }
  | { op: 'emboss'; params: { maxHeight: number } };

export type ComponentEditOp = ComponentEdit['op'];

/** Move, scale and turn of a component in XY; heights are unchanged.
 * A point goes to centre + rotate(scale(p - centre)) + offset. Lengths are
 * millimetres, the turn degrees counter clockwise. */
export interface ComponentTransformParams {
  offset: { x: number; y: number };
  scaleX: number;
  scaleY: number;
  rotateDeg: number;
  /** Omitted for the centre of the component's box. */
  centre?: { x: number; y: number };
}

export interface BoundaryArgs {
  /** Components to trace; omitted for the whole composite. */
  ids?: number[];
  /** Distance out from the edge, mm. */
  offset: number;
  /** Height the edge is traced at, mm; omitted for the silhouette. */
  level?: number;
  /** Put several loops into one group. */
  group?: boolean;
}

/** Heights may come as u16 steps (see bridge/heights.ts). */
export interface CompositeHeader extends BlobHeader, HeightCodeHeader {
  revision: number;
  width: number;
  height: number;
  cellSize: number;
  originX: number;
  originY: number;
  min: number | null;
  max: number | null;
  /** Highest point of the other side's model on a double sided job, in
   * millimetres above the shared modelling plane (0 without one). */
  otherPeak?: number;
}

export interface Composite extends CompositeHeader {
  /** Model height per cell (read through heightReader in bridge/heights.ts). */
  z: Heights;
}

export type Brush = 'smooth' | 'deposit' | 'remove' | 'smudge' | 'erase' | 'restore';

/** A sculpting brush, as `model.sculpt` takes it. Lengths in millimetres. */
/** A component used as the sculpting brush (Deposit and Remove only). */
export interface CustomBrushParams {
  /** The component whose shape is the brush. */
  brush?: number;
  /** Or a brush file: a clip or a picture. */
  file?: string;
  /** Turn of the brush, degrees counter clockwise. */
  angle: number;
  /** Turn the brush with the stroke. */
  dynamic: boolean;
  /** Merge each stroke with the earlier ones instead of adding onto them. */
  merge: boolean;
}

export interface BrushParams {
  brush: Brush;
  diameter: number;
  /** 0 to 1. */
  strength: number;
  /** 0 for a hard edge, 1 for fully soft. */
  falloff: number;
  /** Preserve Transparency off: the brush works on empty cells too. */
  grow: boolean;
  /** Normal, or keep only the higher (raise) or lower (lower) heights. */
  mode?: SculptMode;
}

export type SculptMode = 'normal' | 'raise' | 'lower';
