// Canvas2D renderer for the drawing view. Owns no React state and no DOM
// beyond its canvas. Everything it draws comes from the RenderState that the
// view component and the interaction controller keep up to date.

import type { Bbox, Guide, Matrix, Path, ProbeHit, SelectionFrame } from '../../bridge/document';
import { flattenPath } from '../../lib/geometry';
import { fromFrame } from '../../lib/transformMath';
import type { JobSetup } from '../../bridge/types';
import { strings } from '../../lib/strings';
import { legibleOn, type CanvasPalette } from '../../lib/theme';
import type { PreviewLabel, PreviewOverlay } from '../../state/drawStore';
import {
  gridSpacing,
  jobBounds,
  jobToScreen,
  screenToJob,
  type Camera2D,
  type Point2,
  type Viewport,
} from './camera';
import { BitmapLayer } from './bitmapLayer';
import { ComponentPreviewLayer } from './componentPreview';
import { gridPointStep, gridValues } from './gridPoints';
import type { Composite } from '../../bridge/model';
import { rotationCentre, type HandleMode } from './tools/handleModes';
import type { DisplayEntry } from './displayCache';
import type { AlignmentSnap } from './smartSnap';
import type { MoveGroup } from '../../lib/moveGroups';
import { straightRuns, type ToolpathPaths } from './toolpathPaths';

/** Width of the ruler strips in CSS pixels. */
export const RULER_SIZE = 22;
const HANDLE_SIZE = 7;
/** How far past each corner, px, the solid rotate handles sit. */
const ROTATE_HANDLE_GAP = 14;
/** Radius, px, of the rotational anchor's ring. */
const PIVOT_RADIUS = 6;
const SNAP_MARK = 6;
const NODE_SIZE = 6;
/** Opacity of the cut width a solid toolpath preview fills. */
const SOLID_PREVIEW_ALPHA = 0.3;

export interface LayerStyle {
  colour: string;
  visible: boolean;
  /** Closed vectors are shaded as keep-out zones. */
  keepOut?: boolean;
}

export interface RubberBand {
  from: Point2;
  to: Point2;
  mode: 'inside' | 'touch';
}

export interface DragPreview {
  ids: ReadonlySet<number>;
  matrix: Matrix;
  /** Boxes of the selected components the drag carries, drawn with their picture. */
  components?: readonly Bbox[];
}

export interface NodeMarker {
  key: string;
  x: number;
  y: number;
  smooth: boolean;
  start: boolean;
  selected: boolean;
}

export interface HandleMarker {
  key: string;
  x: number;
  y: number;
  anchorX: number;
  anchorY: number;
}

export interface NodeOverlay {
  entityId: number;
  nodes: NodeMarker[];
  handles: HandleMarker[];
}

export type HandleId = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w' | 'rotate' | 'pivot';

export interface ScreenHandle {
  id: HandleId;
  x: number;
  y: number;
}

export interface ViewOptions {
  showGrid: boolean;
  showRulers: boolean;
  /** Placed bitmaps; absent means shown. */
  showBitmaps?: boolean;
  /** The shaded component preview; absent means shown. */
  showComponents?: boolean;
  /** Points at the grid spacing, or zoom adaptive lines (the default). */
  gridStyle?: 'points' | 'lines';
  /** Grid spacing for the point grid, millimetres. */
  gridSpacingMm?: number;
  /** Tint over the rulers (the side of a double sided job), or none. */
  rulerTint?: string | null;
}

export type OverlayPainter = (
  context: CanvasRenderingContext2D,
  toScreen: (p: Point2) => Point2,
  palette: CanvasPalette,
) => void;

export interface RenderState {
  job: JobSetup | null;
  camera: Camera2D;
  palette: CanvasPalette | null;
  options: ViewOptions;
  layers: ReadonlyMap<number, LayerStyle>;
  entries: readonly DisplayEntry[];
  selectionBbox: Bbox | null;
  /** The selection's box as the handles follow it, rotated with a rotated selection. */
  selectionFrame: SelectionFrame | null;
  handleMode: HandleMode;
  /** The rotation center once the rotational anchor was dragged; null is the box middle. */
  rotationCentre: Point2 | null;
  hoverId: number | null;
  guides: readonly Guide[];
  snapHit: ProbeHit | null;
  /** Smart snapping alignment lines in use, drawn dashed. */
  alignment: AlignmentSnap | null;
  rubberBand: RubberBand | null;
  dragPreview: DragPreview | null;
  nodeOverlay: NodeOverlay | null;
  rulerCursor: Point2 | null;
  overlay: OverlayPainter | null;
  /** Dashed preview of what an open creation form would create. */
  previewPaths: readonly Path[];
  /** Coloured paths an open form draws (rails, markers). */
  previewOverlays: readonly PreviewOverlay[];
  /** Texts an open form writes at job points (Nest Parts copies). */
  previewLabels: readonly PreviewLabel[];
  /** The visible toolpaths' moves, drawn over the vectors; null draws none. */
  toolpaths: ToolpathPaths | null;
  /** Toolpath Tiling's tiles while it is on; null draws none. */
  tiles: TilesOverlay | null;
  /** Multi Sided View: the other side's vectors, turned over, drawn faint. */
  otherSide: readonly Path[];
}

