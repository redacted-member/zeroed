// Turns DOM events on the drawing canvas into camera changes, guide edits
// and tool calls. Snapping probes go to the kernel, one in flight at a time.

import { api } from '../../bridge/api';
import type { ProbeHit } from '../../bridge/document';
import { setToolCommandHandler } from '../../lib/toolCommands';
import { useDocumentStore } from '../../state/documentStore';
import { useDrawStore } from '../../state/drawStore';
import { useToolpathStore } from '../../state/toolpathStore';
import { useUiStore } from '../../state/uiStore';
import { isDrawingForm } from '../../shell/LeftPanel/drawingForms';
import { useViewStore, type ToolId } from '../../state/viewStore';
import { panBy, zoomAt, type Point2 } from './camera';
import { RULER_SIZE, type View2DRenderer } from './renderer';
import { alignmentCandidates, findAlignment, type AlignmentSnap } from './smartSnap';
import { CreateTool } from './tools/createTool';
import { NodeTool } from './tools/nodeTool';
import { PanTool } from './tools/panTool';
import { SelectTool } from './tools/selectTool';
import { tileAt } from './tileAt';
import type { PointerInfo, ToolController, ToolHost } from './tools/types';
import { ZoomBoxTool } from './tools/zoomBoxTool';
import { modalDialogOpen } from '../../lib/dialogKeys';

const ZOOM_PER_WHEEL_NOTCH = 1.12;
const MIDDLE_BUTTON = 1;
const RIGHT_BUTTON = 2;
const GUIDE_HIT_PX = 4;
/** Pick reach for a vector that keeps a double click from choosing a tile. */
const TILE_PICK_PX = 4;
/** How far (screen pixels) smart snapping looks for objects to align with. */
const SMART_REACH_PX = 400;

type Gesture =
  | { kind: 'none' }
  | { kind: 'pan'; last: Point2; pointerId: number; button: number; moved: boolean; client: Point2 }
  | { kind: 'newGuide'; axis: 'x' | 'y'; id: number | null; pointerId: number }
  | { kind: 'moveGuide'; id: number; pointerId: number }
  | { kind: 'tool'; pointerId: number };

export class InteractionController implements ToolHost {
  readonly renderer: View2DRenderer;
  private readonly canvas: HTMLCanvasElement;
  private readonly draw: () => void;
  private readonly tools: Record<ToolId, ToolController> = {
    select: new SelectTool(),
    node: new NodeTool(),
    zoomBox: new ZoomBoxTool(),
    pan: new PanTool(),
    create: new CreateTool(),
  };
  private tool: ToolController;
  private gesture: Gesture = { kind: 'none' };
  private spaceDown = false;
  private lastPointer: Point2 | null = null;
  private probeQueue: { job: Point2; exclude: readonly number[] } | null = null;
  private probing = false;
  /** Set while a tool asks for snaps; smart snapping runs only then. */
  private snapExclude: readonly number[] | null = null;
  private readonly disposers: Array<() => void> = [];

  constructor(canvas: HTMLCanvasElement, renderer: View2DRenderer, draw: () => void) {
    this.canvas = canvas;
    this.renderer = renderer;
    this.draw = draw;
    this.tool = this.tools.select;
  }

  attach(): void {
    const { canvas } = this;
    const on = <K extends keyof HTMLElementEventMap>(
      type: K,
      handler: (event: HTMLElementEventMap[K]) => void,
      options?: AddEventListenerOptions,
    ) => {
      canvas.addEventListener(type, handler, options);
      this.disposers.push(() => canvas.removeEventListener(type, handler));
    };
    on('pointerdown', (e) => void this.onPointerDown(e));
    on('pointermove', (e) => this.onPointerMove(e));
    on('pointerup', (e) => void this.onPointerUp(e));
    on('pointercancel', (e) => void this.onPointerUp(e));
    on('pointerleave', () => this.onPointerLeave());
    on('dblclick', (e) => void this.onDoubleClick(e));
    on('wheel', (e) => this.onWheel(e), { passive: false });
    on('contextmenu', (e) => e.preventDefault());
    const onKeyDown = (e: KeyboardEvent) => void this.onKeyDown(e);
    const onKeyUp = (e: KeyboardEvent) => this.onKeyUp(e);
    // Capture phase, so a key the tool consumes (Quick Keys) is marked
    // handled before the shell's shortcut listener sees it.
    window.addEventListener('keydown', onKeyDown, true);
    window.addEventListener('keyup', onKeyUp);
    this.disposers.push(() => window.removeEventListener('keydown', onKeyDown, true));
    this.disposers.push(() => window.removeEventListener('keyup', onKeyUp));
    void this.tool.activate(this);
    this.setCursor(this.tool.cursor);
    setToolCommandHandler((key) => {
      const fake = { key, preventDefault: () => undefined } as KeyboardEvent;
      void this.tool.keyDown?.(fake, this.lastPointer);
    });
  }

