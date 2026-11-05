// Fetches the path geometry of a few vector entities so a form can draw
// them in its preview. Refetches when the ids or the document change.

import { useEffect, useState } from 'react';

import { api } from '../bridge/api';
import type { Path } from '../bridge/document';
import { useDocumentStore } from '../state/documentStore';

const NONE: ReadonlyMap<number, Path> = new Map();

/** Path of each id that is a vector; other kinds and failures are left out. */
export function useEntityPaths(ids: readonly number[]): ReadonlyMap<number, Path> {
  const revision = useDocumentStore((store) => store.revision);
  const key = [...new Set(ids)].sort((a, b) => a - b).join(',');
  const [result, setResult] = useState<{ key: string; paths: ReadonlyMap<number, Path> }>({
    key: '',
    paths: NONE,
  });

  useEffect(() => {
    if (key === '') {
      return undefined;
    }
    let live = true;
    const wanted = key.split(',').map(Number);
    void Promise.all(
      wanted.map((id) =>
        api.entities
          .get(id)
          .then((entity) => (entity.kind === 'path' ? ([id, entity.path] as const) : null))
          .catch(() => null),
      ),
    ).then((entries) => {
      if (live) {
        const paths = new Map<number, Path>();
        for (const entry of entries) {
          if (entry !== null) {
            paths.set(entry[0], entry[1]);
          }
        }
        setResult({ key, paths });
      }
    });
    return () => {
      live = false;
    };
  }, [key, revision]);

  return key === '' || result.key !== key ? NONE : result.paths;
}
