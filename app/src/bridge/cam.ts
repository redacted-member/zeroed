// Shapes of the tool database, post processor and toolpath commands.
// Lengths are millimetres; Z values in move previews are job Z.

import type { RotaryWiring } from './prefs';
import type { LaserSketchParams, SketchCarveParams, VectorFilter } from './sketch';
import type { GadgetValue } from './gadgets';
import type { Heights } from './heights';
import type { Point, TextAnchor } from './document';
import type { BlobHeader, HeightCodeHeader, Side } from './types';

export type ToolKind =
  | 'endMill'
  | 'ballNose'
  | 'vBit'
  | 'engravingTool'
  | 'taperedBallNose'
  | 'radiusedEndMill'
  | 'formTool'
  | 'drill'
  | 'diamondDrag'
  | 'laserTool'
  | 'threadMill';

/**
 * A tool. The database list adds `group`, `diameter` and `toolTypeName`; a
 * toolpath's own copy of its tool does not have them, so use `toolDiameter`.
 */
export interface Tool {
  id: number;
  name: string;
  number: number;
  group?: number;
  diameter?: number;
  toolTypeName?: ToolKind;
  toolType: { kind: ToolKind; diameter?: number } & Record<string, unknown>;
  passDepth: number;
  stepover: number;
  stepoverPercent: number;
  feedRate: number;
  plungeRate: number;
  feedUnits: string;
  spindleSpeed: number;
  notes: string;
  units: 'mm' | 'inch';
  clearance: number;
  cuttingData: CuttingData[];
  /** Cutting flutes, for the chip load; zero or absent when unknown. */
  flutes?: number;
  /** Flute length above the tip, mm; zero or absent leaves the flutes and shank unchecked. */
  fluteLength?: number;
  /** Shank diameter, mm; zero or absent for the cutting diameter. */
  shankDiameter?: number;
  /** How far the tool reaches below its holder, mm. */
  lengthBelowHolder?: number;
  /** Holder diameter, mm; zero or absent leaves the holder unchecked. */
  holderDiameter?: number;
  /** Holder length, mm. */
  holderLength?: number;
  /** False for a tool that cannot cut at its center (it may ramp, not plunge). */
  centerCutting?: boolean;
  /** The tool's own naming variables. */
  variables?: ToolVariable[];
  /** The list only: the name with its template filled in for the context. */
  displayName?: string;
  /** The list only: millimetres per tooth for the context, if known. */
  chipLoad?: number | null;
  /** The list only: the tool as a toolpath cuts with it in the context. */
  resolved?: Tool;
}

/** Feeds and speeds of a tool for one machine and material pair. */
export interface CuttingData {
  machine: string;
  material: string;
  feedUnits: string;
  feedRate: number;
  plungeRate: number;
  spindleSpeed: number;
  passDepth: number;
  stepover: number;
  /** Tool number on this machine, where it differs. */
  number?: number | null;
}

export interface ToolVariable {
  name: string;
  value: string;
}

/** How an imported tool database joins the one there. */
export type ToolImportMode = 'import' | 'mergeOverwrite' | 'mergeKeep';

export interface NameFormats {
  /** Name format per tool kind. */
  formats: Partial<Record<ToolKind, string>>;
  /** Built in variable names, in menu order. */
  variables: string[];
}

/** Cutting diameter in millimetres, from whichever form of tool this is: a
 * laser's is the width its beam burns (the kerf). */
export function toolDiameter(tool: Tool): number {
  const kerf = tool.toolType['kerf'];
  return tool.diameter ?? tool.toolType.diameter ?? (typeof kerf === 'number' ? kerf : 0);
}

export interface ToolGroup {
  id: number;
  name: string;
  parent: number | null;
  path: string[];
}

export interface ToolDatabaseList {
  dbRevision: number;
  groups: ToolGroup[];
  tools: Tool[];
  materials: string[];
  machines: string[];
  /** The material toolpaths take their cutting data for, empty for none. */
  activeMaterial?: string;
  /** The machine and material the list was resolved for. */
  machine?: string;
  material?: string;
}

/** The tool a toolpath form should cut with: the list entry resolved for the context. */
export function toolForToolpath(tool: Tool): Tool {
  const resolved = tool.resolved;
  if (resolved === undefined) {
    return tool;
  }
  return {
    ...resolved,
    group: tool.group,
    diameter: tool.diameter,
    toolTypeName: tool.toolTypeName,
  };
}

export interface PostSummary {
  id: string;
  name: string;
  fileExtension: string;
  units: 'mm' | 'inch';
  arcs: boolean;
  /** Whether it can drive a laser. */
  jet?: boolean;
  /** Whether it can wrap a rotary job. */
  rotary?: boolean;
  /** The letter a blank along X is written on, when it can wrap one. */
  rotaryWrapY?: string | null;
  /** The letter a blank along Y is written on, when it can wrap one. */
  rotaryWrapX?: string | null;
  /** Whether it can change tools part way through a file. */
  toolChange?: boolean;
  /** Whether it can drive a drill bank (the drill bank post sections). */
  drillBank?: boolean;
  /** Where an installed post was read from; empty for a shipped one. */
  sourcePath?: string;
  /** The driver program Output direct to machine sends files to. */
  directOutput?: string;
  /** Whether "M6 macro applies tool length" changes what it writes. */
  lengthChoice?: boolean;
}

