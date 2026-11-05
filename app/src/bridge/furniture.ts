// JSON shapes of the Furniture Designer commands (furniture.*): parametric
// furniture templates turned into panels, joints, toolpaths and Assembly
// View pieces. Lengths are millimetres in furniture space: X width (left to
// right), Y depth (front at Y = 0), Z height (floor at Z = 0).
// The kernel's furniture module is the contract.

import type { JointKind, JointParams } from './joints';
import type { Revision } from './document';

/** A point or direction in furniture space, mm. */
export type Vec3 = [number, number, number];

export type ParamKind = 'length' | 'thickness' | 'count' | 'number' | 'angle' | 'toggle' | 'choice';

/** One parameter a template takes. Lengths and thicknesses are mm, angles degrees. */
export interface FurnitureParam {
  key: string;
  label: string;
  kind: ParamKind;
  /** A number for every kind but choice, whose default is one of its options. */
  default: number | string;
  min?: number;
  max?: number;
  /** The options of a choice. */
  options?: string[];
  /** The heading the parameter shows under. */
  group?: string;
}

/** A stock sheet thickness from the hardware data. */
export interface StockThickness {
  id: string;
  label: string;
  /** The size it is sold as, mm. */
  nominal: number;
  /** What it usually measures, mm. */
  actual: number;
  system: 'inch' | 'metric';
}

/** A stock sheet size from the hardware data, mm. */
export interface StockSize {
  id: string;
  label: string;
  width: number;
  height: number;
  system: 'inch' | 'metric';
}

/** The value of each parameter by key: numbers, or a choice's option. */
export type FurnitureValues = Record<string, number | string>;

export type FurnitureCategory =
  | 'Storage'
  | 'Tables'
  | 'Seating'
  | 'Beds'
  | 'Desks'
  | 'Kitchen'
  | 'Outdoor'
  | 'Shop'
  | 'Kids'
  | 'Decor';

/** A template as the library lists it. */
export interface TemplateSummary {
  id: string;
  name: string;
  category: string;
  description: string;
  /** SPDX id: CC0-1.0, CC-BY-4.0, CC-BY-SA-4.0, or PD. */
  license: string;
  author: string;
  source: string | null;
  credit: string | null;
  tags: string[];
  builtIn: boolean;
  params: FurnitureParam[];
}

export interface JointClass {
  label: string;
  default: FurnitureJoint;
}

/** The full template (format 1). Expressions are strings the kernel evaluates. */
export interface FurnitureTemplate {
  format: 1;
  id: string;
  name: string;
  category: string;
  description: string;
  license: string;
  author: string;
  source: string | null;
  credit: string | null;
  tags: string[];
  params: FurnitureParam[];
  derived?: Record<string, string>;
  materials: Record<string, { label: string; thickness: string }>;
  panels: unknown[];
  connections?: Array<{ a: string; b: string; class: string }>;
  jointClasses?: Record<string, JointClass>;
  hardware?: Array<{ name: string; count: string; note?: string }>;
}

/** The joints the furniture work adds to the Joint Creator's, plus a plain butt joint. */
export type FurnitureJoint = JointKind | 'none' | 'stoppedDado';

/** How two panels meet. */
export type ConnectionType = 'tee' | 'corner' | 'cross' | 'edge' | 'faceOnEdges';

/** A joinery style for the whole design: maps each connection type to a joint. */
export type JoineryStyle =
  'template' | 'traditional' | 'flatPack' | 'quickBuild' | 'laser' | 'glueScrews';

export const JOINERY_STYLES: readonly JoineryStyle[] = [
  'template',
  'traditional',
  'flatPack',
  'quickBuild',
  'laser',
  'glueScrews',
];

/** A joint picked for a class or a connection, with the sizes typed over its defaults. */
export interface JointChoice {
  joint: FurnitureJoint;
  params?: JointParams;
}

/** The joint choices of a design: the style, then overrides per class and per connection. */
export interface FurnitureJoints {
  style: JoineryStyle;
  classes: Record<string, JointChoice>;
  connections: Record<string, JointChoice>;
}

/** Where a connection's joint comes from. */
export type JointSource = 'style' | 'class' | 'connection';

/** A connection between two panels and the joint on it. */
export interface ConnectionRow {
  /** `<panel id>[#i]~<panel id>[#j]`, ids sorted, e.g. `left~shelf#2`. */
  key: string;
  a: string;
  b: string;
  type: ConnectionType;
  class: string;
  /** The class's label from the template ("Shelves to sides"). */
  classLabel?: string;
  joint: FurnitureJoint;
  params: JointParams;
  /** The sizes the joint was made with, or null when it could not be made. */
  resolved?: JointParams | null;
  source: JointSource;
  /** False when the joint asked for could not be made (an issue says why). */
  made?: boolean;
  /** The joint kinds valid here. */
  choices: FurnitureJoint[];
  /** The seam, furniture space mm (add the design's origin for the Assembly View). */
  seam: { from: Vec3; to: Vec3 };
}

/** An axis aligned box in furniture space, mm. */
export interface FurnitureBox {
  min: Vec3;
  max: Vec3;
}

export interface PanelMesh {
  positions: number[];
  indices: number[];
  normals: number[];
}

export interface PreviewPanel {
  key: string;
  name: string;
  material: string;
  thickness?: number;
  mesh: PanelMesh;
  box: FurnitureBox;
}