/** Tile boxes as [minX, minY, maxX, maxY] in job millimetres. */
export interface TilesOverlay {
  tiles: ReadonlyArray<{ nominal: readonly number[]; area: readonly number[] }>;
  /** Index of the tile the views show. */
  active: number;
}

export function initialRenderState(): RenderState {
  return {
    job: null,
    camera: { centreX: 0, centreY: 0, pxPerMm: 2 },
    palette: null,
    options: { showGrid: true, showRulers: true },
    layers: new Map(),
    entries: [],
    selectionBbox: null,
    selectionFrame: null,
    handleMode: 'none',
    rotationCentre: null,
    hoverId: null,
    guides: [],
    snapHit: null,
    alignment: null,
    rubberBand: null,
    dragPreview: null,
    nodeOverlay: null,
    rulerCursor: null,
    overlay: null,
    previewPaths: [],
    previewOverlays: [],
    previewLabels: [],
    toolpaths: null,
    tiles: null,
    otherSide: [],
  };
}

/**
 * Screen positions of the eight scale handles and the four rotate handles,
 * first the rotational anchor at `pivot` when it is shown.
 */
export function selectionHandles(
  frame: SelectionFrame,
  camera: Camera2D,
  viewport: Viewport,
  pivot: Point2 | null = null,
): ScreenHandle[] {
  const [minX, minY, maxX, maxY] = frame.extent;
  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  const at = (x: number, y: number) => jobToScreen(camera, viewport, fromFrame(frame, { x, y }));
  const centre = at(cx, cy);
  const handle = (id: HandleId, p: Point2): ScreenHandle => ({ id, x: p.x, y: p.y });
  // Solid rotate handles just outside each corner, pointing away from the middle.
  const past = (p: Point2): ScreenHandle => {
    const dx = p.x - centre.x;
    const dy = p.y - centre.y;
    const length = Math.hypot(dx, dy);
    const k = length > 1e-9 ? ROTATE_HANDLE_GAP / length : 0;
    return { id: 'rotate', x: p.x + dx * k, y: p.y + dy * k };
  };
  const nw = at(minX, maxY);
  const ne = at(maxX, maxY);
  const se = at(maxX, minY);
  const sw = at(minX, minY);
  const anchor = pivot === null ? [] : [handle('pivot', jobToScreen(camera, viewport, pivot))];
  return [
    ...anchor,
    handle('nw', nw),
    handle('n', at(cx, maxY)),
    handle('ne', ne),
    handle('e', at(maxX, cy)),
    handle('se', se),
    handle('s', at(cx, minY)),
    handle('sw', sw),
    handle('w', at(minX, cy)),
    past(nw),
    past(ne),
    past(se),
    past(sw),
  ];
}

export function formatTick(value: number): string {
  const rounded = Math.round(value * 1000) / 1000;
  return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
}

/** Opacity of the component picture a drag carries over the model. */
const DRAG_COMPONENT_ALPHA = 0.75;

/** Toolpath segments built into paths, for the moves they came from. */
interface ToolpathStrokes {
  source: ToolpathPaths;
  lines: Record<MoveGroup, Path2D | null>;
  solid: { width: number; path: Path2D | null }[];
}

/** Segments (flat x0, y0, x1, y1) as one path of separate lines; null
 * when there are none. */
function segmentPath(segments: Float32Array): Path2D | null {
  if (segments.length === 0) {
    return null;
  }
  const path = new Path2D();
  for (const [x0, y0, x1, y1] of straightRuns(segments)) {
    path.moveTo(x0, y0);
    path.lineTo(x1, y1);
  }
  return path;
}

export class View2DRenderer {
  private readonly canvas: HTMLCanvasElement;
  private readonly context: CanvasRenderingContext2D;
  private viewport: Viewport = { width: 0, height: 0 };
  private dpr = 1;
  readonly state: RenderState = initialRenderState();
  private readonly bitmaps = new BitmapLayer(() => this.draw());
  private readonly components = new ComponentPreviewLayer(() => this.draw());
  /** The selected components alone, carried along by a drag. */
  private readonly dragComponents = new ComponentPreviewLayer(() => this.draw());
  /** What {@link dragComponents} was made from, see {@link setDragComposite}. */
  dragComponentsKey: string | null = null;
  /** The toolpaths' paths, see {@link toolpathStrokes}. */
  private strokes: ToolpathStrokes | null = null;

  /** Sets the model the component preview shows; null shows none. */
  setComposite(composite: Composite | null): void {
    this.components.set(composite);
  }

  /** Sets the picture of the selected components a drag carries; `key`
   * names what it was made from. */
  setDragComposite(composite: Composite | null, key: string | null): void {
    this.dragComponents.set(composite);
    this.dragComponentsKey = key;
  }

  constructor(canvas: HTMLCanvasElement) {
    const context = canvas.getContext('2d');
    if (context === null) {
      throw new Error('this display cannot provide a 2D canvas context');
    }
    this.canvas = canvas;
    this.context = context;
  }

  update(patch: Partial<RenderState>): void {
    Object.assign(this.state, patch);
  }

  getViewport(): Viewport {
    return this.viewport;
  }