export interface PostText {
  id: string;
  text: string;
  path: string;
}

/** One row of a post's History table. */
export interface PostChange {
  who: string;
  when: string;
  what: string;
}

/** `posts.changelog`: the History table of a post, as written. */
export interface PostChangeLog {
  id: string;
  entries: PostChange[];
}

export type StrategyName =
  | 'profile'
  | 'pocket'
  | 'drill'
  | 'vgroove'
  | 'inlay'
  | 'moulding'
  | 'quickEngrave'
  | 'chamfer'
  | 'fluting'
  | 'prism'
  | 'texture'
  | 'rough3d'
  | 'finish3d'
  | 'laserCutFill'
  | 'laserPicture'
  | 'sketchCarve'
  | 'laserSketch'
  | 'threadMilling'
  | 'straightInlay'
  | 'photoCarve'
  | 'drillBank';

/** How Photo Carve lays out its grooves. */
export type PhotoStrategy = 'raster' | 'hatch' | 'vectors';

/** Photo Carve settings; lengths in millimetres. */
export interface PhotoCarveParams {
  strategy: PhotoStrategy;
  /** Degrees from the X axis, 0 to 90. */
  lineAngle: number;
  /** Percent of the groove width at the full carving depth. */
  lineSpacing: number;
  startDepth: number;
  maxDepth: number;
  stepoverRetract: number;
}
export type ProfileSide = 'outside' | 'inside' | 'on';
export type CutDirection = 'climb' | 'conventional';

export interface TabSpec {
  enabled: boolean;
  count: number;
  length: number;
  thickness: number;
  /** 3D tabs: the cut climbs to the tab top at its middle and back down. */
  triangular?: boolean;
  /** Tabs this far apart along each vector instead of a fixed count. */
  spacing?: number | null;
  /** How Add Tabs places them in the Edit Tabs dialog. */
  placement?: TabRulePlacement;
}

/** Even (older toolpaths), first tab at the start point, or clear of corners. */
export type TabRulePlacement = 'even' | 'firstAtStart' | 'avoidCorners';

/** What the Edit Tabs dialog asks the kernel for. */
export type TabOp = 'auto' | 'click' | 'move' | 'clear';

export type RampKind = 'smooth' | 'zigZag' | 'spiral';

export interface RampSpec {
  enabled: boolean;
  distance: number;
  kind?: RampKind;
  /** Ramp at this angle in degrees: a smooth ramp's length follows from the
   * step down, a zig zag goes back and forth over `distance` as often as needed. */
  angle?: number | null;
  /** With a lead in, ramp down the lead instead of the cut. */
  onLeadIn?: boolean;
}

/** Side of open vectors, looking along them. */
export type OpenSide = 'on' | 'left' | 'right';
export type CutOrder = 'optimized' | 'selectionOrder' | 'leftToRight' | 'bottomToTop' | 'grid';
export type StartAt = 'optimized' | 'vectorStart' | 'boundingBox';
/** One of the nine points of a bounding box, as the kernel names them. */
export type BoxPoint = TextAnchor;

/** A separate thin finishing pass at full depth. */
export interface LastPassSpec {
  enabled: boolean;
  allowance: number;
  reverse: boolean;
}

/** A placed tab kept on its vector (the kernel's anchor; the forms pass it back unchanged). */
export interface TabAnchor {
  id: number;
  loopIndex: number;
  /** Fraction of the loop's length from its start. */
  along: number;
  at: Point;
}
export interface ProfileParams {
  startDepth: number;
  cutDepth: number;
  passDepth: number | null;
  /** Evenly spaced passes by count; null goes by the pass depth. */
  passCount?: number | null;
  /** Thickness of the last depth pass; null spreads the passes evenly. */
  lastPassThickness?: number | null;
  /** Tabs placed or edited in the Edit Tabs dialog, as points on the vectors. */
  tabPositions?: Point[] | null;
  /** Written by the kernel: where each placed tab sits on its vector, so tabs follow the vector. */
  tabAnchors?: Array<TabAnchor | null> | null;
  /** Keep every pass the exact pass depth (no 15% stretch to save a pass). */
  exactPassDepth?: boolean;
  /** Custom pass depths below the surface; they win over the other pass settings. */
  passList?: number[] | null;
  side: ProfileSide;
  openSide?: OpenSide;
  direction: CutDirection;
  allowance: number;
  lastPass?: LastPassSpec;
  tabs: TabSpec;
  ramp: RampSpec;
  lead: LeadSpec;
  order?: CutOrder;
  /** The ticked Order options; each is tried and the least travel kept. */
  orders?: CutOrder[];
  startAt?: StartAt;
  /** The bounding box point Closest on Bounding Box starts near. */
  startAnchor?: BoxPoint;
  sharpCorners?: boolean;
  /** Sharp Internal Corner (3D): a V-bit runs its tip up into internal corners. */
  sharpInternalCorners?: boolean;
  /** How far the wall leans back from upright as it rises, degrees; 0 for upright. */
  wallAngle?: number;
  /** The most a step of a tapered wall cut with an end mill stands out of the slope, mm. */
  scallop?: number;
  /** An open vector's tapered wall moves as a whole (the Joint Creator's socket walls). */
  rigid?: boolean;
}

