// Selection tool: click, shift add, ctrl toggle, rubber band in both
// directions, drag to move with snapping, scale and rotate handles on a
// second click, the rotational anchor on a third, double click to enter node
// editing.

import { api } from '../../../bridge/api';
import type { Bbox, Matrix, SelectionFrame } from '../../../bridge/document';
import { rotationAbout, scaleAbout, translation } from '../../../lib/geometry';
import { distanceStep, dragDelta } from '../../../lib/dragSteps';
import { trackPick } from '../../../lib/pickOrder';
import { quickStepPoint, type QuickEntry } from '../../../lib/quickKeys';
import { quickRotation, quickScale } from '../../../lib/quickShapes';
import { inFrame, selectionFrame, snapPivot, toFrame } from '../../../lib/transformMath';
import { toMillimetres } from '../../../lib/units';
import { clearPickedComponents, pickComponentAt } from '../../../state/componentSelection';
import { useDocumentStore } from '../../../state/documentStore';
import { useViewStore } from '../../../state/viewStore';
import {
  handlesAfterMove,
  handlesAfterPick,
  handlesAfterReclick,
  rotationCentre,
  rotationStep,
  scaleFactors,
} from './handleModes';
import { rectBetween, type Point2 } from '../camera';
import {
  selectionHandles,
  type DragPreview,
  type HandleId,
  type OverlayPainter,
} from '../renderer';
import {
  DRAG_THRESHOLD_PX,
  HANDLE_HIT_PX,
  PICK_TOLERANCE_PX,
  type PointerInfo,
  type ToolController,
  type ToolHost,
} from './types';
import { MOVE_CURSOR, SELECT_CURSOR, handleCursor } from './cursors';
import { carriedComponents } from './carriedComponents';
import { clearQuickKeys, quickKeyDown } from './quickInput';

type Phase =
  | { kind: 'idle' }
  | { kind: 'pendingDrag'; ids: number[]; grab: Point2; start: Point2 }
  | {
      kind: 'moving';
      ids: number[];
      grab: Point2;
      last: Point2;
      matrix: Matrix;
      copy: boolean;
    }
  | { kind: 'band'; from: Point2; shift: boolean; ctrl: boolean }
  | {
      kind: 'scaling';
      handle: HandleId;
      /** The frame the scale runs along; anchor and start are in its space. */
      frame: SelectionFrame;
      anchor: Point2;
      start: Point2;
      ids: number[];
      matrix: Matrix;
    }
  | { kind: 'rotating'; centre: Point2; startAngle: number; ids: number[]; matrix: Matrix }
  | { kind: 'pivoting'; frame: SelectionFrame; ids: number[] };

/** How close, px, a dragged rotational anchor snaps to the selection box's points. */
const PIVOT_SNAP_PX = 8;

/** The fixed point of a handle drag, in the selection frame's space. */
function anchorFor(handle: HandleId, frame: SelectionFrame): Point2 {
  const [minX, minY, maxX, maxY] = frame.extent;
  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  switch (handle) {
    case 'nw':
      return { x: maxX, y: minY };
    case 'n':
      return { x: cx, y: minY };
    case 'ne':
      return { x: minX, y: minY };
    case 'e':
      return { x: minX, y: cy };
    case 'se':
      return { x: minX, y: maxY };
    case 's':
      return { x: cx, y: maxY };
    case 'sw':
      return { x: maxX, y: maxY };
    case 'w':
      return { x: maxX, y: cy };
    case 'rotate':
    case 'pivot':
      return { x: cx, y: cy };
  }
}

function transformMode(): boolean {
  return useViewStore.getState().transformMode;
}

export class SelectTool implements ToolController {
  readonly id = 'select' as const;
  readonly cursor = SELECT_CURSOR;
  private host: ToolHost | null = null;
  private phase: Phase = { kind: 'idle' };
  private hoverTimer: number | null = null;
  /** Boxes of the selected components the current drag carries. */
  private carried: Bbox[] = [];
  /** Whether the button is down: a click can end while its press still
   * waits on the kernel, and then must not leave a drag behind. */
  private pressed = false;

  activate(host: ToolHost): void {
    this.host = host;
    this.phase = { kind: 'idle' };
    const shown = useDocumentStore.getState().selection.ids.length > 0;
    host.renderer.update({
      handleMode: shown ? handlesAfterPick(transformMode()) : 'none',
      rotationCentre: null,
      nodeOverlay: null,
    });
  }

  deactivate(): void {
    this.cancel();
    this.host?.renderer.update({ hoverId: null });
    this.host = null;
  }

  overlay(): OverlayPainter | null {
    return null;
  }

