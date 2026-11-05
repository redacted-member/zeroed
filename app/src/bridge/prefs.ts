// JSON shapes of the kernel's preferences (prefs.get, prefs.set,
// prefs.reset). Field names follow crates/carve-core/src/prefs.rs.

import type { Units } from './types';
import type { TextAnchor } from './document';

export interface SnapDefaults {
  grid: boolean;
  guides: boolean;
  nodes: boolean;
  midpoints: boolean;
  centres: boolean;
  intersections: boolean;
  jobPoints: boolean;
  smart: boolean;
}

export interface GeneralPrefs {
  defaultUnits: Units;
  /** Steps Undo keeps, 1 to 1000. */
  undoLimit: number;
  /** Minutes between autosaves, 0 for never. */
  autosaveMinutes: number;
  showToolTips: boolean;
  /** The status bar says what the pointer can do in the views. */
  showUsageInfo: boolean;
  /** Length of the Open Recent list, 0 to 20. */
  recentFilesLength: number;
  /** Newest first. */
  recentFiles: string[];
  /** Font families picked lately, newest first, at most five. */
  recentFonts: string[];
  /** Sculpting reads a graphics tablet pen's pressure. */
  graphicsTablet: boolean;
  /** Joining bridges gaps with a curve tangent to both ends. */
  smoothJoin: boolean;
  /** The smooth Join / Close tool's method: smoother or symmetrical shallower. */
  smoothJoinMethod: 'smoother' | 'shallow';
  /** Draw Line starts in the curve mode last used. */
  lineCurveMode: boolean;
  /** New text starts with the anchor last used, not the bottom left. */
  lastTextAnchor: boolean;
  /** The anchor last used for new text. */
  textAnchor: TextAnchor;
  /** The Design tab shows the extra File Operations row (New, Save, Undo, Redo). */
  showFileOperations: boolean;
  /** The Design tab shows the Align Objects group. */
  showAlignTools: boolean;
  /** The getting started guide opens when the app starts. */
  showGuide: boolean;
}

export type { TextAnchor };

/** What a calculation does when its vectors cross ('proceed' only for sharp offsets). */
export type IntersectionCheck = 'ask' | 'validator' | 'skip' | 'proceed';

export interface View2dPrefs {
  snap: SnapDefaults;
  snapRadiusPx: number;
  showGrid: boolean;
  showRulers: boolean;
  /** Millimetres. */
  gridSpacing: number;
  /** Drags move in round steps that follow the zoom (Snap Distances). */
  snapDistances: boolean;
  /** Ctrl+Shift+arrow nudge, millimetres. */
  nudgeDistance: number;
  /** Points at the grid spacing (plain points) or zoom adaptive lines. */
  gridStyle: GridStyle;
}

export type GridStyle = 'points' | 'lines';

export type Background3d = 'theme' | 'gradient' | 'solid';
export type Shading = 'smooth' | 'flat';
export type Projection = 'perspective' | 'orthographic';
export type Navigation3d = 'modern' | 'legacy';
export type RenderMode = 'shaded' | 'wireframe' | 'shadedWire';
export type Material3d =
  'theme' | 'pine' | 'oak' | 'walnut' | 'maple' | 'mdf' | 'acrylic' | 'aluminium' | 'solid';
export type TileDirection = 'vertical' | 'horizontal';

export interface View3dPrefs {
  background: Background3d;
  /** `#rrggbb`. */
  topColour: string;
  bottomColour: string;
  shading: Shading;
  animateCamera: boolean;
  projection: Projection;
  navigation: Navigation3d;
  renderMode: RenderMode;
  material: Material3d;
  /** `#rrggbb`, used when `material` is `solid`. */
  solidColour: string;
  showModel: boolean;
  showBlock: boolean;
  showToolpaths: boolean;
  showVectors: boolean;
  showCube: boolean;
  /** Arrows at the job origin (View > Draw Origin). */
  drawOrigin: boolean;
  /** A see through plane at the model base (View > Draw Modeling Plane). */
  drawModelingPlane: boolean;
  /** Both sides of a double sided job at once (View > Multi Sided View). */
  multiSided: boolean;
  /** A rotary job drawn wrapped round its axis (View > Auto-Wrapping View). */
  autoWrap: boolean;
  /** Material and component colours; plain grey when off (View > Color Shaded View). */
  colorShaded: boolean;
  /** The key light comes from the camera (View > Light follows viewer). */
  lightFollows: boolean;
  /** Deeper shadows that bring out detail (View > Use Shadow Shading). */
  shadowShading: boolean;
  /** The user's own material textures; absent in old preferences. */
  textures?: MaterialTexture[];
}