export type LeadKind = 'none' | 'straight' | 'arc';

/** Lead into and out of closed profile cuts. Lengths in millimetres. */
export interface LeadSpec {
  kind: LeadKind;
  length: number;
  angleDeg: number;
  radius: number;
  /** How far round a circular lead turns, 0.1 to 90 degrees; 90 when absent. */
  arcAngleDeg?: number;
  overcut: number;
  /** Leave along a lead too; absent in old toolpaths, which always did. */
  leadOut?: boolean;
}

export interface PocketParams {
  startDepth: number;
  cutDepth: number;
  passDepth: number | null;
  stepover: number | null;
  strategy: 'offset' | 'raster';
  rasterAngle: number;
  direction: CutDirection;
  allowance: number;
  profilePass: boolean;
  /** Whether the raster's boundary pass comes before or after the lines. */
  profilePassWhen?: 'first' | 'last';
  ramp: RampSpec;
  /** Older toolpaths: a larger tool that clears the bulk first; the main
   * tool cleans up after it. The form sends null and uses `restTools`. */
  clearanceTool: Tool | null;
  /** The Tools list after the request's own tool, in cutting order: each
   * clears what the tools before it could not reach. */
  restTools?: Tool[];
  passCount?: number | null;
  lastPassThickness?: number | null;
  exactPassDepth?: boolean;
  passList?: number[] | null;
  /** Clear separate pockets in the order their vectors were selected. */
  selectionOrder?: boolean;
}

/** How a flat area clearance tool clears (V-Groove and inlay). */
export interface ClearanceOptions {
  strategy: 'offset' | 'raster';
  rasterAngle: number;
  direction: CutDirection;
  ramp: RampSpec;
}

export interface DrillParams {
  /** Cut in the order the vectors were selected (Use Vector Selection Order). */
  selectionOrder?: boolean;
  startDepth: number;
  cutDepth: number;
  /** Depth of each peck, the tool's pass depth; zero drills in one plunge. */
  peckDepth: number;
  fullRetract: boolean;
  /** Retract Gap (R) in millimetres; older toolpaths have none. */
  retractGap?: number;
  /** Seconds at the bottom of each drill pass. */
  dwell: number;
}

/** One position of a drill bank: its Drill Number (0 when no drill is
 * fitted) and diameter in millimetres. */
export interface BankDrill {
  number: number;
  diameter: number;
}

/** A drill bank: `horizontal` by `vertical` positions `pitch` apart, row by
 * row from the bottom left, with the reference (central) drill's column and
 * row. */
export interface DrillBankTemplate {
  horizontal: number;
  vertical: number;
  pitch: number;
  reference: [number, number];
  drills: BankDrill[];
}

/** Drill Bank settings: the bank, the matching tolerances and the drilling. */
export interface DrillBankParams {
  template: DrillBankTemplate;
  pitchTolerance: number;
  diameterTolerance: number;
  drill: DrillParams;
}

export interface VGrooveParams {
  startDepth: number;
  /** Deepest cut; null lets the V go as deep as the shape needs. */
  flatDepth: number | null;
  /** Deepest single pass; null cuts the full depth at once. */
  maxPassDepth: number | null;
  /** The single clearance tool of older toolpaths; the form sends null and
   * uses `clearanceTools`. */
  clearanceTool: Tool | null;
  /** How the clearance tools clear; absent in old toolpaths (offset, climb). */
  clearanceOptions?: ClearanceOptions;
  /** Flat area clearance tools in cutting order; used only with a flat depth. */
  clearanceTools?: VGrooveClearanceTool[];
  /** Carve the shapes in the order their vectors were selected in. */
  selectionOrder?: boolean;
  /** Start each shape nearest its vector's start point. */
  vectorStart?: boolean;
}

/** One flat area clearance tool of a V-Groove. */
export interface VGrooveClearanceTool {
  tool: Tool;
  /** A later V-bit or engraving tool follows the centre line into the
   * corners the tools before it left, raised to fit. */
  cornerSharpen: boolean;
  /** This tool's own strategy, direction and ramps; the shared options when absent. */
  options?: ClearanceOptions | null;
}

/** Which half of an inlay a toolpath cuts: the pocket or the plug. */
export type InlayPart = 'female' | 'male';

export interface InlayParams {
  /** Depth of the pocket's flat floor below the surface. */
  pocketDepth: number;
  /** Room left for glue between the plug's top and the pocket's floor. */
  glueGap: number;
  /** Surface Gap: depth the plug's waste is cleared below its start; null uses the pocket depth. */
  plugDepth: number | null;
  /** Plug Outer Boundary; absent in older toolpaths (vector offset). */
  plugBoundary?: PlugBoundary;
  /** How much further than the plug walls need a vector offset boundary reaches; null is 2 mm. */
  boundaryExtra?: number | null;
  /** Mirror the plug left to right, since it is glued in face down. */
  mirrorPlug: boolean;
  /** Distance the plug's outer boundary grows out from the shapes; null picks one. */
  boundaryOffset: number | null;
  /** Deepest single pass; null cuts the full depth at once. */
  maxPassDepth: number | null;
  /** Flat end tool that clears the flat areas; older toolpaths keep it, the form sends the list. */
  clearanceTool: Tool | null;
  part: InlayPart;
  /** How the clearance tools clear; absent in old toolpaths (offset, climb). */
  clearanceOptions?: ClearanceOptions;
  /** Use Clearance Tools: in cutting order, for both halves. */
  clearanceTools?: VGrooveClearanceTool[];
}

