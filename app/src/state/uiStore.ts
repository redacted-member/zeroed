// Which left dock tab, which view layout and which form is open, and the
// state of the Toolpaths panel on the right.

import { create } from 'zustand';

import type { Tool, ToolKind } from '../bridge/cam';

import { clampDockWidth, clampListHeight } from '../lib/prefs';
import type { ControllerMenuItem } from './drawStore';
import { formDock, isModelingForm } from './formDock';

/** The tabs of the left dock. */
export type WorkspaceTab = 'design' | 'sheets' | 'layers' | 'components' | 'clipart';
export type ViewMode = '2d' | '3d' | 'split';
export type FormId =
  | 'jobSetup'
  | 'documentVariables'
  | 'drawRectangle'
  | 'drawCircle'
  | 'drawEllipse'
  | 'drawPolygon'
  | 'drawStar'
  | 'drawPolyline'
  | 'drawArc'
  | 'dimensions'
  | 'drawFreehand'
  | 'traceBitmap'
  | 'move'
  | 'setSize'
  | 'rotate'
  | 'mirror'
  | 'distort'
  | 'vectorUnwrapper'
  | 'align'
  | 'snapSettings'
  | 'guidePosition'
  | 'join'
  | 'offset'
  | 'fillet'
  | 'trim'
  | 'trimObjects'
  | 'editPicture'
  | 'vectorTexture'
  | 'measure'
  | 'extend'
  | 'fitCurves'
  | 'validator'
  | 'boundary'
  | 'arrayCopy'
  | 'nest'
  | 'plates'
  | 'textSelection'
  | 'circularArray'
  | 'copyAlong'
  | 'createText'
  | 'textOnCurve'
  | 'profileToolpath'
  | 'pocketToolpath'
  | 'drillToolpath'
  | 'drillBankToolpath'
  | 'vgrooveToolpath'
  | 'inlayToolpath'
  | 'mouldingToolpath'
  | 'quickEngraveToolpath'
  | 'chamferToolpath'
  | 'threadMillingToolpath'
  | 'straightInlayToolpath'
  | 'flutingToolpath'
  | 'prismToolpath'
  | 'textureToolpath'
  | 'materialSetup'
  | 'saveToolpaths'
  | 'mergeToolpaths'
  | 'arrayCopyToolpath'
  | 'toolpathTiling'
  | 'photoCarveToolpath'
  | 'importedToolpath'
  | 'previewToolpaths'
  | 'createShape'
  | 'createFlatShape'
  | 'createAngledShape'
  | 'createSmoothShape'
  | 'createCustomShape'
  | 'importModel'
  | 'positionModel'
  | 'segmentModel'
  | 'twoRailSweep'
  | 'turnSpin'
  | 'extrudeWeave'
  | 'textureArea'
  | 'componentEdit'
  | 'componentProperties'
  | 'componentTransform'
  | 'replaceBelow'
  | 'crossSection'
  | 'sliceModel'
  | 'smoothComponents'
  | 'embossComponent'
  | 'scaleModelHeight'
  | 'offsetModel'
  | 'addDraft'
  | 'sculpt'
  | 'gadget'
  | 'exportStl'
  | 'rough3dToolpath'
  | 'finish3dToolpath'
  | 'laserCutFillToolpath'
  | 'laserPictureToolpath'
  | 'sketchToolpath'
  | 'laserSketchToolpath'
  | 'assemblyView'
  | 'jointCreator'
  | 'furnitureDesigner';

export interface ContextMenuState {
  x: number;
  y: number;
  /** The open form's own menu at that point, instead of the view's. */
  items?: ControllerMenuItem[];
}

