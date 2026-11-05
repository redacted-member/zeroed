// 2D camera, view options, guides, snapping, the active tool and the cursor
// position the status bar reads.

import { create } from 'zustand';

import type { Bbox, Guide, ProbeHit, SnapOptions } from '../bridge/document';
import type { JobSetup } from '../bridge/types';
import {
  fitCamera,
  jobBounds,
  type Bounds2,
  type Camera2D,
  type Viewport,
} from '../views/View2D/camera';

export interface CursorPosition {
  x: number;
  y: number;
}

export type ToolId = 'select' | 'node' | 'zoomBox' | 'pan' | 'create';

export const DEFAULT_SNAP: SnapOptions = {
  grid: false,
  guides: true,
  nodes: true,
  midpoints: true,
  centres: true,
  intersections: true,
  jobPoints: true,
  smart: true,
};

interface ViewState {
  camera: Camera2D;
  viewport: Viewport;
  cursor: CursorPosition | null;
  showGrid: boolean;
  showRulers: boolean;
  /** Whether guide lines show (View > Guide Lines > Guides Visible). */
  showGuides: boolean;
  toggleGuides: () => void;
  /** Whether placed bitmaps show in the 2D view (view toolbar). */
  showBitmaps: boolean;
  toggleBitmaps: () => void;
  /** Whether the shaded component preview shows under the 2D view's vectors. */
  showComponentPreview: boolean;
  toggleComponentPreview: () => void;
  guides: Guide[];
  nextGuideId: number;
  snap: SnapOptions;
  snapRadiusPx: number;
  gridSpacing: number;
  /** Drags move in round steps that follow the zoom (Snap Distances). */
  snapDistances: boolean;
  /** Ctrl+Shift+arrow nudge, millimetres. */
  nudgeDistance: number;
  setSnapDistances: (on: boolean) => void;
  setNudgeDistance: (mm: number) => void;
  activeTool: ToolId;
  /**
   * Interactive Selection mode: picks show the transform handles at once.
   * Off in Vector Selection mode, where a second click brings them up.
   */
  transformMode: boolean;
  snapHit: ProbeHit | null;
  hoverId: number | null;
  /** Guide being edited in the Guide Position form. */
  editingGuideId: number | null;

  setCamera: (camera: Camera2D) => void;
  setViewport: (viewport: Viewport) => void;
  setCursor: (cursor: CursorPosition | null) => void;
  toggleGrid: () => void;
  toggleRulers: () => void;
  zoomToJob: (job: JobSetup) => void;
  zoomToBounds: (bounds: Bounds2 | Bbox | null) => void;
  addGuide: (axis: 'x' | 'y', position: number) => number;
  moveGuide: (id: number, position: number) => void;
  removeGuide: (id: number) => void;
  setGuideLocked: (id: number, locked: boolean) => void;
  clearGuides: () => void;
  /** Puts in the guides of a job just opened, replacing any shown. */
  loadGuides: (guides: Guide[]) => void;
  setSnap: (patch: Partial<SnapOptions>) => void;
  setSnapRadiusPx: (px: number) => void;
  setGridSpacing: (mm: number) => void;
  setActiveTool: (tool: ToolId) => void;
  /** Switches to the selection tool in Interactive Selection mode. */
  enterTransformMode: () => void;
  /** Leaves Interactive Selection mode for Vector Selection mode. */
  exitTransformMode: () => void;
  setSnapHit: (hit: ProbeHit | null) => void;
  setHoverId: (id: number | null) => void;
  setEditingGuide: (id: number | null) => void;
}

export const INITIAL_CAMERA: Camera2D = { centreX: 150, centreY: 100, pxPerMm: 2 };

function toBounds(bounds: Bounds2 | Bbox): Bounds2 {
  return Array.isArray(bounds)
    ? { minX: bounds[0], minY: bounds[1], maxX: bounds[2], maxY: bounds[3] }
    : bounds;
}

export const useViewStore = create<ViewState>()((set, get) => ({
  camera: INITIAL_CAMERA,
  viewport: { width: 0, height: 0 },
  cursor: null,
  showGrid: true,
  showRulers: true,
  showGuides: true,
  showBitmaps: true,
  showComponentPreview: true,
  guides: [],
  nextGuideId: 1,
  snap: DEFAULT_SNAP,
  snapRadiusPx: 10,
  gridSpacing: 10,
  snapDistances: false,
  nudgeDistance: 5,
  setSnapDistances: (snapDistances) => set({ snapDistances }),
  setNudgeDistance: (nudgeDistance) => set({ nudgeDistance }),
  activeTool: 'select',
  transformMode: false,
  snapHit: null,
  hoverId: null,
  editingGuideId: null,

  setCamera: (camera) => set({ camera }),
  setViewport: (viewport) => set({ viewport }),
  setCursor: (cursor) => set({ cursor }),
  toggleGrid: () => set((state) => ({ showGrid: !state.showGrid })),
  toggleRulers: () => set((state) => ({ showRulers: !state.showRulers })),
  toggleGuides: () => set((state) => ({ showGuides: !state.showGuides })),
  toggleBitmaps: () => set((state) => ({ showBitmaps: !state.showBitmaps })),
  toggleComponentPreview: () =>
    set((state) => ({ showComponentPreview: !state.showComponentPreview })),
  zoomToJob: (job) => {
    get().zoomToBounds(jobBounds(job));
  },
  zoomToBounds: (bounds) => {
    const { viewport } = get();
    if (bounds === null || viewport.width === 0 || viewport.height === 0) {
      return;
    }
    set({ camera: fitCamera(toBounds(bounds), viewport) });
  },
  addGuide: (axis, position) => {
    const id = get().nextGuideId;
    set((state) => ({
      guides: [...state.guides, { id, axis, position, locked: false }],
      nextGuideId: id + 1,
    }));
    return id;
  },
  moveGuide: (id, position) =>
    set((state) => ({
      guides: state.guides.map((g) => (g.id === id && !g.locked ? { ...g, position } : g)),
    })),
  removeGuide: (id) => set((state) => ({ guides: state.guides.filter((g) => g.id !== id) })),
  setGuideLocked: (id, locked) =>
    set((state) => ({ guides: state.guides.map((g) => (g.id === id ? { ...g, locked } : g)) })),
  clearGuides: () => set({ guides: [] }),
  loadGuides: (guides) =>
    set((state) => ({
      guides,
      nextGuideId: Math.max(state.nextGuideId, ...guides.map((g) => g.id + 1)),
      editingGuideId: null,
    })),
  setSnap: (patch) => set((state) => ({ snap: { ...state.snap, ...patch } })),
  setSnapRadiusPx: (snapRadiusPx) => set({ snapRadiusPx }),
  setGridSpacing: (gridSpacing) => set({ gridSpacing }),
  setActiveTool: (activeTool) => set({ activeTool, transformMode: false }),
  enterTransformMode: () => set({ activeTool: 'select', transformMode: true }),
  exitTransformMode: () => set({ transformMode: false }),
  setSnapHit: (snapHit) => set({ snapHit }),
  setHoverId: (hoverId) => set({ hoverId }),
  setEditingGuide: (editingGuideId) => set({ editingGuideId }),
}));