/** Array Copy Toolpath settings; lengths in millimetres. */
export interface ArrayCopySpec {
  rows: number;
  columns: number;
  /** From one copy's start to the next, or between their bounding boxes. */
  spacing: 'offset' | 'gap';
  x: number;
  y: number;
  minimizeToolChanges: boolean;
}

/** Where the plug's waste ends: the sheet edges or an offset round the vectors. */
export type PlugBoundary = 'sheetLimits' | 'vectorOffset';

export type MouldingStrategy = 'along' | 'across';

export interface MouldingParams {
  /** Distance between passes across the moulding; null uses the tool's stepover. */
  stepover: number | null;
  /** Factor the profile's width is scaled by. */
  widthScale: number;
  /** Factor the profile's height is scaled by. */
  heightScale: number;
  /** Material left on the moulding; negative cuts below the profile. */
  allowance: number;
  /** Passes along the rail, or across it stepping along. */
  strategy: MouldingStrategy;
  /** Depth of the profile's top below the surface; absent in old toolpaths (0). */
  startDepth?: number;
  /** How far the passes carry on past the profile's sides; absent is 0. */
  boundaryOffset?: number;
  /** Along: space passes by the distance over the profile's surface. */
  varyStepover?: boolean;
  /** Along: mitre the passes at rail corners instead of rolling round. */
  sharpCorners?: boolean;
  /** A larger tool that roughs the moulding first. */
  clearanceTool?: Tool | null;
  /** Material the clearance tool leaves. */
  clearanceAllowance?: number;
  /** Ramp the clearance tool into each level. */
  clearanceRamp?: RampSpec;
  /** The clearance tool also cuts the profile's flats it fits on, to their height. */
  machineFlatRegions?: boolean;
  /** The finishing tool leaves out the passes over those flats. */
  skipFlatRegions?: boolean;
  /** Carry past the profile's sides by the tool radius and a tenth instead of boundaryOffset. */
  autoBoundaryOffset?: boolean;
}

export interface EngraveParams {
  depth: number;
  passes: number;
  mode: 'outline' | 'fill';
  stepover: number;
  angle: number;
  pattern?: EngraveFill;
  /** Every pass at full depth, for a sprung nose cone. */
  noseCone?: boolean;
}

export type EngraveFill = 'raster' | 'crossHatch' | 'offset';

/** Straight walled inlay settings: one half per toolpath. Lengths in millimetres. */
export interface StraightInlayParams {
  part: 'female' | 'male';
  male: 'straight' | 'stepped';
  female: 'pocket' | 'hole';
  startDepth: number;
  cutDepth: number;
  passDepth: number | null;
  /** Extra cut off the insert and off a hole's edge. */
  allowance: number;
  /** Extra cut round the pocket's edge. */
  pocketAllowance: number;
  stepDepth: number;
  stepWidth: number;
  direction: CutDirection;
  tabs: TabSpec;
  stepover: number | null;
  strategy: 'offset' | 'raster';
  rasterAngle: number;
  clearanceTool: Tool | null;
  /** An insert's or hole's Profile options; its side, depths and allowance are the inlay's. */
  profileOptions?: ProfileParams | null;
  /** A pocket's Pocket options; its depths are the inlay's. */
  pocketOptions?: PocketParams | null;
  /** A pocket's tools after the request's own, in cutting order. */
  restTools?: Tool[];
}

/** Thread Milling settings. Lengths in millimetres. */
export interface ThreadMillingParams {
  startDepth: number;
  /** Deepest the cutter goes below the start depth. */
  maxDepth: number;
  pitch: number;
  /** Major (crest) diameter. */
  diameter: number;
  /** How much deeper than the basic thread form to cut. */
  fitTolerance: number;
  kind: 'internal' | 'external';
  hand: 'right' | 'left';
  cut: 'down' | 'up';
  /** Thread the circles in selection order; absent in old toolpaths (nearest next). */
  selectionOrder?: boolean;
}

export type LaserMode = 'cut' | 'fill' | 'cutAndFill';
export type LaserSide = 'on' | 'outside' | 'inside';
export type LaserHatch = 'lines' | 'cross';

/** Laser Cut and Fill settings; powers are percentages, speeds mm per minute. */
export interface LaserCutFillParams {
  mode: LaserMode;
  /** Kerf compensation: half the tool's kerf outside or inside the vector. */
  side: LaserSide;
  /** No. Repeats: times each pass is burnt; older toolpaths send `passes`. */
  repeats?: number;
  passes?: number;
  /** Cut Depth in millimetres: deeper cuts step down in passes of the
   * laser's pass depth (No. Passes); zero burns at the surface. */
  cutDepth?: number;
  /** Null burns at the tool's power. */
  power: number | null;
  /** Null runs at the tool's speed. */
  speed: number | null;
  /** Null fills at the cut power. */
  fillPower: number | null;
  lineInterval: number;
  angle: number;
  hatch: LaserHatch;
  bidirectional: boolean;
  /** Overscan in millimetres (older toolpaths). */
  overscan: number;
  /** Overscan as a percentage of each fill line; used when above zero. */
  overscanPercent?: number;
  /** Fill Regionally: each separate area filled on its own. */
  fillRegions?: boolean;
  /** Extra offset past the kerf for an outside or inside cut; absent in older toolpaths. */
  allowance?: number;
  /** Automatic vector selection, kept with the toolpath. */
  selector?: VectorFilter;
}

