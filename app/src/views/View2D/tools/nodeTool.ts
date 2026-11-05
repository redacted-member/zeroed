// Node editing tool: drag nodes (every selected one together, mirrored in
// H / V mode), bezier handles and spans (Ctrl moves a whole span); drag on
// empty space to select nodes in a box; insert, delete, smooth, convert,
// cut, set the start point, line nodes up (X / Y), insert a midpoint and
// delete a span on the selected vector. K keeps curve end directions while a
// span is dragged; J joins an open end to the nearest other open end.

import { api } from '../../../bridge/api';
import type { EntityJson, Path } from '../../../bridge/document';
import { flattenPath } from '../../../lib/geometry';
import {
  alignNodes,
  closeSubpaths,
  convertSegment,
  deleteSpan,
  dragSpan,
  insertMidpoint,
  mirrorNode,
  moveNodes,
  cutAtNode,
  deleteNode,
  insertNode,
  listHandles,
  listNodes,
  moveHandle,
  moveNode,
  nearestSegment,
  nodePosition,
  reversePath,
  setStartNode,
  toggleSmooth,
  type ControlHandle,
  type NodeRef,
  type PathNode,
  type SegmentRef,
} from '../../../lib/pathEdit';
import { useDocumentStore } from '../../../state/documentStore';
import { useNodeToolStore } from '../../../state/nodeToolStore';
import type { Point2 } from '../camera';
import type { OverlayPainter } from '../renderer';
import {
  NODE_HIT_PX,
  PICK_TOLERANCE_PX,
  type PointerInfo,
  type ToolController,
  type ToolHost,
} from './types';
import { NODE_CURSOR } from './cursors';
import type { EnvelopeEditor } from './envelopeEdit';

type Drag =
  | { kind: 'none' }
  | { kind: 'node'; ref: NodeRef; original: Path; from: Point2 }
  | { kind: 'handle'; handle: ControlHandle; original: Path }
  | { kind: 'span'; ref: SegmentRef; t: number; original: Path; from: Point2 }
  | { kind: 'band'; from: Point2; to: Point2; add: boolean };

function refKey(ref: NodeRef): string {
  return `${ref.subpath}:${ref.node}`;
}

export class NodeTool implements ToolController {
  readonly id = 'node' as const;
  readonly cursor = NODE_CURSOR;
  private host: ToolHost | null = null;
  private entity: EntityJson | null = null;
  private path: Path | null = null;
  private drag: Drag = { kind: 'none' };
  private selectedNodes = new Set<string>();
  private hoverSegment: SegmentRef | null = null;
  private hoverNode: NodeRef | null = null;
  private preview: Path | null = null;
  /** H or V mirror mode: node moves are mirrored across the vector's centre. */
  private mirror: 'h' | 'v' | null = null;
  /** K mode: dragging a curve span keeps the directions at its two nodes. */
  private keepTangency = false;
  /** Set while the selection carries a live distortion: its envelope is
   * node edited instead of the vectors. */
  private envelope: EnvelopeEditor | null = null;

  async activate(host: ToolHost): Promise<void> {
    this.host = host;
    host.renderer.update({ handleMode: 'none', hoverId: null });
    const selected = useDocumentStore.getState().selection.ids;
    if (await this.loadEnvelope(selected)) {
      return;
    }
    const id = selected.length === 1 ? selected[0] : undefined;
    if (id === undefined || !(await this.load(id))) {
      host.switchTool('select');
    }
  }

  /** Starts editing the envelope when `ids` carry a live distortion. */
  private async loadEnvelope(ids: readonly number[]): Promise<boolean> {
    const host = this.host;
    // Loaded on first use: distortions are rare, the start bundle small.
    const { liveDistortionOf, EnvelopeEditor } = await import('./envelopeEdit');
    const stored = await liveDistortionOf(ids);
    if (host === null || stored === null) {
      this.envelope?.dispose();
      this.envelope = null;
      return false;
    }
    this.entity = null;
    this.path = null;
    this.selectedNodes.clear();
    this.envelope?.dispose();
    this.envelope = new EnvelopeEditor(host, stored);
    return true;
  }