  resize(width: number, height: number, devicePixelRatio: number): void {
    this.viewport = { width, height };
    this.dpr = devicePixelRatio;
    this.canvas.width = Math.max(1, Math.round(width * devicePixelRatio));
    this.canvas.height = Math.max(1, Math.round(height * devicePixelRatio));
  }

  /** Job point under a canvas pixel. */
  pointAt(pixel: Point2): Point2 {
    return screenToJob(this.state.camera, this.viewport, pixel);
  }

  toScreen(point: Point2): Point2 {
    return jobToScreen(this.state.camera, this.viewport, point);
  }

  draw(): void {
    const { context, viewport } = this;
    const { palette, job } = this.state;
    if (palette === null || job === null || viewport.width === 0) {
      return;
    }
    this.screenSpace();
    context.fillStyle = palette.background;
    context.fillRect(0, 0, viewport.width, viewport.height);

    // Lines sit under the material; points show over it.
    const points = this.state.options.gridStyle === 'points';
    if (this.state.options.showGrid && !points) {
      this.drawGrid(palette);
    }
    this.drawJob(palette, job);
    if (this.state.options.showGrid && points) {
      this.drawGrid(palette);
    }
    this.drawDatum(palette);
    this.drawGuides(palette);
    this.jobSpace();
    const { showBitmaps, showComponents } = this.state.options;
    if (showComponents !== false) {
      this.components.draw(this.context);
    }
    if (showBitmaps !== false) {
      this.bitmaps.draw(this.context, this.visibleEntries());
    }
    this.drawTiles(palette);
    this.drawOtherSide(palette);
    this.drawEntities(palette);
    this.drawToolpaths(palette);
    this.drawDragPreview(palette);
    this.drawSelection(palette);
    this.drawNodeOverlay(palette);
    this.drawPreviewPaths(palette);
    this.drawPreviewOverlays(palette);
    this.drawPreviewLabels(palette);
    this.drawRubberBand(palette);
    this.drawAlignment(palette);
    this.drawSnapMarker(palette);
    if (this.state.overlay !== null) {
      this.screenSpace();
      this.state.overlay(context, (p) => this.toScreen(p), palette);
    }
    if (this.state.options.showRulers) {
      this.drawRulers(palette);
    }
  }

  private screenSpace(): void {
    this.context.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
  }

  /** Switches the context to job millimetres (Y up). Returns pixels per mm. */
  private jobSpace(): number {
    const { camera } = this.state;
    const s = camera.pxPerMm;
    const { width, height } = this.viewport;
    this.context.setTransform(
      this.dpr * s,
      0,
      0,
      -this.dpr * s,
      this.dpr * (width / 2 - camera.centreX * s),
      this.dpr * (height / 2 + camera.centreY * s),
    );
    return s;
  }

  private drawGrid(palette: CanvasPalette): void {
    const { gridStyle, gridSpacingMm } = this.state.options;
    if (gridStyle === 'points' && gridSpacingMm !== undefined) {
      this.drawPointGrid(palette, gridSpacingMm);
      return;
    }
    const { context, viewport } = this;
    const { camera } = this.state;
    const { minor, major } = gridSpacing(camera.pxPerMm);
    const topLeft = screenToJob(camera, viewport, { x: 0, y: 0 });
    const bottomRight = screenToJob(camera, viewport, { x: viewport.width, y: viewport.height });
    context.lineWidth = 1;

    for (let x = Math.floor(topLeft.x / minor) * minor; x <= bottomRight.x; x += minor) {
      const isMajor = Math.abs(x % major) < minor / 2 || Math.abs((x % major) - major) < minor / 2;
      context.strokeStyle = isMajor ? palette.gridMajor : palette.grid;
      const px = Math.round(jobToScreen(camera, viewport, { x, y: 0 }).x) + 0.5;
      context.beginPath();
      context.moveTo(px, 0);
      context.lineTo(px, viewport.height);
      context.stroke();
    }

    for (let y = Math.floor(bottomRight.y / minor) * minor; y <= topLeft.y; y += minor) {
      const isMajor = Math.abs(y % major) < minor / 2 || Math.abs((y % major) - major) < minor / 2;
      context.strokeStyle = isMajor ? palette.gridMajor : palette.grid;
      const py = Math.round(jobToScreen(camera, viewport, { x: 0, y }).y) + 0.5;
      context.beginPath();
      context.moveTo(0, py);
      context.lineTo(viewport.width, py);
      context.stroke();
    }
  }

  private drawPointGrid(palette: CanvasPalette, spacingMm: number): void {
    const { context, viewport } = this;
    const { camera } = this.state;
    const step = gridPointStep(spacingMm, camera.pxPerMm);
    const topLeft = screenToJob(camera, viewport, { x: 0, y: 0 });
    const bottomRight = screenToJob(camera, viewport, { x: viewport.width, y: viewport.height });
    const xs = gridValues(topLeft.x, bottomRight.x, step);
    const ys = gridValues(bottomRight.y, topLeft.y, step);
    context.fillStyle = palette.gridMajor;
    for (const x of xs) {
      const px = Math.round(jobToScreen(camera, viewport, { x, y: 0 }).x);
      for (const y of ys) {
        const py = Math.round(jobToScreen(camera, viewport, { x: 0, y }).y);
        context.fillRect(px - 0.5, py - 0.5, 1.5, 1.5);
      }
    }
  }

