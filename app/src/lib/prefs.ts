// Pure helpers for preferences: the defaults (mirroring the kernel's), patch
// merging, the recent files list and the CSS custom properties that carry
// user colours to the views.

import type { Preferences, PrefsPatch, WindowTab, WindowViewMode } from '../bridge/prefs';
import type { ViewMode, WorkspaceTab } from '../state/uiStore';

export const MAX_UNDO_LIMIT = 1000;
export const MAX_RECENT_FILES = 20;
export const MAX_AUTOSAVE_MINUTES = 120;
/** Longest Toolpath Geometry Fixing Timeout, in seconds (the kernel's limit). */
export const MAX_GEOMETRY_FIX_SECONDS = 3600;
/** Narrowest and widest a dock panel can be dragged, in CSS pixels. */
export const MIN_DOCK_WIDTH = 200;
export const MAX_DOCK_WIDTH = 640;

/** Shortest a dock list can be dragged, and the tallest, in CSS pixels. */
export const MIN_LIST_HEIGHT = 80;
export const MAX_LIST_HEIGHT = 4000;

/** A dock list height rounded and kept in range; 0 (share the space) stays 0. */
export function clampListHeight(height: number): number {
  if (!Number.isFinite(height) || height <= 0) {
    return 0;
  }
  return Math.min(MAX_LIST_HEIGHT, Math.max(MIN_LIST_HEIGHT, Math.round(height)));
}

/** `width` rounded and kept inside the dock width range. */
export function clampDockWidth(width: number): number {
  if (!Number.isFinite(width)) {
    return MIN_DOCK_WIDTH;
  }
  return Math.min(MAX_DOCK_WIDTH, Math.max(MIN_DOCK_WIDTH, Math.round(width)));
}

/** The kernel's defaults, used before prefs.get answers and by the mock. */
export const DEFAULT_PREFS: Preferences = {
  version: 1,
  general: {
    defaultUnits: 'mm',
    undoLimit: 200,
    autosaveMinutes: 0,
    showToolTips: true,
    showUsageInfo: true,
    recentFilesLength: 10,
    recentFiles: [],
    recentFonts: [],
    graphicsTablet: false,
    smoothJoin: false,
    smoothJoinMethod: 'smoother',
    lineCurveMode: false,
    lastTextAnchor: false,
    textAnchor: 'bottomLeft',
    showFileOperations: false,
    showAlignTools: false,
    showGuide: true,
  },
  view2d: {
    snap: {
      grid: false,
      guides: true,
      nodes: true,
      midpoints: true,
      centres: true,
      intersections: true,
      jobPoints: true,
      smart: true,
    },
    snapRadiusPx: 10,
    showGrid: true,
    showRulers: true,
    gridSpacing: 10,
    snapDistances: false,
    nudgeDistance: 5,
    gridStyle: 'points',
  },
  view3d: {
    background: 'theme',
    topColour: '#4c525a',
    bottomColour: '#23262b',
    shading: 'smooth',
    animateCamera: true,
    projection: 'perspective',
    navigation: 'modern',
    renderMode: 'shaded',
    material: 'theme',
    solidColour: '#c9b28f',
    showModel: true,
    showBlock: true,
    showToolpaths: true,
    showVectors: false,
    showCube: true,
    drawOrigin: false,
    drawModelingPlane: false,
    multiSided: false,
    autoWrap: true,
    colorShaded: true,
    lightFollows: false,
    shadowShading: false,
    textures: [],
  },
  toolpaths: {
    tolerance: 0.025,
    tolerance3d: 0.01,
    vgrooveTolerance: 0.025,
    dropTool: false,
    newRaster: true,
    geometryFixSeconds: 10,
    checkAllDepths: false,
    appendDuplicated: false,
    appendToSelectedGroup: true,
    defaultFilter: 'activeSheet',
    selectSheetOnEdit: true,
    askBeforeRecalculating: true,
    cutColour: '#1d4ed8',
    rapidColour: '#d62828',
    solidPreviewColour: '#5b8def',
    showOperationsWithPreview: false,
    detailedMaterialSummary: false,
    saveOutput: 'selected',
    m6AppliesLength: true,
    draw2dPreviews: true,
    solidPreviews: false,
    autoOpen3d: true,
    drawRapids: true,
    drawPlunges: true,
    drawRetracts: true,
    drawJoins: true,
    estimateScale: 1,
  },
  confirmations: {
    deleteLayer: true,
    deleteSheet: true,
    deleteTool: true,
    toolpathIntersections: 'ask',
    radiusedOffsetIntersections: 'ask',
    sharpOffsetIntersections: 'ask',
    embossWarning: true,
  },
  window: {
    remember: true,
    activeTab: 'design',
    viewMode: '2d',
    jobSetupAtStart: true,
    tile: 'vertical',
    leftPinned: true,
    toolpathsPinned: true,
    leftWidth: 268,
    rightWidth: 300,
    toolpathListHeight: 0,
    componentTreeHeight: 0,
    fontListWidth: 0,
    fontListHeight: 0,
    topRulerColour: '#5b9bd5',
    bottomRulerColour: '#e0a040',
  },
  machines: {
    active: 'Default',
    list: [{ name: 'Default', units: 'mm', bedX: 600, bedY: 600, bedZ: 100, posts: [] }],
  },
  modeling: { sculptPresets: [] },
};