  detach(): void {
    setToolCommandHandler(null);
    this.tool.deactivate();
    for (const dispose of this.disposers) {
      dispose();
    }
    this.disposers.length = 0;
  }

  // ToolHost

  requestDraw(): void {
    this.renderer.update({ overlay: this.tool.overlay() });
    this.draw();
  }

  mmPerPx(): number {
    return 1 / this.renderer.state.camera.pxPerMm;
  }

  setCursor(cursor: string): void {
    this.canvas.style.cursor = cursor;
  }

  switchTool(id: ToolId): void {
    useViewStore.getState().setActiveTool(id);
  }

  /** Called by the view when the store's active tool changes. */
  useTool(id: ToolId): void {
    if (this.tool.id === id) {
      return;
    }
    this.tool.deactivate();
    this.tool = this.tools[id];
    void this.tool.activate(this);
    this.setCursor(this.tool.cursor);
    this.requestDraw();
  }

  probe(job: Point2, exclude: readonly number[] = []): void {
    this.snapExclude = exclude;
    this.probeQueue = { job, exclude };
    if (this.probing) {
      return;
    }
    this.probing = true;
    void this.drainProbes();
  }

  clearSnap(): void {
    this.probeQueue = null;
    this.snapExclude = null;
    if (this.renderer.state.alignment !== null) {
      this.renderer.update({ alignment: null });
      this.draw();
    }
    if (useViewStore.getState().snapHit !== null) {
      useViewStore.getState().setSnapHit(null);
      this.renderer.update({ snapHit: null });
      this.draw();
    }
  }

  private async drainProbes(): Promise<void> {
    try {
      while (this.probeQueue !== null) {
        const request = this.probeQueue;
        this.probeQueue = null;
        const view = useViewStore.getState();
        const radius = view.snapRadiusPx * this.mmPerPx();
        const reply = await api.entities.probe({
          x: request.job.x,
          y: request.job.y,
          radius,
          // Smart snapping runs in the view so it can draw its alignment lines.
          snap: { ...view.snap, smart: false },
          gridSpacing: view.snap.grid ? view.gridSpacing : 0,
          guides: view.guides,
          exclude: [...request.exclude],
        });
        const hit: ProbeHit | null = reply.hits[0] ?? null;
        useViewStore.getState().setSnapHit(hit);
        this.renderer.update({ snapHit: hit });
        this.draw();
      }
    } finally {
      this.probing = false;
    }
  }

  // Events

  private pointerInfo(event: PointerEvent | MouseEvent): PointerInfo {
    const rect = this.canvas.getBoundingClientRect();
    const screen = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const job = this.renderer.pointAt(screen);
    const hit = useViewStore.getState().snapHit;
    let snapped = job;
    let hitUsed = false;
    if (hit !== null) {
      const hitScreen = this.renderer.toScreen(hit);
      const radius = useViewStore.getState().snapRadiusPx;
      if (Math.hypot(hitScreen.x - screen.x, hitScreen.y - screen.y) <= radius * 1.5) {
        snapped = { x: hit.x, y: hit.y };
        // Object snaps beat alignment; the grid only applies when nothing aligns.
        hitUsed = hit.kind !== 'grid';
      }
    }
    const alignment = hitUsed ? null : this.smartSnap(job);
    if (alignment !== null) {
      snapped = alignment.point;
    }
    if (alignment !== null || this.renderer.state.alignment !== null) {
      this.renderer.update({ alignment });
    }
    return {
      screen,
      job,
      snapped,
      button: event.button,
      shift: event.shiftKey,
      ctrl: event.ctrlKey || event.metaKey,
      alt: event.altKey,
    };
  }