  private drawJob(palette: CanvasPalette, job: JobSetup): void {
    const { context, viewport } = this;
    const { camera } = this.state;
    const bounds = jobBounds(job);
    const topLeft = jobToScreen(camera, viewport, { x: bounds.minX, y: bounds.maxY });
    const bottomRight = jobToScreen(camera, viewport, { x: bounds.maxX, y: bounds.minY });
    const width = bottomRight.x - topLeft.x;
    const height = bottomRight.y - topLeft.y;

    context.fillStyle = palette.jobFill;
    context.fillRect(topLeft.x, topLeft.y, width, height);
    context.strokeStyle = palette.jobBorder;
    context.lineWidth = 1.5;
    context.strokeRect(topLeft.x, topLeft.y, width, height);
  }

  private drawDatum(palette: CanvasPalette): void {
    const { context } = this;
    const origin = this.toScreen({ x: 0, y: 0 });
    const arm = 9;
    context.strokeStyle = palette.datum;
    context.lineWidth = 1.5;
    context.beginPath();
    context.moveTo(origin.x - arm, origin.y);
    context.lineTo(origin.x + arm, origin.y);
    context.moveTo(origin.x, origin.y - arm);
    context.lineTo(origin.x, origin.y + arm);
    context.stroke();
    context.beginPath();
    context.arc(origin.x, origin.y, 4, 0, Math.PI * 2);
    context.stroke();
  }

  private drawGuides(palette: CanvasPalette): void {
    const { context, viewport } = this;
    if (this.state.guides.length === 0) {
      return;
    }
    context.save();
    context.strokeStyle = palette.guide;
    context.lineWidth = 1;
    context.setLineDash([6, 4]);
    for (const guide of this.state.guides) {
      context.beginPath();
      if (guide.axis === 'x') {
        const px = Math.round(this.toScreen({ x: guide.position, y: 0 }).x) + 0.5;
        context.moveTo(px, 0);
        context.lineTo(px, viewport.height);
      } else {
        const py = Math.round(this.toScreen({ x: 0, y: guide.position }).y) + 0.5;
        context.moveTo(0, py);
        context.lineTo(viewport.width, py);
      }
      context.stroke();
    }
    context.restore();
  }

  private visibleEntries(): DisplayEntry[] {
    const { camera } = this.state;
    const { viewport } = this;
    const topLeft = screenToJob(camera, viewport, { x: 0, y: 0 });
    const bottomRight = screenToJob(camera, viewport, { x: viewport.width, y: viewport.height });
    return this.state.entries.filter((entry) => {
      const layer = this.state.layers.get(entry.layer);
      if (layer !== undefined && !layer.visible) {
        return false;
      }
      const [minX, minY, maxX, maxY] = entry.bbox;
      return (
        maxX >= topLeft.x && minX <= bottomRight.x && maxY >= bottomRight.y && minY <= topLeft.y
      );
    });
  }

  private drawEntities(palette: CanvasPalette): void {
    const { context } = this;
    const scale = this.jobSpace();
    const px = 1 / scale;
    const dragging = this.state.dragPreview;
    const visible = this.visibleEntries();

    context.lineJoin = 'round';
    context.lineCap = 'round';
    for (const entry of visible) {
      if (entry.selected) {
        continue;
      }
      const layer = this.state.layers.get(entry.layer);
      if (layer?.keepOut === true && entry.closed) {
        context.globalAlpha = 0.2;
        context.fillStyle = layer.colour;
        context.fill(entry.path, 'evenodd');
        context.globalAlpha = 1;
      }
      context.strokeStyle = entry.closed
        ? legibleOn(layer?.colour ?? palette.node, palette.jobFill, palette.node)
        : palette.openVector;
      context.lineWidth = px;
      context.stroke(entry.path);
    }
    if (this.state.hoverId !== null) {
      const entry = visible.find((e) => e.id === this.state.hoverId);
      if (entry !== undefined && !entry.selected) {
        context.strokeStyle = palette.hover;
        context.lineWidth = 2 * px;
        context.stroke(entry.path);
      }
    }
    context.setLineDash([4 * px, 3 * px]);
    for (const entry of visible) {
      if (!entry.selected || (dragging !== null && dragging.ids.has(entry.id))) {
        continue;
      }
      context.strokeStyle = palette.selection;
      context.lineWidth = 1.5 * px;
      context.stroke(entry.path);
    }
    context.setLineDash([]);
    this.screenSpace();
  }

  private drawDragPreview(palette: CanvasPalette): void {
    const preview = this.state.dragPreview;
    if (preview === null) {
      return;
    }
    const { context } = this;
    const scale = this.jobSpace();
    const px = 1 / scale;
    const [a, b, c, d, e, f] = preview.matrix;
    context.save();
    context.transform(a, b, c, d, e, f);
    const boxes = preview.components ?? [];
    if (boxes.length > 0 && this.state.options.showComponents !== false) {
      context.globalAlpha = DRAG_COMPONENT_ALPHA;
      this.dragComponents.draw(context);
      context.globalAlpha = 1;
    }
    context.setLineDash([4 * px, 3 * px]);
    context.strokeStyle = palette.selection;
    context.lineWidth = 1.5 * px;
    for (const entry of this.state.entries) {
      if (preview.ids.has(entry.id)) {
        context.stroke(entry.path);
      }
    }
    for (const box of boxes) {
      context.strokeRect(box[0], box[1], box[2] - box[0], box[3] - box[1]);
    }
    context.restore();
    this.screenSpace();
  }