export type PictureMode = 'powerModulation' | 'dither';
export type DitherKind = 'floydSteinberg' | 'ordered';

/** The lines Laser Picture burns along. */
export type PictureStrategy = 'raster' | 'hatch' | 'vectors';

/** Laser Picture settings: the bitmap selected in the drawing, burnt along
 * raster or hatch lines or the selected vectors (older toolpaths name an
 * image file in `path`). */
export interface LaserPictureParams {
  strategy?: PictureStrategy;
  /** Line Angle, -90 to 90 degrees. */
  lineAngle?: number;
  /** No. Passes: times the picture is burnt. */
  repeats?: number;
  /** Overscan as a percentage of each line; used when above zero. */
  overscanPercent?: number;
  path: string;
  /** Lower left corner of the picture, in millimetres. */
  origin: { x: number; y: number };
  width: number;
  /** Zero keeps the image's proportions. */
  height: number;
  lineInterval: number;
  mode: PictureMode;
  dither: DitherKind;
  minPower: number;
  /** Null burns black at the tool's power. */
  maxPower: number | null;
  speed: number | null;
  invert: boolean;
  bidirectional: boolean;
  overscan: number;
}

export interface ChamferParams {
  /** Cut in the order the vectors were selected (Use Vector Selection Order). */
  selectionOrder?: boolean;
  /** Bevel width from the vector to its top edge; used when depth is null. */
  width: number;
  /** Bevel depth below its top edge; when set the width follows the tool angle. */
  depth: number | null;
  /** Side of closed vectors the material is on; open vectors are cut on the line. */
  side: ProfileSide;
  startDepth: number;
  direction?: CutDirection;
  /** Moves the cut out and down the slope so the cone's edge finishes the foot. */
  overcut?: number;
  /** Up: the vector is the bevel's foot. Down: it is the bevel's top edge. */
  slope?: ChamferSlope;
  /** Bevel angle from the vertical for a ball nose, in degrees. */
  angle?: number;
}

export type ChamferSlope = 'up' | 'down';

export type FlutingRamp = 'linear' | 'smooth';

export interface FlutingParams {
  /** Cut in the order the vectors were selected (Use Vector Selection Order). */
  selectionOrder?: boolean;
  /** Depth the flute starts from; absent in old toolpaths (zero). */
  startDepth?: number;
  depth: number;
  ramp: FlutingRamp;
  rampStart: boolean;
  rampEnd: boolean;
  /** Length of each ramp; null ramps over the whole vector (half each way). */
  rampLength: number | null;
  /** Ramp length as a percent of the longest ramp; used instead of the length. */
  rampPercent?: number | null;
}

/** Inside sinks the shape as a V channel; Outside leaves it standing as a ridged prism. */
export type PrismDirection = 'inside' | 'outside';

export interface PrismParams {
  /** Cut in the order the vectors were selected (Use Vector Selection Order). */
  selectionOrder?: boolean;
  direction: PrismDirection;
  /** Deepest the prism may go below the start depth; null goes as deep as the shape asks. */
  maxDepth: number | null;
  startDepth: number;
  /** Round the raised prism's passes; absent in old toolpaths (climb). */
  cutDirection?: CutDirection;
}

export interface TextureParams {
  /** Direction of the strokes and their rows, degrees counter clockwise from X. */
  angleDeg: number;
  /** Most a stroke may turn from the angle either way, 0 to 90 degrees. */
  angleVariationDeg: number;
  strokeLength: number;
  /** Fraction (0 to 1) a stroke may be longer or shorter by. */
  lengthVariation: number;
  /** Distance between rows of strokes. */
  spacing: number;
  /** Most a stroke moves across its row either way, percent (0 to 50) of the spacing. */
  stepoverVariationPercent?: number | null;
  /** Depth below the surface the strokes start from. */
  startDepth?: number;
  /** Deepest a stroke cuts below the start depth. */
  depth: number;
  /** Fraction (0 to 1) of the depth a stroke may be shallower by. */
  depthVariation: number;
  /** Same seed, same strokes. */
  seed: number;
  /** Shortest stroke as a percent of the stroke length; replaces the length variation. */
  minLengthPercent?: number | null;
  /** Shallowest stroke as a percent of the depth; replaces the depth variation. */
  minDepthPercent?: number | null;
  /** Most a stroke overlaps the one before, percent of its length; null leaves gaps. */
  maxOverlapPercent?: number | null;
  /** 0 always overlaps by the maximum, 100 anywhere from none to the maximum. */
  overlapVariationPercent?: number;
  /** Distance the strokes keep in from the vectors. */
  boundaryOffset?: number;
  /** Cut along the selected vectors instead of scattering strokes inside them. */
  vectorsAsPattern?: boolean;
  /** With a vector pattern, dip in and out like a flute rather than cut at depth. */
  ramp?: boolean;
}

