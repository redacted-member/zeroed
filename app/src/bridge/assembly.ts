// JSON shapes of the Assembly View commands (assembly.pieces and friends):
// one piece per part a sheet's toolpaths cut free (tabs count as cut), or
// the whole sheet when nothing is cut free, or the sheet's raw model (part
// 0), plus components added as pieces of their own, each placed in
// assembly space.
// Lengths are millimetres, angles degrees, assembly space is Z up.

import type { Heightfield } from './cam';

/** Three numbers: X, Y and Z. */
export type Triple = [number, number, number];

/** The part number of a sheet's raw model piece. */
export const MODEL_PART = 0;

/**
 * Names a piece: its sheet and its part on the sheet (from 1; 0 for the raw
 * model), or the component it is (with its sheet and part 1).
 */
export interface PieceRef {
  sheet: number;
  part: number;
  component?: number | null;
}

/** A piece's key in maps and lists: `sheet:part`, or `c<id>` for a component piece. */
export function pieceKey(ref: PieceRef): string {
  return ref.component === undefined || ref.component === null
    ? `${ref.sheet}:${ref.part}`
    : `c${ref.component}`;
}

/** Just the fields that name a piece, as the kernel commands take them. */
export function pieceRef(ref: PieceRef): PieceRef {
  return { sheet: ref.sheet, part: ref.part, component: ref.component ?? null };
}

/** A component that can be added as a piece of its own. */
export interface AddableComponent {
  id: number;
  name: string;
  sheet: number;
  sheetName: string;
  /** Changes when the component's shape does, so its thumbnail is stale. */
  look?: string;
}

/** A piece's place and turns, assembly space. */
export interface PiecePose {
  position: Triple;
  rotation: Triple;
}

/** Where a component is let go: the point under the pointer and the piece it is on (null: the ground). */
export interface DropLanding {
  point: Triple;
  onto: PieceRef | null;
}

/** Where assembly.addComponentPieces stands the components it adds. */
export interface ComponentPlace {
  /** The middle of the first one's underside, assembly space. */
  at?: Triple;
  /** Each one (after the one put `at` a point) on the ground beside the pieces. */
  beside?: boolean;
  /** The first one's place and turns, as assembly.dropPlace answers them. */
  pose?: PiecePose;
}

/** A component's relief standing alone, cropped to its box, and that box's size. */
export interface ComponentRelief extends Heightfield {
  component: number;
  size: Triple;
}

/** What a piece is called: its sheet, its part number and a vector that fits it. */
export interface PieceNaming extends PieceRef {
  /** How many parts its sheet has. */
  parts: number;
  sheetName: string;
  /** The name of a named vector whose box fits the part, if any. */
  vectorName: string | null;
  /** Whether the piece is its sheet's raw model rather than its simulation. */
  model: boolean;
  /** The component's name for a component piece. */
  componentName?: string | null;
}

/** A piece and where it sits. */
export interface PieceRow extends PieceNaming {
  /** Where the middle of the part's box sits. */
  position: Triple;
  /** Turns about X, then Y, then Z (fixed axes), degrees. */
  rotation: Triple;
  /** The part's box: width, height, thickness. */
  size: Triple;
  /** Whether its sheet has a raw model that can be the piece. */
  hasModel: boolean;
  /** Whether its sheet has toolpaths to simulate. */
  hasToolpaths: boolean;
  /** Hidden in the view (Verify Fit still measures it). */
  hidden?: boolean;
  /** Grounded: it cannot be moved and stays put when mates solve. */
  grounded?: boolean;
}

/**
 * What a mate holds between two faces (or two round axes); a joint mate
 * holds two pieces as a Joint Creator joint puts them.
 */
export type MateKind = 'against' | 'flush' | 'distance' | 'angle' | 'concentric' | 'joint';

/** The mate kinds in the order the form offers them (joint mates come from the Joint Creator). */
export const MATE_KINDS: readonly MateKind[] = [
  'against',
  'flush',
  'distance',
  'angle',
  'concentric',
];

