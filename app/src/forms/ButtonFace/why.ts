import { whyStrings } from '../../lib/strings/why';

const why = whyStrings.why;

/**
 * Why the button moving row `index` of `count` rows by `by` is off: the
 * first row cannot go up, the last cannot go down. Undefined when it can.
 */
export function moveWhy(index: number, count: number, by: -1 | 1): string | undefined {
  if (index < 0 || index >= count) {
    return why.pickInList;
  }
  if (by < 0) {
    return index === 0 ? why.atTop : undefined;
  }
  return index === count - 1 ? why.atBottom : undefined;
}