export type Boundary3d = 'model' | 'material' | 'selection' | 'level';

/** How 3D Finishing covers the surface. */
export type Finish3dStrategy = 'raster' | 'rasterXY' | 'offset';

/** How 3D Roughing clears the material. */
export type Rough3dStrategy = 'zLevelRaster' | 'zLevelOffset' | 'raster3d';

/** When a Z level roughing cuts round the outline of each level. */
export type Rough3dProfile = 'none' | 'first' | 'last';

/** Whole levels first, or each separate area down to the bottom first. */
export type Rough3dOrder = 'levelByLevel' | 'depthFirst';

interface Base3dParams {
  stepover: number | null;
  /** Raster angle in degrees; offset strategies ignore it. */
  angle: number;
  allowance: number;
  boundary: Boundary3d;
  /** Model level id for a level boundary. */
  level?: number;
  /** Grows the boundary outward (negative shrinks), millimetres. */
  boundaryOffset?: number;
}

export interface Finish3dParams extends Base3dParams {
  strategy: Finish3dStrategy;
  /** Offset rings: climb or conventional. */
  direction?: CutDirection;
  /** Offset rings: lift between rings, millimetres; 0 stays on the surface. */
  stepoverRetract?: number;
  /** Tools after the first, in cutting order, each finishing what the tools
   * before it could not reach. */
  restTools?: Tool[];
  /** Material left thinner than this is not worth another tool, millimetres. */
  minimumDetail?: number;
  /** How far each area left for a later tool is grown, millimetres. */
  restBoundaryOffset?: number;
}

export interface Rough3dParams extends Base3dParams {
  passDepth: number | null;
  strategy: Rough3dStrategy;
  /** Older toolpaths: raster clearing finishes each level round its outline. */
  profileEachLevel: boolean;
  profile?: Rough3dProfile;
  order?: Rough3dOrder;
  reverseStep?: boolean;
  avoidMachined?: boolean;
  ramp?: RampSpec;
}

export type ToolpathRequest =
  | { strategy: 'profile'; name?: string; tool: Tool; params: ProfileParams; ids: number[] }
  | { strategy: 'pocket'; name?: string; tool: Tool; params: PocketParams; ids: number[] }
  | { strategy: 'drill'; name?: string; tool: Tool; params: DrillParams; ids: number[] }
  | { strategy: 'vgroove'; name?: string; tool: Tool; params: VGrooveParams; ids: number[] }
  | { strategy: 'inlay'; name?: string; tool: Tool; params: InlayParams; ids: number[] }
  | { strategy: 'moulding'; name?: string; tool: Tool; params: MouldingParams; ids: number[] }
  | { strategy: 'quickEngrave'; name?: string; tool: Tool; params: EngraveParams; ids: number[] }
  | { strategy: 'chamfer'; name?: string; tool: Tool; params: ChamferParams; ids: number[] }
  | { strategy: 'fluting'; name?: string; tool: Tool; params: FlutingParams; ids: number[] }
  | { strategy: 'prism'; name?: string; tool: Tool; params: PrismParams; ids: number[] }
  | { strategy: 'texture'; name?: string; tool: Tool; params: TextureParams; ids: number[] }
  | { strategy: 'rough3d'; name?: string; tool: Tool; params: Rough3dParams; ids: number[] }
  | { strategy: 'finish3d'; name?: string; tool: Tool; params: Finish3dParams; ids: number[] }
  | {
      strategy: 'laserCutFill';
      name?: string;
      tool: Tool;
      params: LaserCutFillParams;
      ids: number[];
    }
  | {
      strategy: 'laserPicture';
      name?: string;
      tool: Tool;
      params: LaserPictureParams;
      ids: number[];
    }
  | { strategy: 'sketchCarve'; name?: string; tool: Tool; params: SketchCarveParams; ids: number[] }
  | { strategy: 'photoCarve'; name?: string; tool: Tool; params: PhotoCarveParams; ids: number[] }
  | {
      strategy: 'laserSketch';
      name?: string;
      tool: Tool;
      params: LaserSketchParams;
      ids: number[];
    }
  | {
      strategy: 'straightInlay';
      name?: string;
      tool: Tool;
      params: StraightInlayParams;
      ids: number[];
    }
  | {
      strategy: 'threadMilling';
      name?: string;
      tool: Tool;
      params: ThreadMillingParams;
      ids: number[];
    }
  | {
      strategy: 'drillBank';
      name?: string;
      tool: Tool;
      params: DrillBankParams;
      ids: number[];
    };

export interface ToolpathSummary {
  id: number;
  name: string;
  kind: string;
  /** The strategy that made it; absent for imported and merged toolpaths. */
  strategy?: string | null;
  visible: boolean;
  stale: boolean;
  toolName: string;
  toolNumber: number;
  estimatedSeconds: number;
  moves: number;
  /** The side of a double sided job it cuts. */
  side?: Side;
  /** The sheet it belongs to; 0 or absent shows on every sheet. */
  sheet?: number;
  /** The toolpath group it is in; 0 or absent for none. */
  group?: number;
  /** The tool's shape, such as `endMill` or `vBit`. */
  toolType?: string;
  /** False when calculating left it with no cutting moves. */
  cuts?: boolean;
  /** The gadget that made it, which Edit reopens; absent or null for none. */
  gadget?: string | null;
}

