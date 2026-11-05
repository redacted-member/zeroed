// Links the sketch source to the document: a bitmap picked in the 2D view
// becomes the source, the selected closed vectors are the boundary, and the
// sketch lines the settings give are fetched (debounced) and previewed.

import { useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';

import { api } from '../../bridge/formApi';
import type { Path } from '../../bridge/document';
import type { SketchTrace } from '../../bridge/sketch';
import type { Units } from '../../bridge/types';
import { usePreview } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import {
  boundaryIds,
  selectedBitmap,
  sketchIds,
  sketchTrace,
  type SketchSourceState,
} from './model';

const PREVIEW_DELAY_MS = 300;
const NO_PATHS: Path[] = [];

export interface SketchSourceHandle {
  /** Settings for the kernel, or null while one is invalid. */
  trace: SketchTrace | null;
  /** Closed vectors selected for the boundary. */
  boundary: number[];
  /** Objects the toolpath records. */
  ids: number[];
  /** Name of the placed bitmap in use, or null. */
  bitmapName: string | null;
  areas: number | null;
  tracing: boolean;
  failure: string | null;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useSketchSource(
  source: SketchSourceState,
  setSource: Dispatch<SetStateAction<SketchSourceState>>,
  units: Units,
): SketchSourceHandle {
  const selection = useDocumentStore((store) => store.selection.ids);
  const entities = useDocumentStore((store) => store.entities);
  const revision = useDocumentStore((store) => store.revision);
  const picked = selectedBitmap(selection, entities);

  useEffect(() => {
    if (picked !== null) {
      setSource((current) =>
        current.bitmap === picked ? current : { ...current, bitmap: picked },
      );
    }
  }, [picked, setSource]);

  // A placed bitmap that was deleted is no longer the source.
  const placed = source.bitmap === null ? undefined : entities.find((e) => e.id === source.bitmap);
  useEffect(() => {
    if (source.bitmap !== null && entities.length > 0 && placed === undefined) {
      setSource((current) => ({ ...current, bitmap: null }));
    }
  }, [entities.length, placed, setSource, source.bitmap]);

  const boundary = useMemo(() => boundaryIds(selection, entities), [selection, entities]);
  const trace = useMemo(
    () => sketchTrace(source, units, boundary.length),
    [source, units, boundary.length],
  );
  const ids = useMemo(() => sketchIds(source, boundary), [source, boundary]);
  const key = trace === null ? null : JSON.stringify([trace, ids, revision]);

  const [paths, setPaths] = useState<Path[]>(NO_PATHS);
  const [tracing, setTracing] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  useEffect(() => {
    if (trace === null || key === null) {
      return;
    }
    let live = true;
    const timer = window.setTimeout(() => {
      setTracing(true);
      api.cam
        .sketchPreview(trace, ids)
        .then((reply) => {
          if (live) {
            setPaths(reply.paths);
            setFailure(null);
          }
        })
        .catch((error: unknown) => {
          if (live) {
            setPaths(NO_PATHS);
            setFailure(message(error));
          }
        })
        .finally(() => {
          if (live) {
            setTracing(false);
          }
        });
    }, PREVIEW_DELAY_MS);
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
    // The key holds everything the preview depends on.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  const shownPaths = trace === null ? NO_PATHS : paths;
  usePreview(shownPaths);

  return {
    trace,
    boundary,
    ids,
    bitmapName: placed === undefined ? null : (placed.name ?? ''),
    areas: trace === null || failure !== null ? null : paths.length,
    tracing,
    failure: trace === null ? null : failure,
  };
}