/** How a mate stands after the last solve. */
export type MateStanding = 'solved' | 'conflict' | 'suppressed' | 'missing';

/** A mate kept with the document, with how it stands. */
export interface MateRow {
  id: number;
  kind: MateKind;
  /** The piece whose face stayed when the mate was made. */
  a: PieceRef;
  b: PieceRef;
  /** The gap in mm, or the angle in degrees for an angle mate. */
  value: number;
  /** For a distance: the faces look the same way. */
  aligned: boolean;
  suppressed: boolean;
  state: MateStanding;
  /** How far off the mate is, mm and degrees. */
  distanceError: number;
  angleError: number;
  /** The radii of round faces (concentric), mm. */
  radius?: [number | null, number | null];
  /** The Joint Creator joint a joint mate keeps. */
  joint?: number | null;
}

/** Changes to a mate: assembly.editMate. */
export interface MateEdit {
  kind?: MateKind;
  value?: number;
  aligned?: boolean;
  suppressed?: boolean;
}

/** A named explode step: some pieces moved a distance along a direction. */
export interface ExplodeStep {
  /** 0 for a step not yet kept. */
  id: number;
  name: string;
  pieces: PieceRef[];
  /** Unit vector in assembly space. */
  direction: Triple;
  /** mm. */
  distance: number;
}

/** What assembly.pieceState does to a piece (Show All to every piece). */
export type PieceAction = 'hide' | 'show' | 'showAll' | 'ground' | 'unground';

/** Two pieces that overlap or come closer than the clearance. */
export interface PieceIssue {
  a: PieceRef;
  b: PieceRef;
  /** Approximate overlap, cubic millimetres; zero when they only come close. */
  overlap: number;
  tooClose: boolean;
  /** Box of the problem in assembly space, min then max corner. */
  bounds: [Triple, Triple];
}

/** One row of the cut list: a piece's material left after machining. */
export interface PieceCutRow extends PieceNaming {
  /** Extent along the sheet's X and Y; null when nothing is left. */
  length: number | null;
  width: number | null;
  thickness: number;
  /** Area of the piece's outline, square mm, to find identical pieces. */
  area?: number;
  /** Sized from its exact cut edges, else the simulation grid. */
  exact?: boolean;
}

/** A face picked on a piece: a point on its surface in the piece's own frame. */
export interface PieceFace extends PieceRef {
  point: Triple;
}

/** assembly.matePieces: the moving face meets the target face. */
export interface MateArgs {
  target: PieceFace;
  moving: PieceFace;
  /** What the mate holds; face against face when left out. */
  kind?: MateKind;
  /** Gap along the target face's normal, mm; the angle in degrees for an angle mate. */
  offset: number;
  /** The mate this one takes the place of (changing the mate just made). */
  replace?: number | null;
  /** Turn the moving piece over, so its side opposite the picked face meets the target. */
  flip: boolean;
  /** Turn about the face normal, degrees; null picks the nearest quarter turn. */
  turn: number | null;
}

/** A face found on a piece: a point on it and its outward normal, in the piece's frame. */
export interface PieceFaceHit {
  point: Triple;
  normal: Triple;
}

/** A piece's place after a solve. */
export interface SolvedPlace {
  piece: PieceRef;
  position: Triple;
  rotation: Triple;
}

/** assembly.solvePlace: where every piece would end were one placed, the mates solved. */
export interface SolvePlaceReply {
  places: SolvedPlace[];
  /** Mates the solve could not meet. */
  conflicts: number;
  revision: number;
}

export interface MateReply {
  revision: number;
  /** The mate made (or changed). */
  mate: number;
  state: MateStanding | null;
  /** The piece that moved to meet the other (a grounded one never does). */
  moved: PieceRef;
  position: Triple | null;
  rotation: Triple | null;
  /** The turn used, to send again when the mate is changed. */
  turn: number;
}