  /** Clicking another object moves node editing onto it. */
  private async pickOther(id: number): Promise<void> {
    const host = this.host;
    if (host === null) {
      return;
    }
    await useDocumentStore.getState().select([id], 'set');
    if (!(await this.loadEnvelope([id])) && !(await this.load(id))) {
      host.switchTool('select');
    }
  }

  private async load(id: number): Promise<boolean> {
    const entity = await api.entities.get(id);
    if (entity.kind !== 'path') {
      return false;
    }
    this.envelope?.dispose();
    this.envelope = null;
    this.entity = entity;
    this.path = entity.path;
    this.selectedNodes.clear();
    this.refreshOverlay();
    return true;
  }

  deactivate(): void {
    this.cancel();
    useNodeToolStore.getState().setHover(null);
    useNodeToolStore.getState().closeProperties();
    this.host?.renderer.update({ nodeOverlay: null });
    this.envelope?.dispose();
    this.envelope = null;
    this.entity = null;
    this.path = null;
    this.host = null;
  }

  overlay(): OverlayPainter | null {
    if (this.envelope !== null) {
      return this.envelope.overlay();
    }
    const drag = this.drag;
    if (drag.kind === 'band') {
      return (context, toScreen, palette) => {
        const a = toScreen(drag.from);
        const b = toScreen(drag.to);
        context.save();
        context.strokeStyle = palette.selection;
        context.setLineDash([4, 3]);
        context.strokeRect(
          Math.min(a.x, b.x),
          Math.min(a.y, b.y),
          Math.abs(b.x - a.x),
          Math.abs(b.y - a.y),
        );
        context.restore();
      };
    }
    const preview = this.preview;
    if (preview === null) {
      return null;
    }
    return (context, toScreen, palette) => {
      context.save();
      context.strokeStyle = palette.selection;
      context.lineWidth = 1;
      context.setLineDash([4, 3]);
      for (const sub of flattenPath(preview, 0.1)) {
        context.beginPath();
        for (let i = 0; i + 1 < sub.points.length; i += 2) {
          const p = toScreen({ x: sub.points[i] ?? 0, y: sub.points[i + 1] ?? 0 });
          if (i === 0) {
            context.moveTo(p.x, p.y);
          } else {
            context.lineTo(p.x, p.y);
          }
        }
        if (sub.closed) {
          context.closePath();
        }
        context.stroke();
      }
      context.restore();
    };
  }

  private refreshOverlay(path: Path | null = this.path): void {
    const host = this.host;
    if (host === null || path === null || this.entity === null) {
      return;
    }
    host.renderer.update({
      nodeOverlay: {
        entityId: this.entity.id,
        nodes: listNodes(path).map((node) => ({
          key: refKey(node),
          x: node.x,
          y: node.y,
          smooth: node.smooth,
          start: node.start,
          selected: this.selectedNodes.has(refKey(node)),
        })),
        handles: listHandles(path).map((handle) => ({
          key: `${handle.subpath}:${handle.segment}:${handle.which}`,
          x: handle.x,
          y: handle.y,
          anchorX: handle.anchorX,
          anchorY: handle.anchorY,
        })),
      },
    });
    host.requestDraw();
  }

  private nodeAt(screen: Point2): PathNode | null {
    const host = this.host;
    if (host === null || this.path === null) {
      return null;
    }
    let best: { node: PathNode; dist: number } | null = null;
    for (const node of listNodes(this.path)) {
      const s = host.renderer.toScreen(node);
      const dist = Math.hypot(s.x - screen.x, s.y - screen.y);
      if (dist <= NODE_HIT_PX && (best === null || dist < best.dist)) {
        best = { node, dist };
      }
    }
    return best?.node ?? null;
  }

  private handleAt(screen: Point2): ControlHandle | null {
    const host = this.host;
    if (host === null || this.path === null) {
      return null;
    }
    for (const handle of listHandles(this.path)) {
      const s = host.renderer.toScreen(handle);
      if (Math.hypot(s.x - screen.x, s.y - screen.y) <= NODE_HIT_PX) {
        return handle;
      }
    }
    return null;
  }