  /** Alignment with key points of nearby objects, while a tool is snapping. */
  private smartSnap(job: Point2): AlignmentSnap | null {
    const exclude = this.snapExclude;
    const view = useViewStore.getState();
    if (exclude === null || !view.snap.smart) {
      return null;
    }
    const skipped = new Set(exclude);
    for (const entity of useDocumentStore.getState().entities) {
      if (skipped.has(entity.id)) {
        for (const child of entity.children ?? []) {
          skipped.add(child);
        }
      }
    }
    const mmPerPx = this.mmPerPx();
    const layers = this.renderer.state.layers;
    const candidates = alignmentCandidates(
      this.renderer.state.entries,
      job,
      SMART_REACH_PX * mmPerPx,
      (source) => skipped.has(source.id) || layers.get(source.layer)?.visible === false,
    );
    return findAlignment(job, candidates, view.snapRadiusPx * mmPerPx);
  }

  private rulerAt(screen: Point2): 'top' | 'left' | null {
    if (!this.renderer.state.options.showRulers) {
      return null;
    }
    if (screen.y < RULER_SIZE) {
      return 'top';
    }
    if (screen.x < RULER_SIZE) {
      return 'left';
    }
    return null;
  }

  /** A guide let go here is deleted: over a ruler, or dragged out of the view. */
  private dropsGuide(screen: Point2): boolean {
    const outside =
      screen.x < 0 ||
      screen.y < 0 ||
      screen.x > this.canvas.clientWidth ||
      screen.y > this.canvas.clientHeight;
    return outside || this.rulerAt(screen) !== null;
  }

  private guideAt(screen: Point2): number | null {
    for (const guide of useViewStore.getState().guides) {
      const s =
        guide.axis === 'x'
          ? this.renderer.toScreen({ x: guide.position, y: 0 }).x - screen.x
          : this.renderer.toScreen({ x: 0, y: guide.position }).y - screen.y;
      if (Math.abs(s) <= GUIDE_HIT_PX) {
        return guide.id;
      }
    }
    return null;
  }

  /**
   * Esc or a right click leaves a Drawing tab tool: what is in progress is
   * ended (a line keeps the points placed so far) and the form closes.
   * False when no Drawing tab tool is drawing in the view.
   */
  private exitDrawingTool(): boolean {
    const form = useUiStore.getState().activeForm;
    if (this.tool.id !== 'create' || !isDrawingForm(form)) {
      return false;
    }
    this.tool.cancel();
    useUiStore.getState().closeForm();
    return true;
  }

  private async onPointerDown(event: PointerEvent): Promise<void> {
    const info = this.pointerInfo(event);
    try {
      this.canvas.setPointerCapture(event.pointerId);
    } catch {
      // Synthetic events carry no active pointer; capture is a convenience only.
    }
    this.canvas.focus();
    if (event.button === MIDDLE_BUTTON || event.button === RIGHT_BUTTON || this.spaceDown) {
      event.preventDefault();
      this.gesture = {
        kind: 'pan',
        last: info.screen,
        pointerId: event.pointerId,
        button: event.button,
        moved: false,
        client: { x: event.clientX, y: event.clientY },
      };
      this.setCursor('grabbing');
      return;
    }
    if (event.button !== 0) {
      return;
    }
    const ruler = this.rulerAt(info.screen);
    if (ruler !== null) {
      this.gesture = {
        kind: 'newGuide',
        axis: ruler === 'top' ? 'y' : 'x',
        id: null,
        pointerId: event.pointerId,
      };
      return;
    }
    if (this.tool.id === 'select') {
      const guide = this.guideAt(info.screen);
      const locked = useViewStore.getState().guides.find((g) => g.id === guide)?.locked ?? false;
      if (guide !== null && !locked && this.renderer.state.selectionBbox === null) {
        this.gesture = { kind: 'moveGuide', id: guide, pointerId: event.pointerId };
        return;
      }
    }
    this.gesture = { kind: 'tool', pointerId: event.pointerId };
    await this.tool.pointerDown(info);
  }

