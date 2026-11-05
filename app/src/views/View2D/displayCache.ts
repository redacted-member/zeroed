// Keeps the flattened display geometry of every entity, as Path2D objects in
// job millimetres, so the renderer never rebuilds paths between frames and
// never draws from a JSON copy of the document.

import { api } from '../../bridge/api';
import type { Bbox, EntityKind } from '../../bridge/document';

export interface DisplayEntry {
  id: number;
  revision: number;
  layer: number;
  kind: EntityKind;
  closed: boolean;
  selected: boolean;
  /** Flat x, y pairs per run (subpath). */
  runs: Float32Array[];
  path: Path2D;
  bbox: Bbox;
  /** The image a bitmap entity places, and how opaque it is drawn. */
  imageId?: number;
  opacity?: number;
}

function bboxOfRuns(runs: Float32Array[]): Bbox {
  const bbox: Bbox = [Infinity, Infinity, -Infinity, -Infinity];
  for (const run of runs) {
    for (let i = 0; i + 1 < run.length; i += 2) {
      const x = run[i] ?? 0;
      const y = run[i + 1] ?? 0;
      bbox[0] = Math.min(bbox[0], x);
      bbox[1] = Math.min(bbox[1], y);
      bbox[2] = Math.max(bbox[2], x);
      bbox[3] = Math.max(bbox[3], y);
    }
  }
  return bbox;
}

export function buildPath2D(runs: Float32Array[], closed: boolean): Path2D {
  const path = new Path2D();
  for (const run of runs) {
    if (run.length < 2) {
      continue;
    }
    path.moveTo(run[0] ?? 0, run[1] ?? 0);
    for (let i = 2; i + 1 < run.length; i += 2) {
      path.lineTo(run[i] ?? 0, run[i + 1] ?? 0);
    }
    if (closed) {
      path.closePath();
    }
  }
  return path;
}

export class DisplayCache {
  private readonly entries = new Map<number, DisplayEntry>();
  private order: number[] = [];
  private revision = -1;
  private pending: Promise<void> | null = null;
  /** The latest call that came while a fetch ran: it runs next, as asked. */
  private queued: { revision: number; tolerance: number; force: boolean } | null = null;
  private trailing: Promise<void> | null = null;

  /** Entities in drawing order. */
  list(): readonly DisplayEntry[] {
    return this.order
      .map((id) => this.entries.get(id))
      .filter((e): e is DisplayEntry => e !== undefined);
  }

  get(id: number): DisplayEntry | undefined {
    return this.entries.get(id);
  }

  loadedRevision(): number {
    return this.revision;
  }

  /**
   * Brings the cache to the given document revision. Overlapping calls are
   * coalesced: one fetch runs at a time and a trailing one follows with the
   * newest revision and tolerance asked for, forced when any waiting call
   * was (a zoom that needs finer curves must not be lost behind an edit).
   */
  sync(revision: number, tolerance: number, force = false): Promise<void> {
    if (!force && revision === this.revision && this.pending === null) {
      return Promise.resolve();
    }
    if (this.pending !== null) {
      this.queued = {
        revision: Math.max(revision, this.queued?.revision ?? revision),
        tolerance,
        force: force || this.queued?.force === true,
      };
      // Every waiting caller resolves once the trailing fetch is in.
      this.trailing ??= this.pending
        .catch(() => undefined)
        .then(() => {
          this.trailing = null;
          const next = this.queued;
          this.queued = null;
          return next === null ? undefined : this.sync(next.revision, next.tolerance, next.force);
        });
      return this.trailing;
    }
    this.pending = this.fetch(revision, tolerance).finally(() => {
      this.pending = null;
    });
    return this.pending;
  }

  private async fetch(revision: number, tolerance: number): Promise<void> {
    const reply = await api.entities.display({ tolerance });
    const seen = new Set<number>();
    const order: number[] = [];
    for (const entity of reply.entities) {
      const runs = entity.runs.map((run) =>
        reply.xy.slice(run.offset * 2, (run.offset + run.count) * 2),
      );
      this.entries.set(entity.id, {
        id: entity.id,
        revision: entity.revision,
        layer: entity.layer,
        kind: entity.kind,
        closed: entity.closed,
        selected: entity.selected,
        runs,
        path: buildPath2D(runs, entity.closed),
        bbox: bboxOfRuns(runs),
        imageId: entity.imageId,
        opacity: entity.opacity,
      });
      seen.add(entity.id);
      order.push(entity.id);
    }
    for (const id of [...this.entries.keys()]) {
      if (!seen.has(id)) {
        this.entries.delete(id);
      }
    }
    this.order = order;
    this.revision = Math.max(revision, reply.revision);
  }
}