  private drawSelection(palette: CanvasPalette): void {
    const frame = this.state.selectionFrame;
    if (frame === null || this.state.dragPreview !== null || this.state.nodeOverlay !== null) {
      return;
    }
    const { context } = this;
    const [minX, minY, maxX, maxY] = frame.extent;
    const corners = [
      { x: minX, y: minY },
      { x: maxX, y: minY },
      { x: maxX, y: maxY },
      { x: minX, y: maxY },
    ].map((p) => this.toScreen(fromFrame(frame, p)));
    context.save();
    context.strokeStyle = palette.handle;
    context.lineWidth = 1;
    context.setLineDash([3, 3]);
    context.beginPath();
    corners.forEach((p, index) => {
      const x = Math.round(p.x) + 0.5;
      const y = Math.round(p.y) + 0.5;
      if (index === 0) {
        context.moveTo(x, y);
      } else {
        context.lineTo(x, y);
      }
    });
    context.closePath();
    context.stroke();
    context.setLineDash([]);
    if (this.state.handleMode !== 'none') {
      const pivot =
        this.state.handleMode === 'pivot' ? rotationCentre(frame, this.state.rotationCentre) : null;
      const handles = selectionHandles(frame, this.state.camera, this.viewport, pivot);
      for (const handle of handles) {
        if (handle.id === 'pivot') {
          this.drawPivot(handle, palette);
          continue;
        }
        // Scale handles are hollow, rotate handles solid.
        context.fillStyle =
          handle.id === 'rotate' ? String(context.strokeStyle) : palette.handleFill;
        context.fillRect(
          handle.x - HANDLE_SIZE / 2,
          handle.y - HANDLE_SIZE / 2,
          HANDLE_SIZE,
          HANDLE_SIZE,
        );
        context.strokeRect(
          Math.round(handle.x - HANDLE_SIZE / 2) + 0.5,
          Math.round(handle.y - HANDLE_SIZE / 2) + 0.5,
          HANDLE_SIZE,
          HANDLE_SIZE,
        );
      }
    }
    context.restore();
  }

  /** The rotational anchor: a ring with a cross through it. */
  private drawPivot(at: Point2, palette: CanvasPalette): void {
    const { context } = this;
    const x = Math.round(at.x) + 0.5;
    const y = Math.round(at.y) + 0.5;
    const reach = PIVOT_RADIUS + 4;
    context.save();
    context.fillStyle = palette.handleFill;
    context.beginPath();
    context.arc(x, y, PIVOT_RADIUS, 0, Math.PI * 2);
    context.fill();
    context.stroke();
    context.beginPath();
    context.moveTo(x - reach, y);
    context.lineTo(x + reach, y);
    context.moveTo(x, y - reach);
    context.lineTo(x, y + reach);
    context.stroke();
    context.restore();
  }

  private drawNodeOverlay(palette: CanvasPalette): void {
    const overlay = this.state.nodeOverlay;
    if (overlay === null) {
      return;
    }
    const { context } = this;
    context.save();
    context.lineWidth = 1;
    context.strokeStyle = palette.controlHandle;
    for (const handle of overlay.handles) {
      const anchor = this.toScreen({ x: handle.anchorX, y: handle.anchorY });
      const point = this.toScreen({ x: handle.x, y: handle.y });
      context.beginPath();
      context.moveTo(anchor.x, anchor.y);
      context.lineTo(point.x, point.y);
      context.stroke();
      context.fillStyle = palette.nodeFill;
      context.beginPath();
      context.arc(point.x, point.y, NODE_SIZE / 2, 0, Math.PI * 2);
      context.fill();
      context.stroke();
    }
    for (const node of overlay.nodes) {
      const point = this.toScreen(node);
      context.strokeStyle = node.smooth ? palette.nodeSmooth : palette.node;
      context.fillStyle = node.selected
        ? palette.selection
        : node.start
          ? palette.snap
          : palette.nodeFill;
      if (node.smooth) {
        context.beginPath();
        context.arc(point.x, point.y, NODE_SIZE / 2 + 0.5, 0, Math.PI * 2);
        context.fill();
        context.stroke();
      } else {
        context.fillRect(point.x - NODE_SIZE / 2, point.y - NODE_SIZE / 2, NODE_SIZE, NODE_SIZE);
        context.strokeRect(
          Math.round(point.x - NODE_SIZE / 2) + 0.5,
          Math.round(point.y - NODE_SIZE / 2) + 0.5,
          NODE_SIZE,
          NODE_SIZE,
        );
      }
    }
    context.restore();
  }

