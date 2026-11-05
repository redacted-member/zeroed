// JSON shapes of Sketch Carving, Laser Sketch and the Vector Selector.
// Lengths are millimetres, powers percent of the machine maximum.

export type SketchSource = 'bitmap' | 'model';
export type SketchBoundary = 'source' | 'vector' | 'level';
export type SketchFill = 'together' | 'regionally' | 'regionallyWithinVectors';

/** What both sketch toolpaths trace. */
export interface SketchTrace {
  source: SketchSource;
  /** A bitmap entity placed in the job; null uses `path` and the placement below. */
  bitmap: number | null;
  path: string;
  origin: { x: number; y: number };
  width: number;
  /** Zero keeps the image's proportions. */
  height: number;
  boundary: SketchBoundary;
  /** The model level whose components limit a 'level' boundary. */
  level?: number | null;
  /** How far outside the boundary, whichever it is, the sketch may go. */
  boundaryOffset: number;
  /** 0 (fine) to 100 (heavy lines, less detail). */
  lineThickness: number;
}

/** Which vectors a toolpath machines, kept with it when `associate` is set. */
export interface VectorFilter {
  open: boolean;
  closed: boolean;
  onlyCircles: boolean;
  /** With onlyCircles, the diameter wanted; null takes any. */
  diameter: number | null;
  tolerance: number;
  /** Layer names; empty takes every visible layer. */
  layers: string[];
  /** Automatic selection: the filter picks the vectors on every calculation. */
  associate: boolean;
}

export interface SketchCarveParams extends SketchTrace {
  startDepth: number;
  flatDepth: number | null;
  projectToModel?: boolean;
  selector?: VectorFilter;
}

export interface LaserSketchParams extends SketchTrace {
  power: number | null;
  speed: number | null;
  overscan: number;
  stepover: number;
  angle: number;
  crossHatch: boolean;
  fill: SketchFill;
  selector?: VectorFilter;
}

/** What `cam.selectVectors` answers. */
export interface SelectedVectors {
  ids: number[];
  closed: number;
  open: number;
}

export const DEFAULT_FILTER: VectorFilter = {
  open: true,
  closed: true,
  onlyCircles: false,
  diameter: null,
  tolerance: 0.1,
  layers: [],
  associate: false,
};
