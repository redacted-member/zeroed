// The strings of lib/strings plus the areas only forms use (drawing,
// modeling and toolpaths) and the strings of the start areas that only
// dialogs, forms and panels loaded on demand show (strings/deferred.ts).
// Forms and other lazily loaded modules import `strings` from here; the
// start bundle imports lib/strings, so these load with the first form or
// dialog instead of at start.

import { strings as startStrings } from './strings';
import { assemblyStrings } from './strings/assembly';
import { deferredStrings } from './strings/deferred';
import { jobFormsStrings } from './strings/jobForms';
import { drawingStrings } from './strings/drawing';
import { helpersStrings } from './strings/helpers';
import { modelingStrings } from './strings/modeling';
import { toolpathsStrings } from './strings/toolpaths';
import { whyStrings } from './strings/why';

type Tree = Readonly<Record<string, unknown>>;

/** `a` with `b` merged in: groups present in both are merged the same way. */
type Merged<A, B> = {
  readonly [K in keyof A | keyof B]: K extends keyof B
    ? K extends keyof A
      ? A[K] extends Tree
        ? B[K] extends Tree
          ? Merged<A[K], B[K]>
          : B[K]
        : B[K]
      : B[K]
    : K extends keyof A
      ? A[K]
      : never;
};

function isTree(value: unknown): value is Tree {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function merged<A extends Tree, B extends Tree>(a: A, b: B): Merged<A, B> {
  const out: Record<string, unknown> = { ...a };
  for (const [key, value] of Object.entries(b)) {
    const before = out[key];
    out[key] = isTree(before) && isTree(value) ? merged(before, value) : value;
  }
  return out as Merged<A, B>;
}

export const strings = merged(
  {
    ...startStrings,
    ...assemblyStrings,
    ...jobFormsStrings,
    ...drawingStrings,
    ...modelingStrings,
    ...toolpathsStrings,
    ...whyStrings,
    ...helpersStrings,
  } as const,
  deferredStrings,
);