  /** Handles for a fresh pick, with the rotation center back in the middle. */
  private showAfterPick(): void {
    this.host?.renderer.update({
      handleMode: handlesAfterPick(transformMode()),
      rotationCentre: null,
    });
  }

  private selection() {
    return useDocumentStore.getState().selection;
  }

  private handleAt(screen: Point2): HandleId | null {
    const host = this.host;
    if (host === null) {
      return null;
    }
    const { selectionFrame: frame, handleMode, rotationCentre: moved } = host.renderer.state;
    if (frame === null || handleMode === 'none') {
      return null;
    }
    const handles = selectionHandles(
      frame,
      host.renderer.state.camera,
      host.renderer.getViewport(),
      handleMode === 'pivot' ? rotationCentre(frame, moved) : null,
    );
    for (const handle of handles) {
      if (Math.hypot(handle.x - screen.x, handle.y - screen.y) <= HANDLE_HIT_PX) {
        return handle.id;
      }
    }
    return null;
  }

  pointerDown(p: PointerInfo): Promise<void> {
    return trackPick(this.press(p));
  }

  private async press(p: PointerInfo): Promise<void> {
    const host = this.host;
    if (host === null || p.button !== 0) {
      return;
    }
    this.pressed = true;
    const selection = this.selection();
    const handle = this.handleAt(p.screen);
    const frame = selectionFrame(selection);
    if (handle !== null && frame !== null) {
      if (handle === 'pivot') {
        this.phase = { kind: 'pivoting', frame, ids: selection.ids };
        host.setCursor(MOVE_CURSOR);
        return;
      }
      this.carried = carriedComponents(host);
      const anchor = anchorFor(handle, frame);
      if (handle === 'rotate') {
        const centre = rotationCentre(frame, host.renderer.state.rotationCentre);
        const startAngle = Math.atan2(p.job.y - centre.y, p.job.x - centre.x);
        this.phase = {
          kind: 'rotating',
          centre,
          startAngle,
          ids: selection.ids,
          matrix: [1, 0, 0, 1, 0, 0],
        };
      } else {
        this.phase = {
          kind: 'scaling',
          handle,
          frame,
          anchor,
          start: toFrame(frame, p.job),
          ids: selection.ids,
          matrix: [1, 0, 0, 1, 0, 0],
        };
      }
      return;
    }

    const tolerance = PICK_TOLERANCE_PX * host.mmPerPx();
    const { ids } = await api.entities.hitTest({ x: p.job.x, y: p.job.y, tolerance });
    const hit = ids[0];
    const store = useDocumentStore.getState();
    if (hit === undefined) {
      // A component preview under the pointer is picked like a vector.
      if (await pickComponentAt(p.job.x, p.job.y, p.shift || p.ctrl)) {
        this.showAfterPick();
        if (this.pressed) {
          this.phase = { kind: 'pendingDrag', ids: [], grab: p.job, start: p.screen };
        }
        host.requestDraw();
        return;
      }
      if (!p.shift && !p.ctrl && selection.ids.length > 0) {
        await store.clearSelection();
      }
      host.renderer.update({ handleMode: 'none' });
      if (!this.pressed) {
        host.requestDraw();
        return;
      }
      this.phase = { kind: 'band', from: p.job, shift: p.shift, ctrl: p.ctrl };
      host.renderer.update({ rubberBand: { from: p.job, to: p.job, mode: 'inside' } });
      host.requestDraw();
      return;
    }

    const wasSelected = selection.ids.includes(hit);
    if (p.shift) {
      await store.select([hit], 'add');
    } else if (p.ctrl) {
      await store.select([hit], 'toggle');
    } else if (wasSelected) {
      host.renderer.update({ handleMode: handlesAfterReclick(host.renderer.state.handleMode) });
    } else {
      clearPickedComponents();
      await store.select([hit], 'set');
      this.showAfterPick();
    }
    const after = useDocumentStore.getState().selection;
    if (this.pressed && after.ids.includes(hit)) {
      this.phase = { kind: 'pendingDrag', ids: after.ids, grab: p.job, start: p.screen };
    }
    host.requestDraw();
  }