  /**
   * Toolpaths seen from above: cutting moves solid, rapids dashed, plunges,
   * retracts and joins in their own colours; with solid previews on, the
   * width each cut takes is filled under the lines first.
   */
  /**
   * Toolpath Tiling: each tile's overlap shaded, its edges drawn, the tile
   * the views show drawn heavier.
   */
  private drawTiles(palette: CanvasPalette): void {
    const overlay = this.state.tiles;
    if (overlay === null || overlay.tiles.length === 0) {
      return;
    }
    const { context } = this;
    const px = 1 / this.jobSpace();
    context.save();
    context.fillStyle = palette.tileOverlap;
    for (const { nominal, area } of overlay.tiles) {
      const [nx0 = 0, ny0 = 0, nx1 = 0, ny1 = 0] = nominal;
      const [ax0 = 0, ay0 = 0] = area;
      if (ax0 < nx0) {
        context.fillRect(ax0, ay0, nx0 - ax0, ny1 - ay0);
      }
      if (ay0 < ny0) {
        context.fillRect(nx0, ay0, nx1 - nx0, ny0 - ay0);
      }
    }
    context.strokeStyle = palette.tileLine;
    overlay.tiles.forEach(({ nominal }, index) => {
      const [x0 = 0, y0 = 0, x1 = 0, y1 = 0] = nominal;
      context.lineWidth = (index === overlay.active ? 3 : 1.5) * px;
      context.strokeRect(x0, y0, x1 - x0, y1 - y0);
    });
    context.restore();
  }

  /** The toolpath segments as job space paths, built once per set of
   * moves rather than on every frame (a finish can have a million). */
  private toolpathStrokes(paths: ToolpathPaths): ToolpathStrokes {
    const cached = this.strokes;
    if (cached !== null && cached.source === paths) {
      return cached;
    }
    const built: ToolpathStrokes = {
      source: paths,
      lines: {
        cut: segmentPath(paths.lines.cut),
        rapid: segmentPath(paths.lines.rapid),
        plunge: segmentPath(paths.lines.plunge),
        retract: segmentPath(paths.lines.retract),
        join: segmentPath(paths.lines.join),
      },
      solid: paths.solid.map((bucket) => ({
        width: bucket.width,
        path: segmentPath(bucket.segments),
      })),
    };
    this.strokes = built;
    return built;
  }

  private drawToolpaths(palette: CanvasPalette): void {
    const paths = this.state.toolpaths;
    if (paths === null) {
      this.strokes = null;
      return;
    }
    const { context } = this;
    const px = 1 / this.jobSpace();
    const strokes = this.toolpathStrokes(paths);
    const stroke = (path: Path2D | null) => {
      if (path !== null) {
        context.stroke(path);
      }
    };
    context.save();
    if (strokes.solid.length > 0) {
      context.globalAlpha = SOLID_PREVIEW_ALPHA;
      context.strokeStyle = palette.toolpathSolid;
      context.lineCap = 'round';
      context.lineJoin = 'round';
      for (const bucket of strokes.solid) {
        context.lineWidth = Math.max(bucket.width, px);
        stroke(bucket.path);
      }
      context.globalAlpha = 1;
      context.lineCap = 'butt';
    }
    context.lineWidth = 1.25 * px;
    context.strokeStyle = palette.toolpathCut;
    stroke(strokes.lines.cut);
    context.strokeStyle = palette.toolpathPlunge;
    stroke(strokes.lines.plunge);
    context.strokeStyle = palette.toolpathJoin;
    stroke(strokes.lines.join);
    context.lineWidth = px;
    context.strokeStyle = palette.toolpathRetract;
    stroke(strokes.lines.retract);
    context.strokeStyle = palette.toolpathRapid;
    context.setLineDash([3 * px, 3 * px]);
    stroke(strokes.lines.rapid);
    context.restore();
    this.screenSpace();
  }

  private drawOtherSide(palette: CanvasPalette): void {
    if (this.state.otherSide.length === 0) {
      return;
    }
    const { context } = this;
    const scale = this.jobSpace();
    const px = 1 / scale;
    context.save();
    context.strokeStyle = palette.gridMajor;
    context.lineWidth = px;
    context.setLineDash([2 * px, 2 * px]);
    for (const path of this.state.otherSide) {
      for (const sub of flattenPath(path, 0.5 * px)) {
        if (sub.points.length < 4) {
          continue;
        }
        context.beginPath();
        context.moveTo(sub.points[0] ?? 0, sub.points[1] ?? 0);
        for (let i = 2; i + 1 < sub.points.length; i += 2) {
          context.lineTo(sub.points[i] ?? 0, sub.points[i + 1] ?? 0);
        }
        if (sub.closed) {
          context.closePath();
        }
        context.stroke();
      }
    }
    context.restore();
    this.screenSpace();
  }

  private drawPreviewPaths(palette: CanvasPalette): void {
    if (this.state.previewPaths.length === 0) {
      return;
    }
    const { context } = this;
    const scale = this.jobSpace();
    const px = 1 / scale;
    context.save();
    context.strokeStyle = palette.selection;
    context.lineWidth = px;
    context.setLineDash([4 * px, 3 * px]);
    for (const path of this.state.previewPaths) {
      for (const sub of flattenPath(path, 0.25 * px)) {
        if (sub.points.length < 4) {
          continue;
        }
        context.beginPath();
        context.moveTo(sub.points[0] ?? 0, sub.points[1] ?? 0);
        for (let i = 2; i + 1 < sub.points.length; i += 2) {
          context.lineTo(sub.points[i] ?? 0, sub.points[i + 1] ?? 0);
        }
        if (sub.closed) {
          context.closePath();
        }
        context.stroke();
      }
    }
    context.restore();
    this.screenSpace();
  }

