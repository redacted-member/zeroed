// JSON shapes of the Joint Creator commands (joints.*): joints between flat
// parts made on their vectors. Lengths are millimetres.

import type { Path } from './document';

/** A point as the joint commands send it: [x, y]. */
export type XY = [number, number];

export type JointKind =
  | 'finger'
  | 'dado'
  | 'rabbet'
  | 'tabSlot'
  | 'mortiseTenon'
  | 'crossLap'
  | 'halfLap'
  | 'wedgedTenon'
  | 'kerfTenon'
  | 'dovetail'
  | 'dowel'
  | 'captiveNut'
  | 'butterfly'
  | 'puzzle'
  | 'livingHinge'
  | 'vJoint'
  | 'twistLock'
  | 'cornerDovetail'
  | 'camLock'
  | 'groove'
  | 'screws';

/** The slots of a living hinge. */
export type HingePattern = 'straight' | 'sinuous' | 'spiral';

export type Fit = 'slip' | 'press' | 'glue' | 'knockDown' | 'custom';

export type Relief = 'none' | 'dogBone' | 'tBone';

/** A straight edge of a part, with the part on its left. */
export interface EdgeLine {
  vector: number;
  from: XY;
  to: XY;
}

/** Part B: an edge, or a line on its face where part A stands. */
export type Target =
  ({ kind: 'edge' } & EdgeLine) | { kind: 'face'; edge: EdgeLine; offset: number; along: number };

export interface JointParams {
  count?: number;
  width?: number;
  depth?: number;
  start?: 'finger' | 'gap';
  bias?: number;
  blind?: boolean;
  stop?: number;
  proud?: number;
  ends?: 'square' | 'rounded';
  detent?: number;
  angle?: number;
  /** A living hinge's inside bend radius. */
  radius?: number;
  pattern?: HingePattern;
  diameter?: number;
  inset?: number;
  length?: number;
  nutWidth?: number;
  nutThickness?: number;
  atA?: number;
  atB?: number;
  /** The depth of the second part's slot of a cross lap (half of it when left out). */
  depthB?: number;
}

export interface JointSettings {
  fit: Fit;
  customClearance: number;
  relief: Relief;
  toolDiameter: number;
  /** Measured thickness per sheet id. */
  thickness: Record<string, number>;
}

export type JointRole =
  | { kind: 'hole' }
  | { kind: 'pocket'; depth: number }
  | { kind: 'part'; thickness: number }
  | { kind: 'mark' }
  | { kind: 'taper'; depth: number; angle: number };

export interface JointSpec {
  kind: JointKind;
  a: EdgeLine;
  b: Target;
  params: JointParams;
}

export interface JointRow extends JointSpec {
  id: number;
  vectors: Array<{ id: number; role: JointRole }>;
  resolved: JointParams;
  parts: [number, number];
  ok: boolean;
}

export interface SheetThickness {
  id: number;
  name: string;
  thickness: number;
  nominal: number;
}

export interface JointList {
  settings: JointSettings;
  clearance: number;
  joints: JointRow[];
  sheets: SheetThickness[];
}

/** An edge the pointer is on. */
export interface PickedEdge extends EdgeLine {
  length: number;
  sheet: number;
  name: string | null;
  thickness: number;
  /** The joint already on this edge, if any. */
  joint: number | null;
  /** Where along the edge the click was, from its start. */
  at?: number;
}

export interface PickEdgeReply {
  edge: PickedEdge | null;
  reason?: string;
}

export interface PickFaceReply {
  face: { edge: PickedEdge; offset: number; along: number } | null;
  reason?: string;
}

export interface JointOutcome {
  id: number;
  resolved: JointParams;
  notes: string[];
  broken?: string;
  bToA?: number[];
}

/** One part as a solid: its outline rings and thickness. */
export interface PartSolid {
  rings: XY[][];
  thickness: number;
}

export interface JointPreview {
  outcome: JointOutcome | null;
  parts: Array<{ vector: number; path: Path }>;
  made: Array<{ vector: number; role: JointRole; path: Path }>;
  madeIds: number[];
  solid: { a: PartSolid; b: PartSolid; bToA: number[] | null };
}
