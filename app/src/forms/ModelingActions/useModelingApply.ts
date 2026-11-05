// Apply, Reset and Start New Component for a modeling tool: each Apply
// replaces the one before it (one undo step, same component), Reset takes
// the form's change back, Start New Component keeps it and clears the
// selection so the next Apply makes another component. Closing the form
// keeps what was applied.

import { useRef, useState } from 'react';

import type { AppliedComponent, ApplyTarget } from '../../bridge/model';
import { usePreviewSession } from '../../lib/usePreviewSession';
import { useDocumentStore } from '../../state/documentStore';
import { useModelStore } from '../../state/modelStore';
import {
  afterApply,
  afterReset,
  afterStartNew,
  applyTarget,
  startTrack,
  type ApplyTrack,
} from './model';

export interface ModelingApply {
  track: ApplyTrack;
  /** Runs the tool's kernel call with the target and step to replace. */
  apply: (call: (target: ApplyTarget) => Promise<AppliedComponent>) => Promise<void>;
  reset: () => Promise<void>;
  startNew: () => Promise<void>;
  /** Keeps what is applied and goes on editing component `id` instead. */
  retarget: (id: number | null) => Promise<void>;
}

function componentOf(id: number | null) {
  return id === null
    ? undefined
    : useModelStore.getState().components.find((component) => component.id === id);
}

/** `editing` is the component the form opened on, if it edits one. */
export function useModelingApply(editing: number | null): ModelingApply {
  const session = usePreviewSession('keep');
  const [track, setTrack] = useState<ApplyTrack>(() => startTrack(editing, componentOf(editing)));
  const trackRef = useRef<ApplyTrack>(track);

  const update = (next: ApplyTrack) => {
    trackRef.current = next;
    setTrack(next);
  };

  return {
    track,
    apply: (call) =>
      session.preview(async (replace) => {
        const reply = await call(applyTarget(trackRef.current, replace));
        update(afterApply(trackRef.current, reply));
        useModelStore.getState().select(reply.id);
        return reply;
      }),
    reset: async () => {
      await session.discard();
      update(afterReset(trackRef.current));
    },
    startNew: async () => {
      await session.keep();
      update(afterStartNew());
      const model = useModelStore.getState();
      model.select(null);
      model.setPicked([]);
      await useDocumentStore.getState().clearSelection();
    },
    retarget: async (id) => {
      await session.keep();
      update(startTrack(id, componentOf(id)));
    },
  };
}
