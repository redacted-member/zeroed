// Node editing a live distortion: while the selected objects still carry a
// distortion (of any mode), the node tool edits its envelope instead of the
// vectors. Drag a node (a corner moves both of its edges), I inserts a node
// on the edge under the pointer, D deletes the node under it, L and C make
// the span under it a line or a curve; every edit bends the objects again
// at once. An edited curve mode envelope goes on as a Bounding Box one.

import { api } from '../../../bridge/formApi';
import type { StoredDistortion } from '../../../bridge/api';
import type { Path } from '../../../bridge/document';
import {
  envelopeKeyEdit,
  envelopeNodeAt,
  envelopeNodes,
  envelopeOutline,
  fromEnvelopeFrame,
  moveNode,
  toEnvelopeFrame,
  withSides,
  type Envelope,
  type EnvelopeNodeRef,
} from '../../../lib/envelope';
import { useDocumentStore } from '../../../state/documentStore';
import type { Point2 } from '../camera';
import type { OverlayPainter } from '../renderer';
import { NODE_HIT_PX, type PointerInfo, type ToolHost } from './types';

/** The envelope of a stored distortion, sides as node lists. */
export function envelopeOf(stored: StoredDistortion): Envelope {
  const envelope: Envelope = { bbox: stored.bbox, bottom: stored.bottom, top: stored.top };
  if (stored.left !== undefined && stored.right !== undefined) {
    envelope.left = stored.left;
    envelope.right = stored.right;
  }
  if (stored.lines !== undefined) {
    envelope.lines = stored.lines;
  }
  return withSides(envelope);
}

/** The live distortion of exactly `ids`, of any mode, or null. */
export async function liveDistortionOf(ids: readonly number[]): Promise<StoredDistortion | null> {
  if (ids.length === 0) {
    return null;
  }
  const reply = await api.ops.distortion([...ids]).catch(() => ({ distortion: null }));
  return reply.distortion;
}

function nodeKey(ref: EnvelopeNodeRef): string {
  return `${ref.edge}:${ref.index}`;
}

export class EnvelopeEditor {
  private envelope: Envelope;
  /** The envelope the objects were last bent with. */
  private applied: Envelope;
  private dragged: EnvelopeNodeRef | null = null;
  private selected: EnvelopeNodeRef | null = null;
  private moved = false;
  private readonly unsubscribe: () => void;

  constructor(
    private readonly host: ToolHost,
    private stored: StoredDistortion,
  ) {
    this.envelope = envelopeOf(stored);
    this.applied = this.envelope;
    this.refresh();
    // Undo, redo or a bake elsewhere changes the distortion under the tool.
    this.unsubscribe = useDocumentStore.subscribe((state, before) => {
      if (state.revision !== before.revision) {
        void this.reload();
      }
    });
  }

  /** Stops following the document. */
  dispose(): void {
    this.unsubscribe();
  }

  /** Reads the distortion again; leaves node editing when it has gone. */
  private async reload(): Promise<void> {
    if (this.dragged !== null) {
      return;
    }
    const stored = await liveDistortionOf(this.stored.ids);
    if (stored === null) {
      this.host.switchTool('select');
      return;
    }
    this.stored = stored;
    this.envelope = envelopeOf(stored);
    this.applied = this.envelope;
    this.refresh();
  }

  /** The distorted objects. */
  get ids(): number[] {
    return this.stored.ids;
  }

  private toJob(p: { x: number; y: number }): Point2 {
    return fromEnvelopeFrame(p, this.stored.angle);
  }

  private toFrame(p: Point2): Point2 {
    return toEnvelopeFrame(p, this.stored.angle);
  }

  private tolerance(): number {
    return NODE_HIT_PX * this.host.mmPerPx();
  }

  private refresh(): void {
    const selected = this.selected === null ? null : nodeKey(this.selected);
    this.host.renderer.update({
      nodeOverlay: {
        entityId: this.stored.ids[0] ?? 0,
        nodes: envelopeNodes(this.envelope).map((node) => {
          const at = this.toJob(node.point);
          const key = nodeKey(node);
          return {
            key,
            x: at.x,
            y: at.y,
            smooth: true,
            start: false,
            selected: key === selected,
          };
        }),
        handles: [],
      },
    });
    this.host.requestDraw();
  }

  /** The envelope drawn dashed, in job space. */
  overlay(): OverlayPainter {
    const outline: Path = envelopeOutline(this.envelope);
    const sub = outline.subpaths[0];
    return (context, toScreen, palette) => {
      if (sub === undefined) {
        return;
      }
      context.save();
      context.strokeStyle = palette.selection;
      context.lineWidth = 1;
      context.setLineDash([4, 3]);
      context.beginPath();
      const start = toScreen(this.toJob(sub.start));
      context.moveTo(start.x, start.y);
      for (const segment of sub.segments) {
        const p = toScreen(this.toJob(segment.to));
        context.lineTo(p.x, p.y);
      }
      context.closePath();
      context.stroke();
      context.restore();
    };
  }

  /** True when the press landed on a node (it is then dragged). */
  pointerDown(p: PointerInfo): boolean {
    const node = envelopeNodeAt(this.envelope, this.toFrame(p.job), this.tolerance());
    this.dragged = node;
    this.selected = node;
    this.moved = false;
    this.refresh();
    return node !== null;
  }

  pointerMove(p: PointerInfo): void {
    const node = this.dragged;
    if (node === null) {
      this.host.setCursor(
        envelopeNodeAt(this.envelope, this.toFrame(p.job), this.tolerance()) !== null
          ? 'pointer'
          : 'default',
      );
      return;
    }
    this.moved = true;
    this.envelope = moveNode(this.envelope, node.edge, node.index, this.toFrame(p.snapped));
    this.refresh();
  }

  async pointerUp(): Promise<void> {
    const dragged = this.dragged !== null && this.moved;
    this.dragged = null;
    this.moved = false;
    if (dragged) {
      await this.apply();
    }
  }

  /** The node editing keys; true when the key was used. */
  async keyDown(key: string, pointer: Point2 | null): Promise<boolean> {
    const at = pointer === null ? null : this.toFrame(this.host.renderer.pointAt(pointer));
    const next = envelopeKeyEdit(this.envelope, key, at, this.tolerance(), this.selected);
    if (next === null) {
      return false;
    }
    this.envelope = next;
    this.selected = null;
    this.refresh();
    await this.apply();
    return true;
  }

  cancel(): void {
    this.dragged = null;
    this.moved = false;
    this.envelope = this.applied;
    this.refresh();
  }

  /** Bends the objects with the edited envelope, one undo step. */
  private async apply(): Promise<void> {
    const { bbox, bottom, top, left, right, lines } = this.envelope;
    const store = useDocumentStore;
    try {
      await api.ops.distort({
        ids: this.stored.ids,
        componentIds: [],
        bbox,
        envelope: {
          bottom,
          top,
          ...(left !== undefined && right !== undefined ? { left, right } : {}),
          ...(lines !== undefined ? { lines } : {}),
        },
        ...(this.stored.angle !== 0 ? { angle: this.stored.angle } : {}),
      });
      store.setState({ lastError: null });
      this.applied = this.envelope;
    } catch (error) {
      store.setState({ lastError: error instanceof Error ? error.message : String(error) });
      this.envelope = this.applied;
    }
    await store.getState().refresh();
    this.refresh();
  }
}