/** A folder in the toolpath list. */
export interface ToolpathGroup {
  id: number;
  name: string;
  /** The sheet it belongs to; 0 shows on every sheet. */
  sheet: number;
  expanded: boolean;
}

/** A toolpath that cuts below the bottom of its material. */
export interface TooDeep {
  id: number;
  name: string;
  /** How deep it cuts below the material surface, millimetres. */
  depth: number;
  /** Its material's thickness, millimetres. */
  thickness: number;
}

/** `cam.depthCheck`: whether hidden toolpaths were checked, and the ones too deep. */
export interface DepthCheckReply {
  all: boolean;
  toolpaths: TooDeep[];
}

export interface ToolpathDetail {
  id: number;
  name: string;
  kind: string;
  visible: boolean;
  stale: boolean;
  tool: Tool;
  request: ToolpathRequest | null;
  estimatedSeconds: number;
  /** The gadget that made it and the values it ran with; absent or null for none. */
  gadget?: { name: string; values: Record<string, GadgetValue> } | null;
}

/** Move kinds in the `cam.moves` preview. */
export const MOVE_KIND = {
  rapid: 0,
  feed: 1,
  plunge: 2,
  arc: 3,
  dwell: 4,
  /** A lift straight up. */
  retract: 5,
  /** A flat feed move from a closed contour to the next. */
  join: 6,
} as const;

export interface MovesHeader extends BlobHeader {
  revision: number;
  toolpaths: Array<{ id: number; offset: number; count: number; visible: boolean }>;
}

export interface MovesReply {
  revision: number;
  toolpaths: MovesHeader['toolpaths'];
  /** x, y, z triples, one per point. */
  xyz: Float32Array;
  /** Width the tool cuts at the material surface at each point, millimetres. */
  width: Float32Array;
  /** Move kind arriving at each point. */
  kind: Uint8Array;
}

/** An order Merge Toolpaths can try for the contours. */
export type MergeOrder = 'leftToRight' | 'bottomToTop' | 'grid' | 'shortestPath';

/** Merge Toolpaths choices: the quickest of `orders` is kept. */
export interface MergeOptions {
  orders: MergeOrder[];
  byPart: boolean;
}

/** How Toolpath Tiling divides the job. */
export type TilingMethod = 'individual' | 'feedX' | 'feedY';

/** Tiles for a job bigger than the machine; 0 leaves that direction whole. */
export interface TileSpec {
  method?: TilingMethod;
  tileWidth: number;
  tileHeight: number;
  overlap: number;
  /** The part size tile left over comes first instead of last. */
  smallestFirst?: boolean;
}

/** The document's Toolpath Tiling settings. */
export interface Tiling {
  enabled: boolean;
  spec: Required<TileSpec>;
  /** Index of the tile the views show. */
  active: number;
  /** Draw the active tile where it sits in the job, not at the origin. */
  originalPosition: boolean;
}

/** One tile: [minX, minY, maxX, maxY] boxes in job millimetres. */
export interface TileInfo {
  row: number;
  column: number;
  /** The tile's own area. */
  nominal: [number, number, number, number];
  /** What it cuts, overlap included. */
  area: [number, number, number, number];
  shift: [number, number];
}

export interface TilingReply extends Tiling {
  tiles: TileInfo[];
}

export interface SaveToolpathsArgs {
  toolpathIds: number[];
  post: string;
  path: string;
  outputAllVisible?: boolean;
  oneFilePerToolpath?: boolean;
  /** The Save Toolpaths output option. */
  output?: 'selected' | 'oneFile' | 'multipleFiles';
  /** With multiple files, neighbours cut with one tool share a file. */
  groupWherePossible?: boolean;
  /** Writes one file per tile, cuts clipped to it and moved to the origin. */
  tiling?: TileSpec;
  /** Divides the toolpaths into the tiles Toolpath Tiling set up. */
  outputTiled?: boolean;
  /** Off: a probing post writes G43 H after each M6 (default on). */
  m6AppliesLength?: boolean;
  /** How the chosen machine's rotary axis is wired, for a rotary job. */
  rotaryWiring?: RotaryWiring;
  /** Posts an air cut or a scrap cut instead of the real program. */
  trialCut?: TrialCut | null;
}

/** A trial run posted before the real file, lengths in millimetres. */
export type TrialCut = { kind: 'air'; height: number } | { kind: 'scrap'; depth: number };

export type PreviewQuality = 'standard' | 'high' | 'veryHigh' | 'extremelyHigh' | 'maximum';

/** What a preview check found. */
export type IssueKind =
  'rapid' | 'plunge' | 'flute' | 'shank' | 'holder' | 'clamp' | 'gouge' | 'leftover' | 'undercut';

export type Severity = 'high' | 'medium' | 'low';

/** Something the tool met while the preview cut. */
export interface Collision {
  toolpathId: number;
  moveIndex: number;
  /** Tool tip, job space. */
  x: number;
  y: number;
  z: number;
  kind: IssueKind;
  severity: Severity;
}