/**
 * The left dock tab a remembered layout opens on. Older layouts may name the
 * Drawing, Modeling or Toolpaths tab, or the Assembly tab from before the
 * Assembly View moved to the Toolpaths panel; those open on the Design tab.
 */
export function dockTab(tab: WindowTab): WorkspaceTab {
  return tab === 'drawing' || tab === 'modeling' || tab === 'toolpaths' || tab === 'assembly'
    ? 'design'
    : tab;
}

/** The view a remembered layout opens on; the old Assembly view opens the 3D view. */
export function windowView(mode: WindowViewMode): ViewMode {
  return mode === 'assembly' ? '3d' : mode;
}

/** A deep copy of the defaults. */
export function defaultPrefs(): Preferences {
  return structuredClone(DEFAULT_PREFS);
}

/** `prefs` with `patch` merged over it group by group. */
export function mergePrefs(prefs: Preferences, patch: PrefsPatch): Preferences {
  const view2d = patch.view2d ?? {};
  return {
    version: prefs.version,
    general: { ...prefs.general, ...patch.general },
    view2d: {
      ...prefs.view2d,
      ...view2d,
      snap: { ...prefs.view2d.snap, ...view2d.snap },
    },
    view3d: { ...prefs.view3d, ...patch.view3d },
    toolpaths: { ...prefs.toolpaths, ...patch.toolpaths },
    confirmations: { ...prefs.confirmations, ...patch.confirmations },
    window: { ...prefs.window, ...patch.window },
    machines: { ...prefs.machines, ...patch.machines },
    modeling: { sculptPresets: [], ...prefs.modeling, ...patch.modeling },
  };
}

/** The recent files list with `path` moved to the front, cut to `max`. */
export function withRecent(list: readonly string[], path: string, max: number): string[] {
  return [path, ...list.filter((p) => p !== path)].slice(0, Math.max(0, max));
}

const COLOUR = /^#[0-9a-f]{6}$/i;

/** True for a `#rrggbb` colour. */
export function isColour(value: string): boolean {
  return COLOUR.test(value);
}

/** Why a set of preferences would be refused, or null when it is fine. */
export function prefsProblem(prefs: Preferences): string | null {
  const { general, view2d, view3d, toolpaths } = prefs;
  const whole = (n: number, lo: number, hi: number) => Number.isInteger(n) && n >= lo && n <= hi;
  if (!whole(general.undoLimit, 1, MAX_UNDO_LIMIT)) {
    return 'general.undoLimit';
  }
  if (!whole(general.autosaveMinutes, 0, MAX_AUTOSAVE_MINUTES)) {
    return 'general.autosaveMinutes';
  }
  if (!whole(general.recentFilesLength, 0, MAX_RECENT_FILES)) {
    return 'general.recentFilesLength';
  }
  if (!whole(view2d.snapRadiusPx, 1, 50)) {
    return 'view2d.snapRadiusPx';
  }
  if (!(
    Number.isFinite(view2d.gridSpacing) &&
    view2d.gridSpacing > 0 &&
    view2d.gridSpacing <= 10000
  )) {
    return 'view2d.gridSpacing';
  }
  if (!(
    Number.isFinite(view2d.nudgeDistance) &&
    view2d.nudgeDistance > 0 &&
    view2d.nudgeDistance <= 1000
  )) {
    return 'view2d.nudgeDistance';
  }
  if (!isColour(view3d.topColour)) {
    return 'view3d.topColour';
  }
  if (!isColour(view3d.bottomColour)) {
    return 'view3d.bottomColour';
  }
  if (!isColour(view3d.solidColour)) {
    return 'view3d.solidColour';
  }
  if (!(
    Number.isFinite(toolpaths.tolerance) &&
    toolpaths.tolerance >= 0.0001 &&
    toolpaths.tolerance <= 1
  )) {
    return 'toolpaths.tolerance';
  }
  if (!whole(toolpaths.geometryFixSeconds, 0, MAX_GEOMETRY_FIX_SECONDS)) {
    return 'toolpaths.geometryFixSeconds';
  }
  if (!isColour(toolpaths.cutColour)) {
    return 'toolpaths.cutColour';
  }
  if (!isColour(toolpaths.rapidColour)) {
    return 'toolpaths.rapidColour';
  }
  if (!isColour(toolpaths.solidPreviewColour)) {
    return 'toolpaths.solidPreviewColour';
  }
  if (!isColour(prefs.window.topRulerColour)) {
    return 'window.topRulerColour';
  }
  if (!isColour(prefs.window.bottomRulerColour)) {
    return 'window.bottomRulerColour';
  }
  return null;
}

/**
 * CSS custom properties the preferences set on the document root. A null
 * value removes the property so the theme token shows through.
 */
export function prefsCssVars(prefs: Preferences): Record<string, string | null> {
  const { view3d, toolpaths } = prefs;
  const top = view3d.background === 'theme' ? null : view3d.topColour;
  const bottom =
    view3d.background === 'theme'
      ? null
      : view3d.background === 'solid'
        ? view3d.topColour
        : view3d.bottomColour;
  return {
    '--view3d-top': top,
    '--view3d-bottom': bottom,
    '--canvas-toolpath-cut': toolpaths.cutColour,
    '--canvas-toolpath-rapid': toolpaths.rapidColour,
    '--canvas-toolpath-solid': toolpaths.solidPreviewColour,
  };
}