interface UiState {
  /** A prompt for what the pointer can do where it is, shown in the status bar. */
  statusHint: string | null;
  setStatusHint: (hint: string | null) => void;
  hideEmptyLayers: boolean;
  toggleHideEmptyLayers: () => void;
  contextMenu: ContextMenuState | null;
  openContextMenu: (menu: ContextMenuState) => void;
  closeContextMenu: () => void;
  activeTab: WorkspaceTab;
  viewMode: ViewMode;
  /** The composite model volume the 3D view shows (Ctrl+Shift+Alt+V), in
   * cubic millimetres, with the model stamp it was measured at; null when
   * hidden. */
  /** The volume line of the 3D view; `selected` counts the components
   * measured on their own, 0 for the composite model. */
  modelVolume: {
    volume: number;
    empty: boolean;
    selected: number;
    stamp: number | null;
  } | null;
  setModelVolume: (volume: UiState['modelVolume']) => void;
  /** The one open form; it shows in the left or the right dock (formDock). */
  activeForm: FormId | null;
  /**
   * Counts closeForm calls. The docks key the open form on it, so a form
   * closed and opened again in one step (File > Open of another file while
   * its form shows) starts fresh instead of keeping the old file or sizes.
   */
  formSerial: number;
  /** The toolpath a toolpath form is editing, or null for a new one. */
  editingToolpath: number | null;
  /** The Toolpaths panel stays open when pinned, and hides to a tab when not. */
  toolpathsPinned: boolean;
  /** Whether the unpinned Toolpaths panel is showing. */
  toolpathsOpen: boolean;
  /** Bumped to move keyboard focus into the Toolpaths panel. */
  toolpathsFocus: number;
  toggleToolpathsPinned: () => void;
  setToolpathsOpen: (open: boolean) => void;
  /** Shows the Toolpaths panel and moves focus into it. */
  showToolpaths: () => void;
  /** The left tool panel stays open when pinned, and hides to the dock tabs when not. */
  leftPinned: boolean;
  /** Whether the unpinned left tool panel is showing. */
  leftOpen: boolean;
  toggleLeftPinned: () => void;
  setLeftOpen: (open: boolean) => void;
  /** Widths of the left tool panel and the Toolpaths panel, in CSS pixels. */
  leftWidth: number;
  rightWidth: number;
  setDockWidth: (dock: 'left' | 'right', width: number) => void;
  /** Heights of the toolpath list and the Component Tree; 0 shares the space. */
  toolpathListHeight: number;
  componentTreeHeight: number;
  setListHeight: (list: 'toolpaths' | 'components', height: number) => void;
  /**
   * The design layout (F11): the Design tab pinned open and the Toolpaths
   * panel hidden to its tab.
   */
  designLayout: () => void;
  /**
   * The machining layout (F12): the Toolpaths panel pinned open with focus
   * in it, the left tool panel hidden to its tabs.
   */
  machiningLayout: () => void;
  /** The gadgets popup (Gadgets > Browse Gadgets in the native menu). */
  gadgetsMenuOpen: boolean;
  openGadgetsMenu: () => void;
  closeGadgetsMenu: () => void;
  toolDatabaseOpen: boolean;
  openToolDatabase: () => void;
  closeToolDatabase: () => void;
  /** Set while a toolpath form's Select ... has the Tool Database open to pick its tool. */
  toolPick: ((tool: Tool) => void) | null;
  /** The only tool kinds the picking Tool Database lists, or null for every kind. */
  toolPickKinds: readonly ToolKind[] | null;
  /** Opens the Tool Database with a Select button that hands the picked tool to
   * `onPick`, listing only `kinds` when given. */
  openToolPicker: (onPick: (tool: Tool) => void, kinds?: readonly ToolKind[]) => void;
  /** Switches the left dock tab; a left panel form open on the old tab closes. */
  setActiveTab: (tab: WorkspaceTab) => void;
  setViewMode: (mode: ViewMode) => void;
  openForm: (form: FormId) => void;
  /**
   * Opens Job Setup for a new job with the 2D view up, where the material it
   * sizes is drawn: the 3D view alone gives way; a split layout shows both.
   */
  openNewJobSetup: () => void;
  /** Opens a toolpath form on an existing toolpath. */
  editToolpath: (form: FormId, id: number) => void;
  closeForm: () => void;
}

/** The state change that opens `form`: its dock shows, its tab if needed. */
function opening(state: UiState, form: FormId): Partial<UiState> {
  if (formDock(form) === 'right') {
    return { activeForm: form, toolpathsOpen: true };
  }
  // A modeling form opened from the Components tab (Component Properties)
  // stays on that tab; every other left form shows on the Design tab.
  const stay = state.activeTab === 'components' && isModelingForm(form);
  return {
    activeForm: form,
    activeTab: stay ? state.activeTab : 'design',
    leftOpen: true,
  };
}