/** An area of the finished preview off the design (`sim.check`). */
export interface SurfaceIssue {
  kind: IssueKind;
  severity: Severity;
  toolpathId: number;
  /** The toolpath's move passing nearest the worst point. */
  moveIndex: number;
  /** The worst point; z is the material height there. */
  x: number;
  y: number;
  z: number;
  /** How far off the design the worst point is, mm. */
  depth: number;
  /** Square mm. */
  area: number;
}

/** Codes of the check map: none, gouge, leftover, undercut. */
export const CHECK_CODE = { clean: 0, gouge: 1, leftover: 2, undercut: 3 } as const;

export interface SimCheck {
  width: number;
  height: number;
  tolerance: number;
  issues: SurfaceIssue[];
  /** Areas found in all; `issues` lists the worst of them. */
  found: number;
  /** A CHECK_CODE per stock cell. */
  map: Uint8Array;
}

export interface SimRunReply {
  width: number;
  height: number;
  cellSize: number;
  collisions: Collision[];
  /** Estimated machining time of the previewed toolpaths. */
  seconds: number;
}

/** Toolpaths an animated preview will cut, with the outline of each tool. */
export interface SimStartReply {
  width: number;
  height: number;
  cellSize: number;
  revision: number;
  toolpaths: Array<{
    id: number;
    name: string;
    /** Tool outline as [radius, height above the tip] pairs, tip first. */
    tool: Array<[number, number]>;
    /** Shank radius and its bottom and top above the tip, when the flute length is set. */
    shank?: [number, number, number] | null;
    /** Holder radius, bottom and top above the tip, when the holder is checked. */
    holder?: [number, number, number] | null;
  }>;
}

/** How far to move an animated preview on. */
export interface SimStepArgs {
  /** Millimetres along the moves. */
  distance?: number;
  /** The rest of the move being cut. */
  oneMove?: boolean;
  /** The rest of the toolpath being cut. */
  toToolpathEnd?: boolean;
  /** Up to and including the next move that lifts the tool out. */
  toRetract?: boolean;
  /** Everything left. */
  toEnd?: boolean;
  /** Up to where a move of a toolpath passes nearest a job point. */
  toMove?: { toolpathId: number; moveIndex: number; x: number; y: number; z: number };
}

export interface SimStepReply {
  done: boolean;
  /** Tool tip in job space and the toolpath it is cutting. */
  tool: { x: number; y: number; z: number; toolpathId: number } | null;
  collisions: Collision[];
  /** Fraction of the preview cut, 0 to 1. */
  progress: number;
  /** Estimated time of the toolpaths finished so far. */
  seconds: number;
}

/** Heights may come as u16 steps under the same code as the heightfield. */
export interface SimPatchHeader extends BlobHeader, HeightCodeHeader {
  /** The changed rectangles: first column and row, width and height. */
  rects: Array<[number, number, number, number]>;
}

/** A rectangle of stock cells: first column and row, and its size. */
export interface SimRect {
  x0: number;
  y0: number;
  width: number;
  height: number;
}

/**
 * The stock cells changed since the last patch: the rectangles' cells one
 * rectangle after another, row by row. `floor` is empty when every one of
 * them has its underside uncut at the bottom.
 */
export interface SimPatch {
  rects: SimRect[];
  z: Heights;
  owner: Uint16Array;
  floor: Heights;
  /** Laser char of the patched cells (0 none, 255 black); absent until a
   * laser has marked the stock. */
  burn?: Uint8Array;
}

/** Heights may come as u16 steps (see bridge/heights.ts). */
export interface HeightfieldHeader extends BlobHeader, HeightCodeHeader {
  width: number;
  height: number;
  cellSize: number;
  /** Job X and Y of the first cell's centre. */
  originX: number;
  originY: number;
  /** Job Z of the uncut top and of the material bottom. */
  top: number;
  bottom: number;
  revision: number;
}

export interface Heightfield extends HeightfieldHeader {
  /** Material height per cell, rows from the lowest Y (read through
   * heightReader in bridge/heights.ts). */
  z: Heights;
  /** Toolpath list position plus one that last cut each cell; zero uncut. */
  owner: Uint16Array;
  /**
   * Height of the material's underside per cell, raised where the other side
   * of a double sided job cuts. NaN in both `z` and `floor` marks a cell cut
   * through. `flat` while nothing has cut into it: `bottom` under every cell
   * but those cut through. Absent for a model heightfield.
   */
  floor?: Heights | 'flat';
  /** How dark a laser has charred each cell, 0 to 255; absent while no
   * laser has marked the stock. */
  burn?: Uint8Array;
}

/** An imported toolpath's placement and feeds (`cam.imported.get`). */
export interface ImportedToolpath {
  id: number;
  name: string;
  /** The G-code file it was read from. */
  file: string;
  /** Move from where the file put it, mm. */
  offsetX: number;
  offsetY: number;
  /** How far below where the file put the cuts they now sit, mm. */
  startDepth: number;
  /** mm per minute. */
  feed: number;
  plunge: number;
  /** rpm. */
  spindle: number;
  tool: Tool;
}

/** Changes to an imported toolpath (`cam.imported.edit`); absent fields stay. */
export interface ImportedEdit {
  id: number;
  name?: string;
  offsetX?: number;
  offsetY?: number;
  startDepth?: number;
  feed?: number;
  plunge?: number;
  spindle?: number;
  tool?: Tool;
}
