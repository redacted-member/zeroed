// A toolpath form's fields as they were when it last calculated: a new form
// of the same kind starts from them's fields keep their
// previous setting once edited. Kept for the session.

import type { Units } from '../bridge/types';

/** A form's fields and the units they were typed in. */
export interface Remembered<S> {
  state: S;
  units: Units;
}

/**
 * The fields a new form starts with: the remembered ones in today's
 * `units` (made `fresh` for a new toolpath, a new name and the like), or
 * the form's first fields when nothing is remembered.
 */
export function startFrom<S>(
  last: Remembered<S> | null,
  units: Units,
  initial: (units: Units) => S,
  convert: (state: S, from: Units, to: Units) => S,
  fresh: (state: S) => S,
): S {
  if (last === null) {
    return initial(units);
  }
  return fresh(last.units === units ? last.state : convert(last.state, last.units, units));
}
