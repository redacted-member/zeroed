// The Add Component list's model: which components show for a search and
// in what order. Pure, so it can be tested without the kernel.

/** A row of the list: a component and the name it is listed by. */
export interface ComponentEntry {
  id: number;
  label: string;
}

/** The words of a search, lower case. */
function words(query: string): string[] {
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter((word) => word !== '');
}

/**
 * The entries a search keeps: those whose name holds every word typed, in
 * any order and case; every entry for an empty search. The ones picked in
 * the Component Tree come first, the rest keep their order.
 */
export function filterComponents<T extends ComponentEntry>(
  entries: readonly T[],
  query: string,
  picked: readonly number[] = [],
): T[] {
  const wanted = words(query);
  const kept = entries.filter((entry) => {
    const label = entry.label.toLowerCase();
    return wanted.every((word) => label.includes(word));
  });
  const first = kept.filter((entry) => picked.includes(entry.id));
  return [...first, ...kept.filter((entry) => !picked.includes(entry.id))];
}

/** A listed component and its look, which changes with its shape. */
export interface ThumbnailWanted {
  id: number;
  look?: string | undefined;
}

/**
 * The thumbnails to load, in list order: listed components without one
 * yet, or whose shape changed since theirs was made (`loaded` holds the
 * look each thumbnail was made for).
 */
export function staleThumbnails(
  wanted: readonly ThumbnailWanted[],
  loaded: ReadonlyMap<number, string>,
): number[] {
  return wanted.filter((w) => loaded.get(w.id) !== (w.look ?? '')).map((w) => w.id);
}