/** Something the generator wants the maker to know, in plain words. */
export interface FurnitureIssue {
  /** What is wrong and what to do. */
  message: string;
  /** The panel it is about. */
  panel?: string;
  /** The connection it is about. */
  connection?: string;
  /** A joint that would work instead. */
  suggest?: string;
}

/** A design shown without touching the document. */
export interface FurniturePreview {
  panels: PreviewPanel[];
  connections: ConnectionRow[];
  /** What cannot be made as asked. */
  issues: FurnitureIssue[];
  /** Where furniture space's origin sits in the Assembly View, mm. */
  origin?: Vec3;
  /** The overall width, depth and height, mm. */
  size?: Vec3;
}

/** What a design made: counts and the problems the maker should know about. */
export interface DesignSummary {
  id: number;
  name: string;
  template: string;
  style: JoineryStyle;
  panels: number;
  connections: number;
  /** The sheets made per material. */
  sheets: Array<{ material: string; label: string; thickness: number; count: number }>;
  size: Vec3;
  issues: FurnitureIssue[];
}

/** What a panel made in the document: its vector, sheet and Assembly View piece. */
export interface MadePanel {
  vector: number;
  sheet: number;
  /** Its part number on the sheet: the Assembly View piece is `sheet:part`. */
  piece: number;
  /** The panel's name, such as "Door left". */
  name?: string;
}

/** A design in the document. */
export interface DesignRow {
  id: number;
  name: string;
  /** The template's id (the design keeps a full copy: furniture.template does not have it). */
  template: string;
  values: FurnitureValues;
  style: JoineryStyle;
  /** Where furniture space's origin sits in the Assembly View, mm. */
  origin?: Vec3;
  classJoints?: Record<string, JointChoice>;
  connectionJoints?: Record<string, JointChoice>;
  /** Each panel key's made objects. */
  made?: Record<string, MadePanel>;
  /** How many models are on its panels' faces. */
  models?: number;
  /** How many carving toolpaths Make Carving Toolpaths made for them. */
  carving?: number;
}

export interface CreateArgs {
  template: string;
  values: FurnitureValues;
  joints?: FurnitureJoints;
  name?: string;
}

export interface UpdateArgs {
  design: number;
  values?: FurnitureValues;
  joints?: FurnitureJoints;
  style?: JoineryStyle;
  name?: string;
}

export type SetJointScope = 'connection' | 'class' | 'style';

export interface SetJointArgs {
  design: number;
  scope: SetJointScope;
  /** The connection key, the class name, or the style. */
  key: string;
  /** Null drops the override. */
  joint: FurnitureJoint | null;
  params?: JointParams;
}

export interface CutListRow {
  /** The names of the panels this row counts. */
  names: string[];
  /** The keys of the panels this row counts. */
  panels: string[];
  quantity: number;
  /** The finished outline's longer side, mm. */
  length: number;
  width: number;
  thickness: number;
  /** The material's key and its label. */
  material: string;
  materialLabel: string;
  grain: 'long' | 'short' | 'none';
  /** The edges that get edge banding. */
  banding: string[];
  /** Banding per panel, mm. */
  bandingLength: number;
}

export interface HardwareRow {
  name: string;
  count: number;
  note: string;
}

export interface MaterialEstimate {
  material: string;
  label: string;
  thickness: number;
  /** Sheets of this material. */
  count: number;
  /** Area of those sheets, mm². */
  area: number;
  /** Area the panels use, mm². */
  used: number;
  /** Share of the sheets that is waste, 0 to 1. */
  waste: number;
}

export interface FurnitureEstimate {
  sheets: MaterialEstimate[];
  /** Cutting time from the toolpaths, seconds. */
  seconds: number;
  /** How many toolpaths the time counts. */
  toolpaths: number;
}

export type DesignReply = Revision & { design: number; summary: DesignSummary };

/** A panel's broad face: the one up when the panel lies on its sheet, or the one down. */
export type PanelFace = 'top' | 'bottom';

/** What goes on a face: a clip from the Clipart tab, a component, or a model file. */
export type ModelSource = { clipart: string } | { component: number } | { file: string };

export interface AddModelArgs {
  design: number;
  panel: string;
  face: PanelFace;
  source: ModelSource;
  /** Where the pointer was: mm from the panel's lower left on its sheet. */
  point: [number, number];
  /** Sized to fit inside the panel less a margin (a share of its smaller side). */
  fit: { margin: number };
}

/** A component or vector the maker put on a panel, kept with it on a regenerate. */
export interface AttachmentRow {
  panel: string;
  kind: 'component' | 'vector';
  /** The component or vector. */
  id: number;
  /** As the panel lies on its sheet. */
  face: PanelFace;
  /** The face in furniture space: left, right, front, back, bottom or top. */
  faceWord: string;
  /** Its middle as a share of the panel along u and v. */
  at: [number, number];
  /** Stretched with the panel when the design changes size, else kept its size. */
  stretch: boolean;
  /** Carved from the job's other side. */
  otherSide: boolean;
}

/** The furniture panel under a 2D point of a sheet, or nulls off a panel. */
export interface PanelUnder {
  design: number | null;
  /** The panel's key. */
  panel: string | null;
  /** Such as "Door 1". */
  name: string | null;
  /** The faces that lie up and down there, in furniture words (front, left...). */
  top: string | null;
  bottom: string | null;
}