  async pointerDown(p: PointerInfo): Promise<void> {
    const host = this.host;
    const envelope = this.envelope;
    if (host !== null && envelope !== null && p.button === 0) {
      if (envelope.pointerDown(p)) {
        return;
      }
      const tolerance = PICK_TOLERANCE_PX * host.mmPerPx();
      const { ids } = await api.entities.hitTest({ x: p.job.x, y: p.job.y, tolerance });
      const hit = ids[0];
      if (hit !== undefined && !envelope.ids.includes(hit)) {
        await this.pickOther(hit);
      }
      return;
    }
    if (host === null || this.path === null || p.button !== 0) {
      return;
    }
    const node = this.nodeAt(p.screen);
    if (node !== null) {
      const key = refKey(node);
      if (p.shift) {
        if (this.selectedNodes.has(key)) {
          this.selectedNodes.delete(key);
        } else {
          this.selectedNodes.add(key);
        }
      } else if (!this.selectedNodes.has(key)) {
        this.selectedNodes = new Set([key]);
      }
      this.drag = { kind: 'node', ref: node, original: this.path, from: p.job };
      this.refreshOverlay();
      return;
    }
    const handle = this.handleAt(p.screen);
    if (handle !== null) {
      this.drag = { kind: 'handle', handle, original: this.path };
      return;
    }
    // Dragging a span of this vector reshapes it.
    const span = nearestSegment(this.path, p.job, 0.1);
    if (span !== null && span.dist <= NODE_HIT_PX * host.mmPerPx()) {
      this.drag = { kind: 'span', ref: span.ref, t: span.t, original: this.path, from: p.job };
      return;
    }
    // Clicking another vector moves node editing onto it.
    const tolerance = PICK_TOLERANCE_PX * host.mmPerPx();
    const { ids } = await api.entities.hitTest({ x: p.job.x, y: p.job.y, tolerance });
    const hit = ids[0];
    if (hit !== undefined && hit !== this.entity?.id) {
      await this.pickOther(hit);
      return;
    }
    if (hit === undefined) {
      if (!p.shift) {
        this.selectedNodes.clear();
      }
      // Dragging on empty space selects the nodes inside the box.
      this.drag = { kind: 'band', from: p.job, to: p.job, add: p.shift };
      this.refreshOverlay();
    }
  }

  /** The nodes a node drag moves: the selected ones with the grabbed one, and their mirrors. */
  private movedNodes(grabbed: NodeRef, path: Path): { refs: NodeRef[]; mirrored: NodeRef[] } {
    const refs = this.selectedNodes.has(refKey(grabbed))
      ? [...this.selectedNodes].map((key) => {
          const [subpath, node] = key.split(':').map(Number);
          return { subpath: subpath ?? 0, node: node ?? 0 };
        })
      : [grabbed];
    const mirror = this.mirror;
    const mirrored =
      mirror === null
        ? []
        : refs
            .map((ref) => mirrorNode(path, ref, mirror))
            .filter((ref): ref is NodeRef => ref !== null)
            .filter((ref) => !refs.some((r) => refKey(r) === refKey(ref)));
    return { refs, mirrored };
  }

