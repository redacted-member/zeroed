// Shapes of the 3D model import commands (model.import.*): the Orientate
// Model and Position Model settings, the segmenting plan, and the preview
// the 3D view draws while a step is open.

import type { Combine } from './model';
import type { BlobHeader } from './types';

export type Vec3 = [number, number, number];

/** The face of the model, in the file's own axes, that becomes the top. */
export type TopFace = 'top' | 'bottom' | 'front' | 'back' | 'left' | 'right';

export const TOP_FACES: readonly TopFace[] = ['top', 'bottom', 'front', 'back', 'left', 'right'];

/** Orientate Model settings. Angles in degrees, lengths in millimetres. */
export interface MeshOrientation {
  top: TopFace;
  rotX: number;
  rotY: number;
  rotZ: number;
  /** Size of the turned model; null keeps the file numbers times unitScale. */
  size: Vec3 | null;
  /** Millimetres per file unit when size is null. */
  unitScale: number;
  /** 0 for none to 1 for strongest. */
  perspective: number;
  /** Where the model's centre goes; null for the job centre. */
  centre: { x: number; y: number } | null;
}

export const DEFAULT_ORIENTATION: MeshOrientation = {
  top: 'top',
  rotX: 0,
  rotY: 0,
  rotZ: 0,
  size: null,
  unitScale: 1,
  perspective: 0,
  centre: null,
};

/** Position Model settings. */
export interface MeshPlacement {
  /** Height of the zero plane above the model's lowest point, mm. */
  zeroPlane: number;
  bothSides: boolean;
  discardBelow: boolean;
}

export const DEFAULT_PLACEMENT: MeshPlacement = {
  zeroPlane: 0,
  bothSides: false,
  discardBelow: true,
};

/** A segmenting plane: a point on it and two tilts of the level plane. */
export interface CutPlane {
  point: Vec3;
  angleX: number;
  angleY: number;
}

/** A node of the segment tree: a leaf, or a cut with [above, below]. */
export interface SegmentNode {
  cut?: CutPlane | null;
  children?: SegmentNode[];
}

/** How one segment is turned in the layout. */
export interface SegmentLayout {
  quarterX: number;
  quarterY: number;
  quarterZ: number;
  rotX: number;
  rotY: number;
  rotZ: number;
}

export const DEFAULT_SEGMENT_LAYOUT: SegmentLayout = {
  quarterX: 0,
  quarterY: 0,
  quarterZ: 0,
  rotX: 0,
  rotY: 0,
  rotZ: 0,
};

export interface SegmentPlan {
  root: SegmentNode;
  /** Groups of leaf ids merged into one segment. */
  merges: string[][];
  /** Turns per segment id (the first leaf id of a merged segment). */
  layouts: Record<string, SegmentLayout>;
  caps: boolean;
  resizeMaterial: boolean;
  boundaryVectors: boolean;
  gap: number;
}

export const DEFAULT_PLAN: SegmentPlan = {
  root: {},
  merges: [],
  layouts: {},
  caps: true,
  resizeMaterial: false,
  boundaryVectors: false,
  gap: 5,
};

/** A full 3D model on a rotary job: where the axis runs through it
 * (across, from the middle of its cross section, mm) and its scale along
 * and across the axis. */
export interface RotaryFit {
  offset: [number, number];
  alongScale: number;
  radialScale: number;
}

export const DEFAULT_ROTARY_FIT: RotaryFit = { offset: [0, 0], alongScale: 1, radialScale: 1 };

/** The cylinder round a model on the axis, mm. */
export interface Cylinder {
  length: number;
  diameter: number;
}

export interface MeshInfo {
  triangles: number;
  /** Rotary: the cylinder round the model as fitted, and unscaled. */
  cylinder?: Cylinder;
  naturalCylinder?: Cylinder;
  /** Extent of the turned model in file units. */
  natural: Vec3;
  /** Extent after scaling, mm. */
  size: Vec3;
  centre: { x: number; y: number };
}

export interface PreviewSegment {
  id: string;
  leaves: string[];
  size: Vec3;
}

export interface MeshPreviewHeader extends BlobHeader {
  size: Vec3;
  centre: { x: number; y: number };
  /** Rotary: the triangles are along, across and up, round the axis. */
  rotary?: boolean;
  cylinder?: Cylinder;
  count: number;
  segments?: PreviewSegment[];
  extent?: [number, number, number, number];
}

/** Triangles to draw, in job millimetres standing on Z zero. */
export interface MeshPreview {
  size: Vec3;
  centre: { x: number; y: number };
  xyz: Float32Array;
  /** Bit 0: undercut. */
  flags: Uint8Array;
  /** Index into segments per triangle. */
  segment: Uint16Array;
  segments: PreviewSegment[];
  /** Rotary: the triangles are along, across and up round the axis, and
   * the cylinder round them. */
  cylinder?: Cylinder;
}

/** Settings a component was imported with, for Reimport. */
export interface ImportSource {
  path: string;
  orient: MeshOrientation;
  place?: MeshPlacement | null;
  plan?: SegmentPlan | null;
  session: number;
  combine: Combine;
  rotary?: RotaryFit | null;
}