  private drawPreviewOverlays(palette: CanvasPalette): void {
    if (this.state.previewOverlays.length === 0) {
      return;
    }
    const { context } = this;
    const scale = this.jobSpace();
    const px = 1 / scale;
    const colour = {
      first: palette.railFirst,
      second: palette.railSecond,
      accent: palette.selection,
    };
    context.save();
    for (const overlay of this.state.previewOverlays) {
      context.strokeStyle = colour[overlay.tone];
      context.fillStyle = colour[overlay.tone];
      context.lineWidth = 2 * px;
      context.setLineDash(overlay.dashed === true ? [4 * px, 3 * px] : []);
      for (const sub of flattenPath(overlay.path, 0.25 * px)) {
        if (sub.points.length < 4) {
          continue;
        }
        context.beginPath();
        context.moveTo(sub.points[0] ?? 0, sub.points[1] ?? 0);
        for (let i = 2; i + 1 < sub.points.length; i += 2) {
          context.lineTo(sub.points[i] ?? 0, sub.points[i + 1] ?? 0);
        }
        if (sub.closed) {
          context.closePath();
        }
        if (overlay.fill === true) {
          context.fill();
        } else {
          context.stroke();
        }
      }
    }
    context.restore();
    this.screenSpace();
  }

  private drawPreviewLabels(palette: CanvasPalette): void {
    if (this.state.previewLabels.length === 0) {
      return;
    }
    const { context } = this;
    const colour = {
      first: palette.railFirst,
      second: palette.railSecond,
      accent: palette.selection,
    };
    context.save();
    context.font = 'bold 16px system-ui, sans-serif';
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    for (const label of this.state.previewLabels) {
      const p = this.toScreen(label.at);
      context.fillStyle = colour[label.tone];
      context.fillText(label.text, p.x, p.y);
    }
    context.restore();
  }

  private drawRubberBand(palette: CanvasPalette): void {
    const band = this.state.rubberBand;
    if (band === null) {
      return;
    }
    const { context } = this;
    const a = this.toScreen(band.from);
    const b = this.toScreen(band.to);
    context.save();
    context.fillStyle = palette.rubberBandFill;
    context.strokeStyle = palette.rubberBand;
    context.lineWidth = 1;
    context.setLineDash(band.mode === 'touch' ? [4, 3] : []);
    const x = Math.round(Math.min(a.x, b.x)) + 0.5;
    const y = Math.round(Math.min(a.y, b.y)) + 0.5;
    const w = Math.round(Math.abs(b.x - a.x));
    const h = Math.round(Math.abs(b.y - a.y));
    context.fillRect(x, y, w, h);
    context.strokeRect(x, y, w, h);
    context.restore();
  }

  private drawAlignment(palette: CanvasPalette): void {
    const alignment = this.state.alignment;
    if (alignment === null) {
      return;
    }
    const { context } = this;
    context.save();
    context.strokeStyle = palette.snap;
    context.lineWidth = 1;
    context.setLineDash([4, 3]);
    context.beginPath();
    for (const line of alignment.lines) {
      const from = this.toScreen(line.from);
      const to = this.toScreen(line.to);
      // Snap to the pixel grid across the line so the dashes stay crisp.
      if (line.axis === 'x') {
        const x = Math.round(to.x) + 0.5;
        context.moveTo(x, from.y);
        context.lineTo(x, to.y);
      } else {
        const y = Math.round(to.y) + 0.5;
        context.moveTo(from.x, y);
        context.lineTo(to.x, y);
      }
    }
    context.stroke();
    context.setLineDash([]);
    context.beginPath();
    for (const line of alignment.lines) {
      const from = this.toScreen(line.from);
      context.moveTo(from.x - 3, from.y - 3);
      context.lineTo(from.x + 3, from.y + 3);
      context.moveTo(from.x + 3, from.y - 3);
      context.lineTo(from.x - 3, from.y + 3);
    }
    context.stroke();
    context.restore();
  }