  pointerMove(p: PointerInfo): void {
    const host = this.host;
    if (this.envelope !== null) {
      this.envelope.pointerMove(p);
      return;
    }
    if (host === null || this.path === null) {
      return;
    }
    const drag = this.drag;
    if (drag.kind === 'node') {
      host.probe(p.job, this.entity === null ? [] : [this.entity.id]);
      const { refs, mirrored } = this.movedNodes(drag.ref, drag.original);
      if (refs.length === 1 && mirrored.length === 0) {
        this.preview = moveNode(drag.original, drag.ref, p.snapped);
      } else {
        const by = { x: p.snapped.x - drag.from.x, y: p.snapped.y - drag.from.y };
        const flip = this.mirror === 'h' ? { x: -by.x, y: by.y } : { x: by.x, y: -by.y };
        this.preview = moveNodes(moveNodes(drag.original, refs, by), mirrored, flip);
      }
      this.refreshOverlay(this.preview);
      return;
    }
    if (drag.kind === 'span') {
      const by = { x: p.job.x - drag.from.x, y: p.job.y - drag.from.y };
      this.preview = dragSpan(drag.original, drag.ref, drag.t, by, p.ctrl, this.keepTangency);
      this.refreshOverlay(this.preview);
      return;
    }
    if (drag.kind === 'band') {
      drag.to = p.job;
      host.requestDraw();
      return;
    }
    if (drag.kind === 'handle') {
      this.preview = moveHandle(drag.original, drag.handle, p.job);
      this.refreshOverlay(this.preview);
      return;
    }
    const node = this.nodeAt(p.screen);
    this.hoverNode = node;
    const nearest = node === null ? nearestSegment(this.path, p.job, 0.1) : null;
    this.hoverSegment =
      nearest !== null && nearest.dist <= NODE_HIT_PX * host.mmPerPx() ? nearest.ref : null;
    useNodeToolStore
      .getState()
      .setHover(node !== null ? 'node' : this.hoverSegment !== null ? 'span' : null);
    host.setCursor(node !== null || this.handleAt(p.screen) !== null ? 'pointer' : this.cursor);
  }

  async pointerUp(): Promise<void> {
    if (this.envelope !== null) {
      await this.envelope.pointerUp();
      return;
    }
    const host = this.host;
    const drag = this.drag;
    this.drag = { kind: 'none' };
    if (host === null) {
      return;
    }
    host.clearSnap();
    if (drag.kind === 'band' && this.path !== null) {
      const [x0, x1] = [Math.min(drag.from.x, drag.to.x), Math.max(drag.from.x, drag.to.x)];
      const [y0, y1] = [Math.min(drag.from.y, drag.to.y), Math.max(drag.from.y, drag.to.y)];
      for (const node of listNodes(this.path)) {
        if (node.x >= x0 && node.x <= x1 && node.y >= y0 && node.y <= y1) {
          this.selectedNodes.add(refKey(node));
        }
      }
      this.refreshOverlay();
      host.requestDraw();
      return;
    }
    const edited = this.preview;
    this.preview = null;
    if (drag.kind !== 'none' && edited !== null) {
      await this.commit(edited);
    } else {
      this.refreshOverlay();
    }
  }

  private async commit(path: Path): Promise<void> {
    const host = this.host;
    const entity = this.entity;
    if (host === null || entity === null || entity.kind !== 'path') {
      return;
    }
    this.path = path;
    const next: EntityJson = { ...entity, path };
    try {
      await api.entities.replace(entity.id, next);
    } finally {
      await useDocumentStore.getState().refresh();
      this.entity = next;
      this.refreshOverlay();
    }
  }