  private onPointerMove(event: PointerEvent): void {
    const info = this.pointerInfo(event);
    this.lastPointer = info.screen;
    useViewStore.getState().setCursor(info.job);
    this.renderer.update({ rulerCursor: info.screen });
    const gesture = this.gesture;
    switch (gesture.kind) {
      case 'pan': {
        const view = useViewStore.getState();
        view.setCamera(
          panBy(view.camera, info.screen.x - gesture.last.x, info.screen.y - gesture.last.y),
        );
        gesture.last = info.screen;
        return;
      }
      case 'newGuide': {
        const view = useViewStore.getState();
        const position = gesture.axis === 'x' ? info.job.x : info.job.y;
        if (gesture.id === null) {
          if (this.rulerAt(info.screen) === null) {
            gesture.id = view.addGuide(gesture.axis, position);
          }
        } else {
          view.moveGuide(gesture.id, position);
        }
        this.syncGuides();
        return;
      }
      case 'moveGuide': {
        const view = useViewStore.getState();
        const guide = view.guides.find((g) => g.id === gesture.id);
        if (guide !== undefined) {
          view.moveGuide(guide.id, guide.axis === 'x' ? info.job.x : info.job.y);
          this.syncGuides();
        }
        return;
      }
      case 'tool':
      case 'none':
        if (
          gesture.kind === 'none' &&
          this.tool.id === 'select' &&
          this.guideAt(info.screen) !== null
        ) {
          this.setCursor('col-resize');
        } else if (gesture.kind === 'none') {
          this.setCursor(this.tool.cursor);
        }
        this.tool.pointerMove(info);
        this.draw();
    }
  }

  private syncGuides(): void {
    this.renderer.update({ guides: useViewStore.getState().guides });
    this.draw();
  }

  private async onPointerUp(event: PointerEvent): Promise<void> {
    const info = this.pointerInfo(event);
    const gesture = this.gesture;
    this.gesture = { kind: 'none' };
    if (this.canvas.hasPointerCapture(event.pointerId)) {
      this.canvas.releasePointerCapture(event.pointerId);
    }
    switch (gesture.kind) {
      case 'pan':
        this.setCursor(this.tool.cursor);
        if (gesture.button === RIGHT_BUTTON && !gesture.moved) {
          // A right click on a guide opens its Guide Properties.
          const guide = this.tool.id === 'select' ? this.guideAt(info.screen) : null;
          if (guide !== null) {
            this.editGuide(guide);
            return;
          }
          if (
            this.tool.id === 'create' &&
            useDrawStore.getState().controller?.rightClick?.(info.job) === true
          ) {
            return;
          }
          const items =
            this.tool.id === 'create'
              ? (useDrawStore.getState().controller?.contextMenu?.(info.job) ?? null)
              : null;
          if ((items === null || items.length === 0) && this.exitDrawingTool()) {
            return;
          }
          useUiStore.getState().openContextMenu({
            x: gesture.client.x,
            y: gesture.client.y,
            ...(items === null || items.length === 0 ? {} : { items }),
          });
        }
        return;
      case 'newGuide':
        if (gesture.id !== null && this.dropsGuide(info.screen)) {
          useViewStore.getState().removeGuide(gesture.id);
          this.syncGuides();
        }
        return;
      case 'moveGuide':
        if (this.dropsGuide(info.screen)) {
          useViewStore.getState().removeGuide(gesture.id);
          this.syncGuides();
        }
        return;
      case 'tool':
        await this.tool.pointerUp(info);
        return;
      case 'none':
        return;
    }
  }

  private onPointerLeave(): void {
    useViewStore.getState().setCursor(null);
    this.renderer.update({ rulerCursor: null });
    this.draw();
  }