  private drawSnapMarker(palette: CanvasPalette): void {
    const hit = this.state.snapHit;
    if (hit === null || (hit.kind === 'grid' && this.state.alignment !== null)) {
      return;
    }
    const { context } = this;
    const p = this.toScreen(hit);
    context.save();
    context.strokeStyle = palette.snap;
    context.lineWidth = 1.5;
    context.beginPath();
    switch (hit.kind) {
      case 'node':
      case 'jobCorner':
        context.rect(p.x - SNAP_MARK / 2, p.y - SNAP_MARK / 2, SNAP_MARK, SNAP_MARK);
        break;
      case 'mid':
        context.moveTo(p.x, p.y - SNAP_MARK / 2 - 1);
        context.lineTo(p.x + SNAP_MARK / 2 + 1, p.y + SNAP_MARK / 2);
        context.lineTo(p.x - SNAP_MARK / 2 - 1, p.y + SNAP_MARK / 2);
        context.closePath();
        break;
      case 'centre':
      case 'jobCentre':
      case 'quadrant':
        context.arc(p.x, p.y, SNAP_MARK / 2 + 1, 0, Math.PI * 2);
        break;
      case 'intersection':
        context.moveTo(p.x - SNAP_MARK / 2, p.y - SNAP_MARK / 2);
        context.lineTo(p.x + SNAP_MARK / 2, p.y + SNAP_MARK / 2);
        context.moveTo(p.x + SNAP_MARK / 2, p.y - SNAP_MARK / 2);
        context.lineTo(p.x - SNAP_MARK / 2, p.y + SNAP_MARK / 2);
        break;
      default:
        context.moveTo(p.x - SNAP_MARK, p.y);
        context.lineTo(p.x + SNAP_MARK, p.y);
        context.moveTo(p.x, p.y - SNAP_MARK);
        context.lineTo(p.x, p.y + SNAP_MARK);
    }
    context.stroke();
    context.fillStyle = palette.snap;
    context.font = '10px system-ui, sans-serif';
    context.textBaseline = 'bottom';
    context.textAlign = 'left';
    context.fillText(snapLabel(hit.kind), p.x + SNAP_MARK + 2, p.y - 2);
    context.restore();
  }

  private drawRulers(palette: CanvasPalette): void {
    const { context, viewport } = this;
    const { camera } = this.state;
    const { major, minor } = gridSpacing(camera.pxPerMm);
    const topLeft = screenToJob(camera, viewport, { x: 0, y: 0 });
    const bottomRight = screenToJob(camera, viewport, { x: viewport.width, y: viewport.height });

    context.fillStyle = palette.rulerBackground;
    context.fillRect(0, 0, viewport.width, RULER_SIZE);
    context.fillRect(0, 0, RULER_SIZE, viewport.height);
    const tint = this.state.options.rulerTint;
    if (tint !== undefined && tint !== null) {
      context.save();
      context.globalAlpha = 0.35;
      context.fillStyle = tint;
      context.fillRect(0, 0, viewport.width, RULER_SIZE);
      context.fillRect(0, 0, RULER_SIZE, viewport.height);
      context.restore();
    }
    context.strokeStyle = palette.rulerLine;
    context.lineWidth = 1;
    context.beginPath();
    context.moveTo(0, RULER_SIZE + 0.5);
    context.lineTo(viewport.width, RULER_SIZE + 0.5);
    context.moveTo(RULER_SIZE + 0.5, 0);
    context.lineTo(RULER_SIZE + 0.5, viewport.height);
    for (let x = Math.ceil(topLeft.x / minor) * minor; x <= bottomRight.x; x += minor) {
      const px = Math.round(jobToScreen(camera, viewport, { x, y: 0 }).x) + 0.5;
      const isMajor = Math.abs(x % major) < minor / 2 || Math.abs((x % major) - major) < minor / 2;
      context.moveTo(px, RULER_SIZE);
      context.lineTo(px, RULER_SIZE - (isMajor ? 10 : 4));
    }
    for (let y = Math.ceil(bottomRight.y / minor) * minor; y <= topLeft.y; y += minor) {
      const py = Math.round(jobToScreen(camera, viewport, { x: 0, y }).y) + 0.5;
      const isMajor = Math.abs(y % major) < minor / 2 || Math.abs((y % major) - major) < minor / 2;
      context.moveTo(RULER_SIZE, py);
      context.lineTo(RULER_SIZE - (isMajor ? 10 : 4), py);
    }
    context.stroke();

    context.fillStyle = palette.rulerText;
    context.font = '10px system-ui, sans-serif';
    context.textBaseline = 'middle';
    context.textAlign = 'center';
    for (let x = Math.ceil(topLeft.x / major) * major; x <= bottomRight.x; x += major) {
      const px = jobToScreen(camera, viewport, { x, y: 0 }).x;
      if (px < RULER_SIZE) {
        continue;
      }
      context.fillText(formatTick(x), px, RULER_SIZE / 2 - 3);
    }
    context.textAlign = 'right';
    for (let y = Math.ceil(bottomRight.y / major) * major; y <= topLeft.y; y += major) {
      const py = jobToScreen(camera, viewport, { x: 0, y }).y;
      if (py < RULER_SIZE) {
        continue;
      }
      context.save();
      context.translate(RULER_SIZE / 2 - 3, py);
      context.rotate(-Math.PI / 2);
      context.textAlign = 'center';
      context.fillText(formatTick(y), 0, 0);
      context.restore();
    }

    const cursor = this.state.rulerCursor;
    if (cursor !== null) {
      context.strokeStyle = palette.snap;
      context.beginPath();
      context.moveTo(Math.round(cursor.x) + 0.5, 0);
      context.lineTo(Math.round(cursor.x) + 0.5, RULER_SIZE);
      context.moveTo(0, Math.round(cursor.y) + 0.5);
      context.lineTo(RULER_SIZE, Math.round(cursor.y) + 0.5);
      context.stroke();
    }
    context.fillStyle = palette.rulerBackground;
    context.fillRect(0, 0, RULER_SIZE, RULER_SIZE);
  }
}

function snapLabel(kind: ProbeHit['kind']): string {
  return strings.snap[kind];
}