/** A picture added to the material list. */
export interface MaterialTexture {
  /** Above zero; a job's appearance refers to it by this. */
  id: number;
  name: string;
  /** Group it is listed under. */
  category: string;
  /** The picture file. */
  path: string;
}

export interface ToolpathPrefs {
  /** 2D toolpath filter tolerance, millimetres. */
  tolerance: number;
  /** 3D and projected toolpath filter tolerance, millimetres. */
  tolerance3d: number;
  /** V-groove toolpath filter tolerance, millimetres. */
  vgrooveTolerance: number;
  /** Projecting onto the model drops the whole tool onto the surface. */
  dropTool: boolean;
  /** Raster clearing cuts each separate strip in turn. */
  newRaster: boolean;
  /** Seconds a calculation spends repairing vectors that cross themselves; 0 repairs none. */
  geometryFixSeconds: number;
  /** After a job size change, hidden toolpaths have their depth checked too. */
  checkAllDepths: boolean;
  /** A duplicated toolpath goes to the end of the list, not beside the original. */
  appendDuplicated: boolean;
  /** New toolpaths go to the end of the selected toolpath group. */
  appendToSelectedGroup: boolean;
  /** The sheet filter the toolpath list starts with. */
  defaultFilter: 'activeSheet' | 'allSheets';
  /** Editing a toolpath of another sheet makes that sheet active. */
  selectSheetOnEdit: boolean;
  askBeforeRecalculating: boolean;
  cutColour: string;
  rapidColour: string;
  /** Colour a solid 2D preview fills the cut width with. */
  solidPreviewColour: string;
  /** The toolpath operations stay above the Preview Toolpaths form. */
  showOperationsWithPreview: boolean;
  /** Material Setup's Show detailed summary on toolpath tab. */
  detailedMaterialSummary: boolean;
  /** The output choice Save Toolpaths opens with, the last one saved with. */
  saveOutput: 'selected' | 'oneFile' | 'multipleFiles';
  /** Save Toolpaths' M6 macro applies tool length (off: G43 H after M6). */
  m6AppliesLength: boolean;
  /** Toolpaths drawn in the 2D view. */
  draw2dPreviews: boolean;
  /** The 2D view fills the width the tool cuts. */
  solidPreviews: boolean;
  /** The 3D view opens after a new toolpath is calculated. */
  autoOpen3d: boolean;
  drawRapids: boolean;
  drawPlunges: boolean;
  drawRetracts: boolean;
  drawJoins: boolean;
  /** Machining time estimates are multiplied by this, 0.1 to 10. */
  estimateScale: number;
}

export interface ConfirmationPrefs {
  deleteLayer: boolean;
  deleteSheet: boolean;
  deleteTool: boolean;
  /** V-Groove and Pocket vectors that cross. */
  toolpathIntersections: IntersectionCheck;
  radiusedOffsetIntersections: IntersectionCheck;
  sharpOffsetIntersections: IntersectionCheck;
  embossWarning: boolean;
}

/**
 * The left dock tab. 'drawing', 'modeling', 'toolpaths' and 'assembly' only
 * appear in older saved layouts.
 */
export type WindowTab =
  | 'design'
  | 'sheets'
  | 'layers'
  | 'components'
  | 'clipart'
  | 'drawing'
  | 'modeling'
  | 'toolpaths'
  | 'assembly';
/** 'assembly' only appears in older saved layouts. */
export type WindowViewMode = '2d' | '3d' | 'split' | 'assembly';