  pointerMove(p: PointerInfo): void {
    const host = this.host;
    if (host === null) {
      return;
    }
    const phase = this.phase;
    switch (phase.kind) {
      case 'idle': {
        const handle = this.handleAt(p.screen);
        if (handle !== null) {
          host.setCursor(handleCursor(handle));
        }
        this.scheduleHover(p, handle !== null);
        return;
      }
      case 'pendingDrag':
        if (
          Math.hypot(p.screen.x - phase.start.x, p.screen.y - phase.start.y) >= DRAG_THRESHOLD_PX
        ) {
          this.carried = carriedComponents(host);
          this.phase = {
            kind: 'moving',
            ids: phase.ids,
            grab: phase.grab,
            last: phase.grab,
            matrix: [1, 0, 0, 1, 0, 0],
            copy: p.ctrl,
          };
          host.setCursor('move');
          this.pointerMove(p);
        }
        return;
      case 'moving': {
        host.probe(p.job, phase.ids);
        phase.last = p.snapped;
        phase.copy = p.ctrl;
        const view = useViewStore.getState();
        const snapped = p.snapped.x !== p.job.x || p.snapped.y !== p.job.y;
        const move = dragDelta(p.snapped.x - phase.grab.x, p.snapped.y - phase.grab.y, {
          alt: p.alt,
          stepMm: view.snapDistances && !snapped ? distanceStep(host.mmPerPx()) : null,
        });
        const matrix = translation(move.dx, move.dy);
        phase.matrix = matrix;
        host.renderer.update({ dragPreview: this.preview(phase.ids, matrix) });
        host.requestDraw();
        return;
      }
      case 'band': {
        const mode = p.job.x >= phase.from.x ? 'inside' : 'touch';
        host.renderer.update({ rubberBand: { from: phase.from, to: p.job, mode } });
        host.requestDraw();
        return;
      }
      case 'scaling': {
        host.probe(p.job, phase.ids);
        const { start, handle, frame } = phase;
        const corner = handle !== 'n' && handle !== 's' && handle !== 'e' && handle !== 'w';
        // Shift on a corner scales about the middle of the selection.
        const anchor = corner && p.shift ? anchorFor('rotate', frame) : phase.anchor;
        const at = toFrame(frame, p.snapped);
        const safe = (value: number) => (Math.abs(value) < 1e-9 ? 1e-9 : value);
        const { sx, sy } = scaleFactors(
          corner ? 'corner' : handle === 'e' || handle === 'w' ? 'horizontalEdge' : 'verticalEdge',
          (at.x - anchor.x) / safe(start.x - anchor.x),
          (at.y - anchor.y) / safe(start.y - anchor.y),
          { alt: p.alt, shift: p.shift },
        );
        if (Math.abs(sx) < 0.01 || Math.abs(sy) < 0.01) {
          return;
        }
        const matrix = inFrame(frame, scaleAbout(sx, sy, anchor.x, anchor.y));
        phase.matrix = matrix;
        host.renderer.update({ dragPreview: this.preview(phase.ids, matrix) });
        host.requestDraw();
        return;
      }
      case 'pivoting': {
        // Snaps like any drag, then to the box's corners, edge middles and
        // middle; Shift places it freely.
        host.probe(p.job, phase.ids);
        const at = p.shift
          ? p.job
          : snapPivot(phase.frame, p.snapped, PIVOT_SNAP_PX * host.mmPerPx());
        host.renderer.update({ rotationCentre: at });
        host.requestDraw();
        return;
      }
      case 'rotating': {
        const { centre, startAngle } = phase;
        let degrees =
          ((Math.atan2(p.job.y - centre.y, p.job.x - centre.x) - startAngle) * 180) / Math.PI;
        degrees = rotationStep(degrees, p.alt);
        const matrix = rotationAbout(degrees, centre.x, centre.y);
        phase.matrix = matrix;
        host.renderer.update({ dragPreview: this.preview(phase.ids, matrix) });
        host.requestDraw();
      }
    }
  }

  pointerUp(p: PointerInfo): Promise<void> {
    return trackPick(this.release(p));
  }

  private async release(p: PointerInfo): Promise<void> {
    const host = this.host;
    if (host === null) {
      return;
    }
    this.pressed = false;
    const phase = this.phase;
    this.phase = { kind: 'idle' };
    clearQuickKeys();
    host.setCursor(this.cursor);
    host.clearSnap();
    const store = useDocumentStore.getState();
    switch (phase.kind) {
      case 'moving':
      case 'scaling':
      case 'rotating': {
        host.renderer.update({ dragPreview: null });
        const m = phase.matrix;
        const identity =
          m[0] === 1 && m[1] === 0 && m[2] === 0 && m[3] === 1 && m[4] === 0 && m[5] === 0;
        if (!identity) {
          // Ctrl held when the drag ends leaves the originals and drops a copy.
          if (phase.kind === 'moving' && (phase.copy || p.ctrl)) {
            await store.transformSelectionCopy(m);
          } else {
            await store.transformSelection(m);
          }
        }
        host.renderer.update({
          handleMode:
            phase.kind === 'moving'
              ? handlesAfterMove(transformMode(), host.renderer.state.handleMode)
              : host.renderer.state.handleMode,
        });
        break;
      }
      case 'band': {
        host.renderer.update({ rubberBand: null });
        const rect = rectBetween(phase.from, p.job);
        const mode = p.job.x >= phase.from.x ? 'inside' : 'touch';
        const dragged =
          Math.abs(rect.maxX - rect.minX) > host.mmPerPx() * DRAG_THRESHOLD_PX ||
          Math.abs(rect.maxY - rect.minY) > host.mmPerPx() * DRAG_THRESHOLD_PX;
        if (dragged) {
          const { ids } = await api.entities.hitTest({
            rect: [rect.minX, rect.minY, rect.maxX, rect.maxY],
            mode,
          });
          if (ids.length > 0) {
            await store.select(ids, phase.shift ? 'add' : phase.ctrl ? 'toggle' : 'set');
            this.showAfterPick();
          }
        }
        break;
      }
      default:
        break;
    }
    host.requestDraw();
  }