  private async onDoubleClick(event: MouseEvent): Promise<void> {
    const info = this.pointerInfo(event);
    const guide = this.guideAt(info.screen);
    if (guide !== null && this.tool.id === 'select') {
      this.editGuide(guide);
      return;
    }
    if (this.tool.id === 'select' && (await this.activateTile(info))) {
      return;
    }
    await this.tool.doubleClick?.(info);
  }

  /** With Toolpath Tiling on, a double click on a tile makes it the one the
   * views show: always while the tiling form is open, otherwise when no
   * vector is under the pointer. Answers whether it did. */
  private async activateTile(info: PointerInfo): Promise<boolean> {
    const tiling = useToolpathStore.getState().tiling;
    if (tiling === null || !tiling.enabled) {
      return false;
    }
    const index = tileAt(tiling.tiles, info.job.x, info.job.y);
    if (index === null) {
      return false;
    }
    if (useUiStore.getState().activeForm !== 'toolpathTiling') {
      const tolerance = TILE_PICK_PX * this.mmPerPx();
      const { ids } = await api.entities.hitTest({ x: info.job.x, y: info.job.y, tolerance });
      if (ids.length > 0) {
        return false;
      }
    }
    if (index !== tiling.active) {
      await useToolpathStore.getState().viewTile({ active: index });
    }
    return true;
  }

  private editGuide(id: number): void {
    useViewStore.getState().setEditingGuide(id);
    useUiStore.getState().openForm('guidePosition');
  }

  private onWheel(event: WheelEvent): void {
    event.preventDefault();
    const rect = this.canvas.getBoundingClientRect();
    const pointer = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const factor = Math.sign(event.deltaY) > 0 ? 1 / ZOOM_PER_WHEEL_NOTCH : ZOOM_PER_WHEEL_NOTCH;
    const view = useViewStore.getState();
    view.setCamera(zoomAt(view.camera, view.viewport, pointer, factor));
  }

  private static isTyping(event: KeyboardEvent): boolean {
    const target = event.target;
    return (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement ||
      (target instanceof HTMLElement && target.isContentEditable) ||
      // A drop-down list such as the font list takes letters and arrows.
      (target instanceof HTMLElement && target.getAttribute('role') === 'combobox')
    );
  }

  private async onKeyDown(event: KeyboardEvent): Promise<void> {
    if (InteractionController.isTyping(event) || modalDialogOpen()) {
      return;
    }
    // A creation form that uses Space (Draw Line ends the line with it)
    // gets it before the view takes it for panning.
    if (event.code === 'Space' && !this.spaceDown && this.tool.id === 'create') {
      const used = this.tool.keyDown?.(event, this.lastPointer);
      if ((used instanceof Promise ? await used : used) === true) {
        event.preventDefault();
        return;
      }
    }
    if (event.code === 'Space' && !this.spaceDown) {
      this.spaceDown = true;
      if (this.gesture.kind === 'none') {
        this.setCursor('grab');
      }
      event.preventDefault();
      return;
    }
    if (event.key === 'Escape') {
      if (this.exitDrawingTool()) {
        event.preventDefault();
        return;
      }
      this.tool.cancel();
      if (this.tool.id === 'create') {
        event.preventDefault();
        return;
      }
      if (this.tool.id !== 'select') {
        this.switchTool('select');
      } else if (useViewStore.getState().transformMode) {
        // Esc leaves Interactive Selection for Vector Selection mode.
        useViewStore.getState().exitTransformMode();
        this.renderer.update({ handleMode: 'none' });
        this.draw();
      } else if (useDocumentStore.getState().selection.ids.length > 0) {
        await useDocumentStore.getState().clearSelection();
        this.renderer.update({ handleMode: 'none' });
        this.draw();
      }
      event.preventDefault();
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    const result = this.tool.keyDown?.(event, this.lastPointer);
    const consumed = result instanceof Promise ? await result : result;
    if (consumed === true) {
      event.preventDefault();
    }
  }

  private onKeyUp(event: KeyboardEvent): void {
    if (event.code === 'Space' && this.spaceDown) {
      this.spaceDown = false;
      if (this.gesture.kind === 'none') {
        this.setCursor(this.tool.cursor);
      }
    }
  }
}