export interface WindowPrefs {
  remember: boolean;
  activeTab: WindowTab;
  viewMode: WindowViewMode;
  jobSetupAtStart: boolean;
  /** Vertical tiles sit side by side, horizontal ones one above the other. */
  tile: TileDirection;
  /** The left tool panel stays open when pinned and hides to its tabs when not. */
  leftPinned: boolean;
  /** The Toolpaths panel stays open when pinned and hides to its tab when not. */
  toolpathsPinned: boolean;
  /** Width of the left tool panel in CSS pixels. */
  leftWidth: number;
  /** Width of the Toolpaths panel in CSS pixels. */
  rightWidth: number;
  /** Height of the toolpath list, CSS pixels; 0 shares the panel. */
  toolpathListHeight: number;
  /** Height of the Component Tree, CSS pixels; 0 shares the tab. */
  componentTreeHeight: number;
  /** Size the font drop-down list was dragged to, CSS pixels; 0 for the usual size. */
  fontListWidth: number;
  fontListHeight: number;
  /** Ruler tint while a double sided job shows its top side. */
  topRulerColour: string;
  /** Ruler tint while it shows its bottom side. */
  bottomRulerColour: string;
}

/** One machine of Machine Configuration; lengths in millimetres. */
export interface MachineConfig {
  name: string;
  units: Units;
  bedX: number;
  bedY: number;
  bedZ: number;
  /** Ids of the post processors that drive the machine. */
  posts: string[];
  /** The post Save Toolpaths picks first; one of `posts`. */
  defaultPost?: string;
  /** Rapid rate for time estimates, mm/min; absent in old preferences (3000). */
  rapidRate?: number;
  /** X, Y, Z accelerations for time estimates, mm/s2; zero when not known. */
  acceleration?: [number, number, number];
  /** Rotary axis acceleration for time estimates, degrees/s2; zero when not known. */
  rotaryAcceleration?: number;
  /** How the rotary axis is wired; absent in old preferences (A in degrees). */
  rotary?: RotaryWiring;
  /** Unit the dialog shows the accelerations in. */
  accelerationUnits?: string;
}

/** How a machine's rotary axis is wired. */
export interface RotaryWiring {
  /** The letter the controller calls the axis. */
  letter?: 'A' | 'B' | null;
  /** What the controller reads the word as: degrees, or the distance round the blank. */
  units?: 'degrees' | 'linear';
  /** The axis turns the other way: a design would come out mirrored. */
  reversed?: boolean;
}

/** A machine's wiring with every field set: A in degrees until someone says otherwise. */
export function rotaryWiring(machine: MachineConfig | undefined): Required<RotaryWiring> {
  return {
    letter: machine?.rotary?.letter ?? 'A',
    units: machine?.rotary?.units ?? 'degrees',
    reversed: machine?.rotary?.reversed ?? false,
  };
}

export interface MachinePrefs {
  /** Name of the active machine, empty only when there are none. */
  active: string;
  list: MachineConfig[];
}

export interface Preferences {
  version: number;
  general: GeneralPrefs;
  view2d: View2dPrefs;
  view3d: View3dPrefs;
  toolpaths: ToolpathPrefs;
  confirmations: ConfirmationPrefs;
  window: WindowPrefs;
  machines: MachinePrefs;
  /** Absent from older kernels. */
  modeling?: ModelingPrefs;
}

/** One saved set of Sculpt form settings. */
export interface SculptPreset {
  name: string;
  /** The sculpting tool it is for. */
  tool: string;
  settings: Record<string, unknown>;
}

export interface ModelingPrefs {
  sculptPresets: SculptPreset[];
}

/** What prefs.get, prefs.set and prefs.reset answer. */
export interface PrefsReply extends Preferences {
  /** The file the preferences are saved in, null when they are not saved. */
  path: string | null;
  /** Where a broken preferences file was moved at start, if one was. */
  setAside: string | null;
}

/** A partial update: any group, any subset of its fields. */
export type PrefsPatch = {
  [G in Exclude<keyof Preferences, 'version'>]?: G extends 'view2d'
    ? Partial<Omit<View2dPrefs, 'snap'>> & { snap?: Partial<SnapDefaults> }
    : Partial<Preferences[G]>;
};