  async doubleClick(p: PointerInfo): Promise<void> {
    const host = this.host;
    if (host === null) {
      return;
    }
    const tolerance = PICK_TOLERANCE_PX * host.mmPerPx();
    const { ids } = await api.entities.hitTest({ x: p.job.x, y: p.job.y, tolerance });
    const hit = ids[0];
    if (hit === undefined) {
      return;
    }
    await useDocumentStore.getState().select([hit], 'set');
    host.switchTool('node');
  }

  keyDown(event: KeyboardEvent): boolean {
    const kind = this.phase.kind;
    const dragging = kind === 'moving' || kind === 'scaling' || kind === 'rotating';
    return quickKeyDown(event, dragging, (entry) => void this.applyQuick(entry));
  }

  /** Finishes the current move, scale or rotate drag with typed values. */
  private async applyQuick(entry: QuickEntry): Promise<void> {
    const host = this.host;
    const phase = this.phase;
    const store = useDocumentStore.getState();
    const units = store.job.units;
    const toMm = (value: number) => toMillimetres(value, units);
    let matrix: Matrix | null = null;
    switch (phase.kind) {
      case 'moving': {
        const to = quickStepPoint(entry, phase.grab, phase.last, toMm);
        matrix = to === null ? null : translation(to.x - phase.grab.x, to.y - phase.grab.y);
        break;
      }
      case 'rotating': {
        const degrees = quickRotation(entry);
        matrix = degrees === null ? null : rotationAbout(degrees, phase.centre.x, phase.centre.y);
        break;
      }
      case 'scaling': {
        const bbox = phase.frame.extent;
        const axes =
          phase.handle === 'n' || phase.handle === 's'
            ? 'y'
            : phase.handle === 'e' || phase.handle === 'w'
              ? 'x'
              : 'both';
        const size = { width: bbox[2] - bbox[0], height: bbox[3] - bbox[1] };
        const factors = quickScale(entry, axes, size, toMm);
        matrix =
          factors === null
            ? null
            : inFrame(
                phase.frame,
                scaleAbout(factors.sx, factors.sy, phase.anchor.x, phase.anchor.y),
              );
        break;
      }
      default:
        return;
    }
    if (matrix === null) {
      return;
    }
    this.phase = { kind: 'idle' };
    if (host !== null) {
      host.renderer.update({ dragPreview: null });
      host.clearSnap();
      host.setCursor(this.cursor);
    }
    await store.transformSelection(matrix);
    host?.requestDraw();
  }

  /** The drag preview of `ids` and the carried components moved by `matrix`. */
  private preview(ids: readonly number[], matrix: Matrix): DragPreview {
    return { ids: new Set(ids), matrix, components: this.carried };
  }

  cancel(): void {
    const host = this.host;
    clearQuickKeys();
    this.phase = { kind: 'idle' };
    if (host !== null) {
      host.renderer.update({ dragPreview: null, rubberBand: null });
      host.clearSnap();
      host.setCursor(this.cursor);
      host.requestDraw();
    }
  }

  private scheduleHover(p: PointerInfo, onHandle: boolean): void {
    const host = this.host;
    if (host === null) {
      return;
    }
    if (this.hoverTimer !== null) {
      window.clearTimeout(this.hoverTimer);
    }
    const job = p.job;
    this.hoverTimer = window.setTimeout(() => {
      this.hoverTimer = null;
      void api.entities
        .hitTest({ x: job.x, y: job.y, tolerance: PICK_TOLERANCE_PX * host.mmPerPx() })
        .then(({ ids }) => {
          const next = ids[0] ?? null;
          if (!onHandle && this.phase.kind === 'idle') {
            const selected = next !== null && this.selection().ids.includes(next);
            host.setCursor(selected ? MOVE_CURSOR : SELECT_CURSOR);
          }
          if (host.renderer.state.hoverId !== next) {
            host.renderer.update({ hoverId: next });
            host.requestDraw();
          }
        });
    }, 30);
  }
}
