// Guide lines belong to the job: they are saved in the .carve file, a new
// job starts without any and opening a job shows its own. The 2D view keeps
// them in viewStore; this keeps the kernel's copy the same and reloads the
// view's when another job takes the open one's place.

import { api } from '../bridge/api';
import type { Guide, GuideLine } from '../bridge/document';
import type { Unsubscribe } from '../bridge/types';
import { useDocumentStore } from './documentStore';
import { useViewStore } from './viewStore';

/** The guides as the job file keeps them. */
export function guidesForJob(guides: readonly Guide[]): GuideLine[] {
  return guides.map(({ axis, position, locked }) => ({ axis, position, locked }));
}

/** A job's guides as the view keeps them, numbered from `firstId`. */
export function guidesFromJob(lines: readonly GuideLine[], firstId: number): Guide[] {
  return lines.map((line, i) => ({ id: firstId + i, ...line }));
}

/** Shows the open job's guides in place of any shown. */
export async function reloadGuides(): Promise<void> {
  const { guides } = await api.doc.guides();
  const view = useViewStore.getState();
  view.loadGuides(guidesFromJob(guides, view.nextGuideId));
}

/**
 * Sends every change of the view's guides to the kernel (one request in
 * flight, the latest waiting) and reloads them whenever another job opens.
 */
export function watchGuides(): Unsubscribe {
  let sending = false;
  let waiting: GuideLine[] | null = null;
  const send = (lines: GuideLine[]) => {
    if (sending) {
      waiting = lines;
      return;
    }
    sending = true;
    void api.doc
      .setGuides(lines)
      .then(({ dirty }) => {
        if (dirty !== useDocumentStore.getState().dirty) {
          useDocumentStore.setState({ dirty });
        }
      })
      .catch(() => undefined)
      .finally(() => {
        sending = false;
        const next = waiting;
        waiting = null;
        if (next !== null) {
          send(next);
        }
      });
  };
  const stopView = useViewStore.subscribe((view, previous) => {
    if (view.guides !== previous.guides) {
      send(guidesForJob(view.guides));
    }
  });
  const stopDocument = useDocumentStore.subscribe((doc, previous) => {
    if (doc.generation !== previous.generation) {
      void reloadGuides().catch(() => undefined);
    }
  });
  // The job may already be open when the watch starts.
  if (useDocumentStore.getState().kernelStatus === 'ready') {
    void reloadGuides().catch(() => undefined);
  }
  return () => {
    stopView();
    stopDocument();
  };
}
