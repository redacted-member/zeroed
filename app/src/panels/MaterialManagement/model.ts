// Pure rules of the Material Management dialog: fresh names and the checks
// on a typed name.

import { machineSetupStrings } from '../../lib/strings/machineSetup';

const text = machineSetupStrings.materials;

/** `base`, or `base 2`, `base 3`... whichever the list does not hold yet. */
export function uniqueName(existing: readonly string[], base: string): string {
  const taken = new Set(existing.map((name) => name.toLowerCase()));
  if (!taken.has(base.toLowerCase())) {
    return base;
  }
  for (let index = 2; ; index += 1) {
    const candidate = `${base} ${index}`;
    if (!taken.has(candidate.toLowerCase())) {
      return candidate;
    }
  }
}

/** Why `name` cannot replace `current`, or null when it can. */
export function nameProblem(
  existing: readonly string[],
  current: string,
  name: string,
): string | null {
  const trimmed = name.trim();
  if (trimmed === '') {
    return text.emptyName;
  }
  if (trimmed !== current && existing.some((other) => other === trimmed)) {
    return text.duplicateName;
  }
  return null;
}

/** The material that becomes active after `name` is deleted: the next one, else the one before. */
export function nextAfterDelete(list: readonly string[], name: string): string {
  const index = list.indexOf(name);
  const rest = list.filter((other) => other !== name);
  if (rest.length === 0) {
    return '';
  }
  return rest[Math.min(Math.max(index, 0), rest.length - 1)] ?? '';
}