  async keyDown(event: KeyboardEvent, pointer: Point2 | null): Promise<boolean> {
    const host = this.host;
    const key = event.key.toLowerCase();
    if (host !== null && this.envelope !== null) {
      if (key === 'escape' || key === 'n') {
        host.switchTool('select');
        return true;
      }
      return this.envelope.keyDown(key, pointer);
    }
    if (host === null || this.path === null) {
      return false;
    }
    const target = this.hoverNode ?? this.firstSelectedNode();
    switch (key) {
      case 'escape':
      case 'n':
        host.switchTool('select');
        return true;
      case 'i': {
        if (pointer === null) {
          return true;
        }
        const nearest = nearestSegment(this.path, host.renderer.pointAt(pointer), 0.1);
        if (nearest !== null && nearest.dist <= NODE_HIT_PX * host.mmPerPx() * 2) {
          await this.commit(insertNode(this.path, nearest.ref, nearest.t));
        }
        return true;
      }
      case 'd': {
        // Delete Point over a node (or the selected one), else the span under
        // the pointer.
        const span = this.hoverNode === null ? this.hoverSegment : null;
        if (span !== null) {
          await this.commit(deleteSpan(this.path, span));
          this.selectedNodes.clear();
        } else if (target !== null) {
          await this.commit(deleteNode(this.path, target));
          this.selectedNodes.clear();
        }
        return true;
      }
      case 's':
        if (target !== null) {
          await this.commit(toggleSmooth(this.path, target));
        }
        return true;
      case 'c':
        if (target !== null) {
          await this.commit(cutAtNode(this.path, target));
        }
        return true;
      case 'p':
        if (target !== null) {
          await this.commit(setStartNode(this.path, target));
        }
        return true;
      case 'l':
      case 'a':
      case 'b': {
        const segment = this.hoverSegment;
        if (segment !== null) {
          const to = key === 'l' ? 'line' : key === 'a' ? 'arc' : 'cubic';
          await this.commit(convertSegment(this.path, segment, to));
        }
        return true;
      }
      case 'x':
      case 'y': {
        const refs = [...this.selectedNodes].map((k) => {
          const [subpath, node] = k.split(':').map(Number);
          return { subpath: subpath ?? 0, node: node ?? 0 };
        });
        if (refs.length > 1) {
          await this.commit(alignNodes(this.path, refs, key));
        } else if (key === 'x' && target !== null) {
          this.openProperties(target);
        }
        return true;
      }
      case 'properties':
        if (target !== null) {
          this.openProperties(target);
        }
        return true;
      case 'reverse':
        await this.commit(reversePath(this.path));
        return true;
      case 'close':
        await this.commit(closeSubpaths(this.path, (target ?? this.hoverSegment)?.subpath));
        this.selectedNodes.clear();
        return true;
      case 'h':
      case 'v':
        this.mirror = this.mirror === key ? null : key;
        return true;
      case 'k':
        this.keepTangency = !this.keepTangency;
        return true;
      case 'j':
        if (target !== null) {
          await this.joinEnd(target);
        }
        return true;
      case 'insert-midpoint': {
        const segment = this.hoverSegment;
        if (segment !== null) {
          await this.commit(insertMidpoint(this.path, segment));
        }
        return true;
      }
      case 'delete-span': {
        const segment = this.hoverSegment;
        if (segment !== null) {
          await this.commit(deleteSpan(this.path, segment));
          this.selectedNodes.clear();
        }
        return true;
      }
      default: {
        // The Properties box sends the typed position as node-xy:x,y.
        const typed = /^node-xy:(-?[\d.e+-]+),(-?[\d.e+-]+)$/.exec(key);
        const ref = this.propertiesNode;
        if (typed !== null && ref !== null) {
          const [x, y] = [Number(typed[1]), Number(typed[2])];
          if (Number.isFinite(x) && Number.isFinite(y)) {
            await this.commit(moveNode(this.path, ref, { x, y }));
          }
          return true;
        }
        return false;
      }
    }
  }

  /** The node the Properties box is open for. */
  private propertiesNode: NodeRef | null = null;

  private openProperties(ref: NodeRef): void {
    const at = this.path === null ? null : nodePosition(this.path, ref);
    if (at !== null) {
      this.propertiesNode = ref;
      useNodeToolStore.getState().openProperties(at);
    }
  }

  /** Joins an open end node to the nearest other open end (kernel side, one undo step). */
  private async joinEnd(ref: NodeRef): Promise<void> {
    const entity = this.entity;
    const sub = this.path?.subpaths[ref.subpath];
    if (entity === null || sub === undefined || sub.closed) {
      return;
    }
    const last = sub.segments.length;
    if (ref.node !== 0 && ref.node !== last) {
      return;
    }
    const store = useDocumentStore;
    try {
      await api.ops.node.join(entity.id, ref.subpath, ref.node === 0);
      store.setState({ lastError: null });
    } catch (error) {
      store.setState({ lastError: error instanceof Error ? error.message : String(error) });
    }
    await store.getState().refresh();
    await this.load(entity.id);
  }

  private firstSelectedNode(): NodeRef | null {
    const first = [...this.selectedNodes][0];
    if (first === undefined) {
      return null;
    }
    const [subpath, node] = first.split(':').map(Number);
    return subpath === undefined || node === undefined ? null : { subpath, node };
  }

  cancel(): void {
    this.envelope?.cancel();
    this.drag = { kind: 'none' };
    this.preview = null;
    this.host?.clearSnap();
    this.refreshOverlay();
  }
}