export const useUiStore = create<UiState>()((set) => ({
  statusHint: null,
  setStatusHint: (statusHint) => set({ statusHint }),
  hideEmptyLayers: false,
  toggleHideEmptyLayers: () => set((state) => ({ hideEmptyLayers: !state.hideEmptyLayers })),
  contextMenu: null,
  openContextMenu: (contextMenu) => set({ contextMenu }),
  closeContextMenu: () => set({ contextMenu: null }),
  activeTab: 'design',
  viewMode: '2d',
  modelVolume: null,
  setModelVolume: (modelVolume) => set({ modelVolume }),
  activeForm: null,
  formSerial: 0,
  editingToolpath: null,
  toolpathsPinned: true,
  toolpathsOpen: false,
  toolpathsFocus: 0,
  toggleToolpathsPinned: () =>
    set((state) => ({ toolpathsPinned: !state.toolpathsPinned, toolpathsOpen: false })),
  setToolpathsOpen: (toolpathsOpen) => set({ toolpathsOpen }),
  showToolpaths: () =>
    set((state) => ({ toolpathsOpen: true, toolpathsFocus: state.toolpathsFocus + 1 })),
  leftPinned: true,
  leftOpen: false,
  toggleLeftPinned: () => set((state) => ({ leftPinned: !state.leftPinned, leftOpen: false })),
  setLeftOpen: (leftOpen) => set({ leftOpen }),
  leftWidth: 268,
  rightWidth: 300,
  setDockWidth: (dock, width) =>
    set(
      dock === 'left'
        ? { leftWidth: clampDockWidth(width) }
        : { rightWidth: clampDockWidth(width) },
    ),
  toolpathListHeight: 0,
  componentTreeHeight: 0,
  setListHeight: (list, height) =>
    set(
      list === 'toolpaths'
        ? { toolpathListHeight: clampListHeight(height) }
        : { componentTreeHeight: clampListHeight(height) },
    ),
  designLayout: () =>
    set((state) => {
      const leftForm = state.activeForm !== null && formDock(state.activeForm) === 'left';
      const keepForm = leftForm && state.activeTab === 'design';
      return {
        activeTab: 'design',
        activeForm: leftForm && !keepForm ? null : state.activeForm,
        editingToolpath: leftForm && !keepForm ? null : state.editingToolpath,
        leftPinned: true,
        leftOpen: false,
        toolpathsPinned: false,
        toolpathsOpen: false,
      };
    }),
  machiningLayout: () =>
    set((state) => ({
      leftPinned: false,
      leftOpen: false,
      toolpathsPinned: true,
      toolpathsOpen: false,
      toolpathsFocus: state.toolpathsFocus + 1,
    })),
  gadgetsMenuOpen: false,
  openGadgetsMenu: () => set({ gadgetsMenuOpen: true }),
  closeGadgetsMenu: () => set({ gadgetsMenuOpen: false }),
  toolDatabaseOpen: false,
  openToolDatabase: () => set({ toolDatabaseOpen: true, toolPick: null, toolPickKinds: null }),
  closeToolDatabase: () => set({ toolDatabaseOpen: false, toolPick: null, toolPickKinds: null }),
  toolPick: null,
  toolPickKinds: null,
  openToolPicker: (onPick, kinds) =>
    set({ toolDatabaseOpen: true, toolPick: onPick, toolPickKinds: kinds ?? null }),
  setActiveTab: (activeTab) =>
    set((state) => {
      const leftForm = state.activeForm !== null && formDock(state.activeForm) === 'left';
      return leftForm && activeTab !== state.activeTab
        ? { activeTab, activeForm: null, editingToolpath: null }
        : { activeTab };
    }),
  setViewMode: (viewMode) => set({ viewMode }),
  openForm: (form) =>
    set((state) => ({
      ...opening(state, form),
      editingToolpath: null,
    })),
  openNewJobSetup: () =>
    set((state) => ({
      ...opening(state, 'jobSetup'),
      editingToolpath: null,
      viewMode: state.viewMode === '3d' ? '2d' : state.viewMode,
    })),
  editToolpath: (form, editingToolpath) =>
    set((state) => ({ ...opening(state, form), editingToolpath })),
  // An auto hidden tool panel hides again once its form is done.
  closeForm: () =>
    set((state) => ({
      activeForm: null,
      editingToolpath: null,
      leftOpen: false,
      formSerial: state.formSerial + 1,
    })),
}));
